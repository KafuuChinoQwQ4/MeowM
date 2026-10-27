import { useState } from 'react';
import type { Report } from '../../api/reports';
import { money, today } from '../../shared/format';

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

function axisStep(max: bigint) {
  if (max <= 0n) return 2500n;
  const minimum = (max + 3n) / 4n;
  const unit = 10n ** BigInt(minimum.toString().length - 1);
  return [
    unit,
    (unit * 3n + 1n) / 2n,
    unit * 2n,
    (unit * 5n + 1n) / 2n,
    unit * 3n,
    unit * 4n,
    unit * 5n,
    unit * 6n,
    unit * 8n,
    unit * 10n,
  ].find((step) => step >= minimum)!;
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
  return [...rows.entries()].map(([label, values]) => ({
    label,
    period: `${label}年`,
    ...values,
  }));
}

type TrendRow = { label: string; period: string; income: bigint; expense: bigint };

function TrendPie({
  rows,
  metric,
  title,
  yearly,
}: {
  rows: TrendRow[];
  metric: 'income' | 'expense';
  title: string;
  yearly: boolean;
}) {
  const name = metric === 'income' ? '收入' : '支出';
  const values = rows.map((row, index) => ({
    label: yearly ? `${row.label}年` : `${row.label}月`,
    value: row[metric],
    color: PIE_COLORS[index % PIE_COLORS.length],
  }));
  const positive = values.filter((row) => row.value > 0n);
  const negatives = values.filter((row) => row.value < 0n);
  const total = positive.reduce((sum, row) => sum + row.value, 0n);
  let cursor = 0n;
  const segments = positive.map((row) => {
    const start = Number((cursor * 10000n) / total) / 100;
    cursor += row.value;
    const end = Number((cursor * 10000n) / total) / 100;
    return { ...row, start, end, share: Number((row.value * 10000n) / total) / 100 };
  });
  return (
    <>
      <p className="field-hint">
        按各{yearly ? '年' : '月'}正{name}金额显示占比，零金额不显示扇区。
      </p>
      {segments.length ? (
        <>
          <p className="pie-total">
            正{name}合计 ¥{money(decimal(total))}
          </p>
          <div className="pie-layout">
            <div
              className="report-pie"
              role="img"
              aria-label={`${title}${name}占比饼图`}
              style={{
                background: `conic-gradient(${segments.map((row) => `${row.color} ${row.start}% ${row.end}%`).join(', ')})`,
              }}
            />
            <div className="pie-legend">
              {segments.map((row) => (
                <div className="pie-legend-row" key={row.label}>
                  <span>
                    <i style={{ background: row.color }} />
                    {row.label}
                  </span>
                  <strong>
                    ¥{money(decimal(row.value))} <small>{row.share.toFixed(1)}%</small>
                  </strong>
                </div>
              ))}
            </div>
          </div>
        </>
      ) : (
        <p>暂无正{name}可绘制饼图。</p>
      )}
      {negatives.length > 0 && (
        <div className="pie-negatives">
          <p className="field-hint">以下{name}净额为负（如退款抵减），单独列出，不计入饼图占比：</p>
          {negatives.map((row) => (
            <div className="analytics-row" key={row.label}>
              <span>{row.label}</span>
              <strong>¥{money(decimal(row.value))}</strong>
            </div>
          ))}
        </div>
      )}
    </>
  );
}

function TrendChart({
  report,
  yearly = false,
}: {
  report: Report | null | undefined;
  yearly?: boolean;
}) {
  const [type, setType] = useState<'bar' | 'pie'>('bar');
  const [metric, setMetric] = useState<'income' | 'expense'>('expense');
  const [selected, setSelected] = useState<string | null>(null);
  const [half, setHalf] = useState<'first' | 'second'>(() =>
    Number(today().slice(5, 7)) <= 6 ? 'first' : 'second',
  );
  const year = report?.trend[0]?.month.slice(0, 4) ?? today().slice(0, 4);
  const title = yearly ? '近6年收支' : '月度收支';
  const period = yearly ? '近6年' : `${year}年`;
  const allRows = yearly
    ? yearlyRows(report)
    : (report?.trend ?? []).map((row) => ({
        label: row.month.slice(5),
        period: `${row.month.slice(0, 4)}年${row.month.slice(5)}月`,
        income: minor(row.income),
        expense: minor(row.expense),
      }));
  const rows = yearly
    ? allRows
    : allRows.filter((row) => (half === 'first' ? Number(row.label) <= 6 : Number(row.label) > 6));
  const max = rows
    .flatMap((row) => [row.income, row.expense])
    .reduce((largest, value) => (value > largest ? value : largest), 0n);
  const step = axisStep(max);
  const axisMax = step * 4n;
  const selection = rows.find((row) => row.period === selected);
  return (
    <section className="analytics-section" aria-label={`${title}图表`}>
      <div className="chart-heading">
        <h3>{yearly ? title : `${year}年${title}`}</h3>
        <div className="chart-switch" role="group" aria-label={`${title}图表类型`}>
          <button type="button" aria-pressed={type === 'bar'} onClick={() => setType('bar')}>
            柱形图
          </button>
          <button type="button" aria-pressed={type === 'pie'} onClick={() => setType('pie')}>
            饼图
          </button>
        </div>
      </div>
      {type === 'pie' ? (
        <>
          <label className="chart-metric">
            饼图统计
            <select
              aria-label="饼图统计"
              value={metric}
              onChange={(event) => setMetric(event.target.value as 'income' | 'expense')}
            >
              <option value="expense">支出</option>
              <option value="income">收入</option>
            </select>
          </label>
          <TrendPie rows={allRows} metric={metric} title={period} yearly={yearly} />
        </>
      ) : (
        <>
          {!yearly && (
            <div className="chart-half-switch" role="group" aria-label="月份范围" data-half={half}>
              <span className="chart-half-slider" aria-hidden="true" />
              <button
                type="button"
                aria-pressed={half === 'first'}
                onClick={() => setHalf('first')}
              >
                上半年
              </button>
              <button
                type="button"
                aria-pressed={half === 'second'}
                onClick={() => setHalf('second')}
              >
                下半年
              </button>
            </div>
          )}
          <p className="field-hint">
            蓝色为收入，粉色为支出，空{yearly ? '年份' : '月份'}也会保留。
          </p>
          <div className="bar-chart-layout">
            <div className="chart-axis" aria-label="金额刻度（元）">
              {[4n, 3n, 2n, 1n, 0n].map((level) => (
                <span key={level.toString()}>¥{money(decimal(step * level))}</span>
              ))}
            </div>
            <div
              className={`vertical-chart ${yearly ? 'yearly-chart' : 'monthly-chart'}`}
              role="group"
              aria-label={`${title}柱状图`}
            >
              {rows.map((row) => (
                <button
                  type="button"
                  className="column-item"
                  key={row.period}
                  aria-label={`查看${row.period}收支金额`}
                  aria-pressed={selected === row.period}
                  onClick={() => setSelected(row.period)}
                >
                  <div className="column-bars" aria-hidden="true">
                    <i
                      className="column-income"
                      style={{ height: `${height(row.income, axisMax)}%` }}
                      title={`收入 ¥${money(decimal(row.income))}`}
                    />
                    <i
                      className="column-expense"
                      style={{ height: `${height(row.expense, axisMax)}%` }}
                      title={`支出 ¥${money(decimal(row.expense))}`}
                    />
                  </div>
                  <span>{yearly ? row.label : `${row.label}月`}</span>
                </button>
              ))}
            </div>
          </div>
          {selection ? (
            <div className="chart-selection" role="status" aria-live="polite">
              <strong>{selection.period}</strong>
              <span>收入 ¥{money(decimal(selection.income))}</span>
              <span>支出 ¥{money(decimal(selection.expense))}</span>
            </div>
          ) : (
            <p className="field-hint">点选{yearly ? '年份' : '月份'}查看收支金额。</p>
          )}
        </>
      )}
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
      <TrendChart report={monthly} />
      <TrendChart report={yearly} yearly />
    </>
  );
}
