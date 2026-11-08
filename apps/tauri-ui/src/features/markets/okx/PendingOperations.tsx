import { useEffect, useState } from 'react';
import type { Json, OkxEnvironment } from '../../../api/okxAdvanced';

const KEY = 'meowm.okx.pending-operations.v1';
const CHANGED = 'meowm-okx-pending-changed';
const IDENTIFIERS = new Set([
  'instId',
  'instIdCode',
  'ordId',
  'clOrdId',
  'algoId',
  'algoClOrdId',
  'reqId',
  'transId',
  'wdId',
  'quoteId',
  'rfqId',
  'clientId',
]);

type PendingState = 'pending' | 'unknown' | 'success' | 'rejected' | 'partial_failure' | 'closed';
type PendingOperation = {
  request_id: string;
  title: string;
  environment: OkxEnvironment;
  state: PendingState;
  identifiers: string[];
};

function read(): PendingOperation[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(KEY) ?? '[]');
    if (!Array.isArray(value)) return [];
    return value
      .filter(
        (item): item is PendingOperation =>
          !!item &&
          typeof item === 'object' &&
          typeof item.request_id === 'string' &&
          typeof item.title === 'string' &&
          (item.environment === 'demo' || item.environment === 'production') &&
          ['pending', 'unknown', 'success', 'rejected', 'partial_failure', 'closed'].includes(
            item.state,
          ) &&
          Array.isArray(item.identifiers) &&
          item.identifiers.every((identifier: unknown) => typeof identifier === 'string'),
      )
      .slice(-50);
  } catch {
    return [];
  }
}

export function hasPendingOperations(): boolean {
  return read().length > 0;
}

function write(operations: PendingOperation[]) {
  localStorage.setItem(KEY, JSON.stringify(operations));
  window.dispatchEvent(new Event(CHANGED));
}

function identifiers(value: Json): string[] {
  const found: string[] = [];
  const visit = (entry: Json) => {
    if (Array.isArray(entry)) {
      entry.slice(0, 20).forEach(visit);
      return;
    }
    if (!entry || typeof entry !== 'object') return;
    for (const [key, item] of Object.entries(entry)) {
      if (IDENTIFIERS.has(key) && (typeof item === 'string' || typeof item === 'number')) {
        const text = String(item);
        if (/^[A-Za-z0-9_.:-]{1,128}$/.test(text)) found.push(`${key}: ${text}`);
      } else if (item && typeof item === 'object') visit(item);
    }
  };
  visit(value);
  return [...new Set(found)].slice(0, 30);
}

export function rememberOperation(
  requestId: string,
  title: string,
  environment: OkxEnvironment,
  params: Json,
) {
  const existing = read();
  if (existing.length >= 50) throw new Error('待核对操作已满，请先核对已有记录。');
  write([
    ...existing,
    {
      request_id: requestId,
      title,
      environment,
      state: 'pending',
      identifiers: identifiers(params),
    },
  ]);
}

export function updateOperation(requestId: string, state: PendingState | 'remove') {
  const existing = read();
  if (!existing.some((item) => item.request_id === requestId)) return;
  write(
    state === 'remove'
      ? existing.filter((item) => item.request_id !== requestId)
      : existing.map((item) => (item.request_id === requestId ? { ...item, state } : item)),
  );
}

export function PendingOperations() {
  const [operations, setOperations] = useState(read);
  useEffect(() => {
    const update = () => setOperations(read());
    window.addEventListener(CHANGED, update);
    window.addEventListener('storage', update);
    return () => {
      window.removeEventListener(CHANGED, update);
      window.removeEventListener('storage', update);
    };
  }, []);
  if (!operations.length) return null;
  return (
    <section className="okx-unknown-order" aria-label="待核对操作">
      <strong>待核对操作</strong>
      <p>请在对应 OKX 账户核对操作记录后再提交相同操作。本地请求编号不一定是交易所订单号。</p>
      {operations.map((item) => (
        <div key={item.request_id} className="okx-pending-operation">
          <strong>{item.title}</strong>
          <p>
            {item.environment === 'demo' ? '模拟盘' : '实盘'} ·{' '}
            {item.state === 'success'
              ? '请求已完成'
              : item.state === 'rejected'
                ? '请求被拒绝'
                : item.state === 'partial_failure'
                  ? '部分操作失败'
                  : item.state === 'closed'
                    ? '连接已关闭'
                    : '结果待核对'}
          </p>
          <p>本地请求编号：{item.request_id}</p>
          {item.identifiers.length > 0 && <p>{item.identifiers.join(' · ')}</p>}
          <button
            className="button"
            type="button"
            onClick={() => updateOperation(item.request_id, 'remove')}
          >
            我已核对
          </button>
        </div>
      ))}
    </section>
  );
}
