import type { Report } from '../../api/reports';
import { money } from '../../shared/format';

const PIE_COLORS = ['#5c8ed8', '#d783a3', '#61a68d', '#e3a85d', '#8d78ba', '#6fa7b8', '#c77b5b'];

function minor(value: string) {
  const negative = value.trim().startsWith('-');
  const [integer, decimal = ''] = value.replace(/^-/, '').split('.');
  const result = BigInt(`${integer || '0'}${decimal.padEnd(2, '0').slice(0, 2)}`);
  return negative ? -result : result;
}

function decimal(value: bigint) {
  const negative = value < 0n;
  const absolute = negative ? -value : value;
  const text = absolute.toString().padStart(3, '0');
  const result = `${text.slice(0, -2)}.${text.slice(-2)}`;
  return negative ? `-${result}` : result;
}

function height(value: bigint, max: bigint) {
  return max > 0n && value > 0n ? Math.max(2, Number((value * 100n) / max)) : 0;
}

function yearlyRows(report: Report | null | undefined) {
  const rows = new Map<string, { income: bigint; expense: bigint }>();
  for (const row of report?.trend ?? []) {
    const year = row.month.slice(0, 4);
    const current = rows.get(year) ?? { income: 0n, expense: 0n };
    current.income += minor(row.income);
    current.expense += minor(row.expense);
    rows.set(year, current);
  }
  return [...rows.entries()].map(([label, values]) => ({ label, ...values }));
}

function VerticalChart({
  report,
  yearly = false,
  ariaLabel,
}: {
  report: Report | null | undefined;
  yearly?: boolean;
  ariaLabel: string;
}) {
  const rows = yearly
    ? yearlyRows(report)
    : (report?.trend ?? []).map((row) => ({
        label: row.month.slice(5),
        income: minor(row.income),
        expense: minor(row.expense),
      }));
  const max = rows.reduce(
    (largest, row) =>
      row.income > largest ? row.income : row.expense > largest ? row.expense : largest,
    0n,
  );
  return (
    <section className="analytics-section" aria-label={ariaLabel}>
      <h3>{yearly ? '近6年收支' : '近12个月收支'}</h3>
      <p className="field-hint">蓝色为收入，粉色为支出，空月份也会保留。</p>
      <div
        className={`vertical-chart ${yearly ? 'yearly-chart' : 'monthly-chart'}`}
        role="img"
        aria-label={ariaLabel}
      >
        {rows.map((row) => (
          <div className="column-item" key={row.label}>
            <div className="column-bars">
              <i
                className="column-income"
                style={{ height: `${height(row.income, max)}%` }}
                title={`收入 ¥${money(decimal(row.income))}`}
              />
              <i
                className="column-expense"
                style={{ height: `${height(row.expense, max)}%` }}
                title={`支出 ¥${money(decimal(row.expense))}`}
              />
            </div>
            <span>{yearly ? row.label : `${row.label}月`}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

export function ReportCharts({
  report,
  monthly,
  yearly,
}: {
  report: Report;
  monthly?: Report | null;
  yearly?: Report | null;
}) {
  const total = report.categories.reduce((sum, row) => sum + minor(row.expense), 0n);
  let cursor = 0;
  const pieSegments = report.categories.map((row, index) => {
    const share = total > 0n ? Number((minor(row.expense) * 10000n) / total) / 100 : 0;
    const start = cursor;
    cursor += share;
    return { ...row, share, start, end: cursor, color: PIE_COLORS[index % PIE_COLORS.length] };
  });
  const gradient = pieSegments.length
    ? `conic-gradient(${pieSegments
        .map((segment) => `${segment.color} ${segment.start}% ${segment.end}%`)
        .join(', ')})`
    : 'conic-gradient(#e5ebf5 0 100%)';
  return (
    <>
      <section className="analytics-section" aria-label="分类支出饼图">
        <h3>分类支出饼图</h3>
        <p className="field-hint">按当前筛选周期显示支出分类占比。</p>
        {!pieSegments.length ? (
          <p>本期暂无分类支出。</p>
        ) : (
          <div className="pie-layout">
            <div className="report-pie" style={{ background: gradient }} aria-hidden="true" />
            <div className="pie-legend">
              {pieSegments.map((row) => (
                <div className="pie-legend-row" key={row.id ?? 'uncategorized'}>
                  <span>
                    <i style={{ background: row.color }} />
                    {row.name}
                  </span>
                  <strong>{row.share.toFixed(0)}%</strong>
                </div>
              ))}
            </div>
          </div>
        )}
      </section>
      <VerticalChart report={monthly} ariaLabel="近12个月收支柱状图" />
      <VerticalChart report={yearly} yearly ariaLabel="近6年收支柱状图" />
    </>
  );
}
