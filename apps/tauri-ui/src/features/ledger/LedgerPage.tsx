import type { DayView } from '../../api/ledger';
import { money } from '../../shared/format';
export function LedgerPage({
  day,
  allAccounts = false,
  onReference,
  onClear,
}: {
  day: DayView;
  allAccounts?: boolean;
  onReference: () => void;
  onClear: () => void;
}) {
  const cards = [
    ['当日收入', day.income, 'income', '↙'],
    ['当日支出', day.expense, 'expense', '↗'],
    ['当日净额', day.net, 'net', '≈'],
  ];
  return (
    <>
      <div className="summary-grid">
        {cards.map(([label, value, kind, symbol]) => (
          <section key={kind} className={'summary-card ' + kind}>
            <div className="summary-title">
              <span>{label}</span>
              <span className="summary-icon">{symbol}</span>
            </div>
            <div className="summary-amount">
              <span className="currency">¥</span>
              <span>{money(value)}</span>
            </div>
          </section>
        ))}
      </div>
      {!allAccounts && (
        <section className="balance-panel">
          <div className="balance-info">
            <span className="balance-label">日末余额</span>
            <strong className="balance-value">
              {day.balance === null ? '尚未设置' : `¥ ${money(day.balance)}`}
            </strong>
            <p className="balance-hint">
              {day.reference
                ? `基于 ${day.reference.date} 的参考余额 ¥ ${money(day.reference.amount)}`
                : '设置参考余额后，即可查看每日结余。'}
            </p>
          </div>
          <div className="balance-actions">
            <button className="text-button" onClick={onReference}>
              {day.reference ? '调整参考余额' : '设置参考余额'}
            </button>
            {day.reference && (
              <button className="text-button muted-button" onClick={onClear}>
                清除
              </button>
            )}
          </div>
        </section>
      )}
    </>
  );
}
