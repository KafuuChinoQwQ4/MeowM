import type { Account } from '../../api/accounts';
import type { Budget } from '../../api/budgets';
import { money, today } from '../../shared/format';
export function BudgetCard({
  budget: b,
  accounts,
  onEdit,
  onArchive,
  busy = false,
}: {
  budget: Budget;
  accounts: Account[];
  onEdit?: () => void;
  onArchive?: () => void;
  busy?: boolean;
}) {
  const percent = BigInt(b.percent);
  const date = today();
  const visualPercent = Number(percent > 100n ? 100n : percent);
  return (
    <article className={'budget-card' + (b.exceeded ? ' exceeded' : '')}>
      <div className="analytics-row">
        <strong>
          {b.category_name} · {accounts.find((a) => a.id === b.account_id)?.name ?? '全部平台'}
        </strong>
        <span>
          {!b.enabled
            ? '已停用'
            : date < b.from_date
              ? '尚未开始'
              : date >= b.until_date
                ? '周期已结束'
                : b.exceeded
                  ? '已超支'
                  : percent >= 100n
                    ? '额度已用尽'
                    : percent >= 80n
                      ? '接近额度'
                      : '进行中'}
        </span>
      </div>
      <p className="field-hint">
        {b.from_date} 至 {b.until_date}（不含）
      </p>
      <div
        className="chart-track"
        role="progressbar"
        aria-label={`${b.category_name}预算使用比例`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={visualPercent}
        aria-valuetext={`${b.percent}%`}
      >
        <i style={{ width: `${visualPercent}%` }} />
      </div>
      <p>
        已用 ¥{money(b.spent)} / ¥{money(b.amount)} · {b.percent}%
      </p>
      <p>剩余 ¥{money(b.remaining)}</p>
      {onEdit && (
        <div className="analytics-actions">
          <button className="text-button" disabled={busy} onClick={onEdit}>
            编辑预算
          </button>
          {b.enabled && onArchive && (
            <button className="text-button" disabled={busy} onClick={onArchive}>
              停用预算
            </button>
          )}
        </div>
      )}
    </article>
  );
}
