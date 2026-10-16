import { useCallback, useRef, useState } from 'react';
import { reconciliationApi } from '../../api/reconciliation';
import { Field, FormError } from '../../components/Fields';
import { LoadState } from '../../components/LoadState';
import { errorText } from '../../shared/errors';
import { useResource } from '../../shared/useResource';
import { SnapshotEntries } from './SnapshotEntries';
import { localTime, statusLabel } from './display';
const eventLabels: Record<string, string> = {
  started: '创建快照',
  entry_reviewed: '核对交易',
  entry_unreviewed: '撤回核对',
  adjusted: '生成余额调整',
  closed: '关闭对账',
  reopened: '重新打开',
};
export function SessionDetail({
  id,
  revision,
  onChanged,
  onPending,
  onEntry,
}: {
  id: string;
  revision: number;
  onChanged: () => void;
  onPending: () => void;
  onEntry: (id: string) => void;
}) {
  const loader = useCallback(() => reconciliationApi.summary(id), [id, revision]);
  const state = useResource(loader);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(''),
    [reason, setReason] = useState('');
  const running = useRef(false);
  async function act(operation: () => Promise<unknown>) {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setError('');
    try {
      await operation();
      onChanged();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      running.current = false;
      setBusy(false);
    }
  }
  const s = state.data;
  if (!s) return <LoadState {...state} />;
  const r = s.record,
    closed = r.status === 'closed',
    all = s.entries.every((e) => e.reviewed);
  const resolved = !!r.adjustment_id || r.difference === '0.00';
  const disabled = busy || state.loading || !r.snapshot_ready;
  return (
    <section className="reconciliation-detail" aria-label="对账详情">
      <div className="analytics-row">
        <h3>对账快照</h3>
        <span>{statusLabel[r.status]}</span>
      </div>
      <button
        className="button button-secondary"
        disabled={busy || state.loading}
        onClick={state.reload}
      >
        刷新会话
      </button>
      <p>截止 {localTime(r.cutoff)}</p>
      <dl className="reconciliation-totals">
        <div>
          <dt>账本余额</dt>
          <dd>¥{r.expected}</dd>
        </div>
        <div>
          <dt>实际余额</dt>
          <dd>¥{r.actual}</dd>
        </div>
        <div>
          <dt>快照差额</dt>
          <dd>¥{r.difference}</dd>
        </div>
        <div>
          <dt>未处理差额</dt>
          <dd>¥{resolved ? '0.00' : r.difference}</dd>
        </div>
      </dl>
      <p className="field-hint">
        快照参考余额 ¥{r.reference}（{localTime(r.reference_at)}）。
        {r.period_from
          ? `流水期间从 ${localTime(r.period_from)} 之后起算。`
          : '展示截止时刻之前的全部关联流水。'}
        参考时刻之前的流水已含在参考余额中。
      </p>
      {!r.snapshot_ready && <p role="alert">旧会话缺少完整快照，请新建对账。</p>}
      <p className="field-hint">
        快照中 {s.pending_count}{' '}
        条待确认；待确认和已忽略记录不计入余额。勾选只表示核对，不会确认入账。
      </p>
      {s.pending_count > 0 && (
        <button className="button button-secondary" onClick={onPending} disabled={busy}>
          前往待确认处理
        </button>
      )}
      <SnapshotEntries
        entries={s.entries}
        disabled={disabled || closed}
        onReview={(entry, reviewed) => void act(() => reconciliationApi.review(r, entry, reviewed))}
        onEntry={onEntry}
      />
      <FormError message={error} />
      {r.adjustment_id && (
        <div className="reconciliation-receipt">
          <strong>已生成余额调整</strong>
          <p>原始快照差额保留。调整计入余额，不计入收入或支出。</p>
          <button
            className="button button-secondary"
            disabled={busy}
            onClick={() => onEntry(r.adjustment_id!)}
          >
            查看调整交易
          </button>
        </div>
      )}
      {!closed && !resolved && (
        <div className="reconciliation-adjustment">
          <Field
            label="调整原因"
            maxLength={200}
            value={reason}
            disabled={disabled}
            onChange={(e) => setReason(e.target.value)}
          />
          <p className="field-hint">
            核实遗漏和待确认记录后，才生成 ¥{r.difference}{' '}
            的余额调整。若期间余额或参考余额已变化，请新建对账。
          </p>
          <button
            className="button button-primary"
            disabled={disabled || !all || !reason.trim()}
            onClick={() => {
              if (
                window.confirm(
                  `将生成 ¥${r.difference} 的余额调整，使截止余额变为 ¥${r.actual}。原因：${reason.trim()}。确认？`,
                )
              )
                void act(() => reconciliationApi.adjust(r, reason.trim()));
            }}
          >
            确认生成余额调整
          </button>
        </div>
      )}
      <div className="reconciliation-actions">
        {closed ? (
          <button
            className="button button-secondary"
            disabled={disabled}
            onClick={() => void act(() => reconciliationApi.reopen(r))}
          >
            重新打开对账
          </button>
        ) : (
          <button
            className="button button-primary"
            disabled={disabled || !all || !resolved}
            onClick={() => void act(() => reconciliationApi.close(r))}
          >
            关闭对账
          </button>
        )}
      </div>
      <details>
        <summary>对账审计记录（{s.events.length}）</summary>
        <ol className="reconciliation-audit">
          {s.events.map((e, i) => (
            <li key={i}>
              {localTime(e.occurred_at)} · {eventLabels[e.action] ?? e.action}
              {e.reason && (
                <p>
                  {e.action === 'adjusted' ? '原因：' : '记录：'}
                  {e.reason}
                </p>
              )}
            </li>
          ))}
        </ol>
      </details>
    </section>
  );
}
