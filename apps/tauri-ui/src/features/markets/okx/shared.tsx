import { useEffect, useRef, useState } from 'react';
import { displayAmount } from '../../../shared/format';

export function okxError(cause: unknown): string {
  const code = cause instanceof Error ? cause.message : String(cause);
  const labels: Record<string, string> = {
    invalid_credentials: '请检查欧易只读凭据是否有效。',
    permission_denied: 'API Key 缺少读取权限。',
    secure_storage_unavailable: '系统安全凭据存储不可用。',
    rate_limited: 'OKX 请求频率受限，请稍后重试。',
    timeout: 'OKX 请求超时，请重试。',
    network_failure: '无法连接 OKX，请检查网络后重试。',
    invalid_response: 'OKX 返回的数据无法验证，请重试。',
    invalid_request: '请检查交易对、时间范围或分页参数；账单限最近 7 天，归档和成交限最近 3 个月。',
    crypto_storage_error: '无法读取或保存本机加密资产记录。',
    crypto_review_invalid: '记录尚未完成或已变更，请刷新后核对。',
    crypto_session_changed: '账户或账本会话已变更，本次结果未保存，请重新读取。',
  };
  return labels[code] ?? 'OKX 操作失败，请重试。';
}
export async function safe<T>(promise: Promise<T>): Promise<T> {
  try {
    return await promise;
  } catch (cause) {
    throw new Error(okxError(cause));
  }
}
export function useAction() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const active = useRef(true);
  const running = useRef(false);
  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);
  const run = async (action: () => Promise<void>) => {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setError('');
    try {
      await action();
    } catch (cause) {
      if (active.current) setError(okxError(cause));
    } finally {
      running.current = false;
      if (active.current) setBusy(false);
    }
  };
  return { busy, error, run, active };
}
export function Amount({ value, unit = '' }: { value: string | null; unit?: string }) {
  return <>{value === null ? '—' : `${displayAmount(value)}${unit ? ` ${unit}` : ''}`}</>;
}
export function ReadState({
  loading,
  error,
  reload,
}: {
  loading: boolean;
  error: string;
  reload: () => void;
}) {
  return (
    <>
      {loading && <p role="status">正在读取 OKX 数据…</p>}
      {error && (
        <p role="alert">
          {error}{' '}
          <button className="button" onClick={reload}>
            重试
          </button>
        </p>
      )}
    </>
  );
}
export function ConfigureHint({ onConfigure }: { onConfigure?: () => void } = {}) {
  return (
    <p className="field-hint">
      请配置 OKX 只读凭据以读取账户数据。{' '}
      {onConfigure && (
        <button className="text-button" type="button" onClick={onConfigure}>
          配置私有凭据
        </button>
      )}
    </p>
  );
}
export function time(value: number): string {
  return new Date(value).toLocaleString('zh-CN');
}

// Only SVG coordinates use Number. All quantities and totals retain decimal precision.
export function sumDecimals(values: string[]): string {
  const parts = values.map((v) => {
    const [whole, fraction = ''] = v.replace(/^-/, '').split('.');
    return { negative: v.startsWith('-'), whole, fraction };
  });
  const scale = Math.max(0, ...parts.map((p) => p.fraction.length));
  const total = parts.reduce(
    (sum, p) => sum + (p.negative ? -1n : 1n) * BigInt(p.whole + p.fraction.padEnd(scale, '0')),
    0n,
  );
  const digits = (total < 0n ? -total : total).toString().padStart(scale + 1, '0');
  const result = scale
    ? `${digits.slice(0, -scale)}.${digits.slice(-scale)}`.replace(/0+$/, '').replace(/\.$/, '')
    : digits;
  return `${total < 0n ? '-' : ''}${result}`;
}
