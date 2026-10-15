import type { Account } from '../../api/accounts';
import { LoadState } from '../../components/LoadState';
import { money } from '../../shared/format';
import type { RecurringResource } from './useRecurring';
export function RecurringReminders({
  state,
  accounts,
  onPending,
  onManage,
}: {
  state: RecurringResource;
  accounts: Account[];
  onPending: () => void;
  onManage: () => void;
}) {
  return (
    <section className="recurring-reminders" aria-label="周期账单提醒">
      <div className="analytics-row">
        <h2>周期账单提醒</h2>
        <button className="text-button" onClick={onManage}>
          管理周期账单
        </button>
      </div>
      <p className="field-hint">未来 7 天与逾期待确认账单；确认后才计入收支。</p>
      <LoadState {...state} />
      {state.data?.reminders.map((r) => (
        <article key={r.template_id + r.due_date}>
          <div className="analytics-row">
            <strong>{r.merchant || r.description || '周期账单'}</strong>
            <strong>¥ {money(r.amount)}</strong>
          </div>
          <p>
            {r.due_date} · {accounts.find((a) => a.id === r.account_id)?.name ?? '原平台'} ·{' '}
            {r.transaction_id ? (r.overdue ? '逾期待确认' : '到期待确认') : '即将到期'}
          </p>
          {r.transaction_id ? (
            <button className="text-button" onClick={onPending}>
              查看待确认
            </button>
          ) : (
            <span className="field-hint">到期后生成候选</span>
          )}
        </article>
      ))}
      {state.data?.reminders.length === 0 && (
        <p className="field-hint">暂无近期到期或逾期待确认的周期账单。</p>
      )}
      {state.data?.has_more && (
        <button className="text-button" disabled={state.loading} onClick={state.reload}>
          继续生成到期候选
        </button>
      )}
    </section>
  );
}
