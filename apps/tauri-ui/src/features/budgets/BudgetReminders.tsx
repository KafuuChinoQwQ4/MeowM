import type { Budget, BudgetReminder } from '../../api/budgets';
import { money } from '../../shared/format';
const labels = { warning: '预算接近额度', exhausted: '预算额度已用尽', exceeded: '预算已超支' };
export function BudgetReminders({
  reminders,
  budgets,
}: {
  reminders: BudgetReminder[];
  budgets: Budget[];
}) {
  return (
    <div aria-live="polite">
      {reminders.map((r) => (
        <p className="budget-reminder" key={r.id}>
          {labels[r.level]} · {budgets.find((b) => b.id === r.budget_id)?.category_name ?? '预算'}
          ：已用 ¥{money(r.spent)} / ¥{money(r.amount)}
        </p>
      ))}
    </div>
  );
}
