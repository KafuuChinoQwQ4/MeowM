import type { Account } from '../../api/accounts';
import type { RecurringTemplate } from '../../api/recurring';
import { money } from '../../shared/format';
const units = { weekly: '周', monthly: '月', yearly: '年' };
export function RecurringCard({
  template: t,
  accounts,
  busy,
  onEdit,
  onToggle,
  onSkip,
  onHistory,
}: {
  template: RecurringTemplate;
  accounts: Account[];
  busy: boolean;
  onEdit: () => void;
  onToggle: () => void;
  onSkip: () => void;
  onHistory: () => void;
}) {
  return (
    <article className="recurring-card">
      <div className="analytics-row">
        <strong>{t.merchant || t.description || '周期账单'}</strong>
        <strong>
          {t.kind === 'income' ? '收入' : '支出'} ¥ {money(t.amount)}
        </strong>
      </div>
      <p>
        {accounts.find((a) => a.id === t.account_id)?.name ?? '原平台'} · {t.category_name} · 每{' '}
        {t.interval} {units[t.cadence]}
      </p>
      <p>
        {t.ended ? '已结束' : t.enabled ? '启用中' : '已暂停'} ·{' '}
        {t.ended ? '后续计划日' : '下次到期'} {t.next_due_date}
        {t.end_date && ` · 截止 ${t.end_date}`}
      </p>
      {t.description && <p className="field-hint">{t.description}</p>}
      <div className="pending-actions">
        <button className="text-button" disabled={busy} onClick={onEdit}>
          编辑未来账单
        </button>
        <button className="text-button" disabled={busy || t.ended} onClick={onToggle}>
          {t.enabled ? '暂停' : '恢复'}
        </button>
        <button className="text-button" disabled={busy || !t.enabled || t.ended} onClick={onSkip}>
          跳过下次
        </button>
        <button className="text-button" disabled={busy} onClick={onHistory}>
          查看实例
        </button>
      </div>
    </article>
  );
}
