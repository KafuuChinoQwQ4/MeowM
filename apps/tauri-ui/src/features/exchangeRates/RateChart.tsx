import { useState } from 'react';
import type { ExchangeRatePairRow } from '../../api/exchangeRates';

export function RateChart({
  rows,
  base,
  quote,
}: {
  rows: ExchangeRatePairRow[];
  base: string;
  quote: string;
}) {
  const [selected, setSelected] = useState<ExchangeRatePairRow | null>(null);
  const points = rows
    .map((row) => ({ row, value: Number(row.rate), time: Date.parse(row.effective_date) }))
    .filter(
      (point) => Number.isFinite(point.value) && point.value > 0 && Number.isFinite(point.time),
    );
  if (!points.length) return <p className="empty-state">所选范围暂无汇率数据</p>;
  const width = 640,
    height = 300,
    left = 78,
    right = 24,
    top = 24,
    bottom = 44;
  const values = points.map((point) => point.value);
  const min = Math.min(...values),
    max = Math.max(...values);
  const pad = Math.max((max - min) * 0.15, max * 0.002);
  const low = min - pad,
    high = max + pad;
  const firstTime = points[0].time,
    lastTime = points[points.length - 1].time;
  const x = (time: number) =>
    lastTime === firstTime
      ? (left + width - right) / 2
      : left + ((time - firstTime) / (lastTime - firstTime)) * (width - left - right);
  const y = (value: number) => top + ((high - value) / (high - low)) * (height - top - bottom);
  const shown =
    selected && points.some((point) => point.row === selected)
      ? selected
      : points[points.length - 1].row;
  return (
    <div className="rate-chart-panel">
      <svg
        className="rate-chart"
        viewBox={`0 0 ${width} ${height}`}
        role="img"
        aria-label={`${base} 至 ${quote} 参考汇率曲线`}
      >
        <title>
          每 1 {base} 对应的 {quote} 数量，按双方共同有效日期连接
        </title>
        {[low, (low + high) / 2, high].map((value) => (
          <g key={value} className="rate-chart-grid">
            <line x1={left} x2={width - right} y1={y(value)} y2={y(value)} />
            <text x={left - 10} y={y(value) + 4} textAnchor="end">
              {value.toPrecision(5)}
            </text>
          </g>
        ))}
        <polyline
          className="rate-chart-line"
          fill="none"
          points={points.map((point) => `${x(point.time)},${y(point.value)}`).join(' ')}
        />
        {points.map(({ row, time, value }) => (
          <circle
            key={row.effective_date}
            cx={x(time)}
            cy={y(value)}
            r="3.5"
            tabIndex={0}
            role="button"
            aria-label={`${row.effective_date}：1 ${base} = ${row.rate} ${quote}`}
            onClick={() => setSelected(row)}
            onFocus={() => setSelected(row)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                setSelected(row);
              }
            }}
          >
            <title>
              {row.effective_date}：1 {base} = {row.rate} {quote}
            </title>
          </circle>
        ))}
        <text x={left} y={height - 10}>
          {points[0].row.effective_date}
        </text>
        {points.length > 1 && (
          <text x={width - right} y={height - 10} textAnchor="end">
            {points[points.length - 1].row.effective_date}
          </text>
        )}
      </svg>
      <p className="rate-chart-selected" role="status">
        {shown.effective_date} · 1 {base} ={' '}
        <strong>
          {shown.rate} {quote}
        </strong>
      </p>
      <p className="muted">点击或聚焦曲线上的数据点查看日期和精确汇率。</p>
    </div>
  );
}
