import type { ReconciliationEntry } from '../../api/reconciliation';
import { localTime } from './display';
const labels: Record<string, string> = {
  income: '收入',
  expense: '支出',
  transfer: '转账',
  refund: '退款',
  adjustment: '余额调整',
  pending: '待确认',
  confirmed: '已确认',
  ignored: '已忽略',
};
export function SnapshotEntries({
  entries,
  disabled,
  onReview,
  onEntry,
}: {
  entries: ReconciliationEntry[];
  disabled: boolean;
  onReview: (id: string, checked: boolean) => void;
  onEntry: (id: string) => void;
}) {
  return (
    <section aria-label="快照交易">
      <h4>
        逐项核对（{entries.filter((e) => e.reviewed).length}/{entries.length}）
      </h4>
      {!entries.length && <p className="field-hint">该期间没有关联到此平台的交易。</p>}
      {entries.map((e) => (
        <article className="reconciliation-entry" key={e.id}>
          <div className="analytics-row">
            <strong>{e.merchant || labels[e.kind] || e.kind}</strong>
            <span>¥{e.delta}</span>
          </div>
          <p>{e.description}</p>
          <small>
            {localTime(e.occurred_at)} · {labels[e.kind]} · {labels[e.status]}
          </small>
          <div className="reconciliation-actions">
            <label className="reconciliation-check">
              <input
                type="checkbox"
                aria-label={'已核对：' + (e.merchant || labels[e.kind] || e.kind)}
                checked={e.reviewed}
                disabled={disabled}
                onChange={(event) => onReview(e.id, event.target.checked)}
              />
              已核对
            </label>
            <button
              className="button button-secondary"
              disabled={disabled}
              onClick={() => onEntry(e.id)}
            >
              查看当前记录
            </button>
          </div>
        </article>
      ))}
    </section>
  );
}
