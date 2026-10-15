import type { Report } from '../../api/reports';
import { money } from '../../shared/format';
// Keep monetary values as BigInt; convert only a bounded visual percentage.
function minor(value: string) {
  return BigInt(value.replace('.', ''));
}
function width(value: string, max: bigint) {
  const amount = minor(value);
  return max > 0n && amount > 0n ? Number((amount * 100n) / max) : 0;
}
export function ReportCharts({ report }: { report: Report }) {
  const max = report.trend.reduce((acc, row) => {
    const income = minor(row.income),
      expense = minor(row.expense);
    return [acc, income, expense].reduce((a, b) => (a > b ? a : b));
  }, 0n);
  return (
    <>
      <section className="analytics-section" aria-label="月度收支趋势">
        <h3>月度收支趋势</h3>
        <p className="field-hint">收入为蓝色，支出为粉色；退款抵减支出。</p>
        {report.trend.map((row) => (
          <div className="trend-row" key={row.month}>
            <strong>{row.month}</strong>
            <div>
              <span>收入 ¥{money(row.income)}</span>
              <div className="chart-track">
                <i style={{ width: `${width(row.income, max)}%` }} />
              </div>
              <span>支出 ¥{money(row.expense)}</span>
              <div className="chart-track expense">
                <i style={{ width: `${width(row.expense, max)}%` }} />
              </div>
            </div>
          </div>
        ))}
      </section>
      <section className="analytics-section" aria-label="分类支出占比">
        <h3>分类支出占比</h3>
        <p className="field-hint">占比按抵减退款后的正支出计算。</p>
        {!report.categories.length && <p>本期暂无分类支出。</p>}
        {report.categories.map((row) => (
          <div className="category-share" key={row.id ?? 'uncategorized'}>
            <div>
              <span>{row.name}</span>
              <span>
                ¥{money(row.expense)} · {row.share_percent}%
              </span>
            </div>
            <div className="chart-track expense">
              <i style={{ width: `${row.share_percent}%` }} />
            </div>
          </div>
        ))}
      </section>
    </>
  );
}
