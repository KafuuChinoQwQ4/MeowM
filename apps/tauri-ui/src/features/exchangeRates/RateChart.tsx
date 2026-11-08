import { useState } from 'react';
import type { KeyboardEvent } from 'react';
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
    )
    .sort((a, b) => a.time - b.time);
  if (!points.length) return <p className="empty-state">所选范围暂无汇率数据</p>;

  const candleCount = Math.min(points.length, 32);
  const candles = Array.from({ length: candleCount }, (_, index) => {
    const from = Math.floor((index * points.length) / candleCount);
    const until = Math.floor(((index + 1) * points.length) / candleCount);
    const group = points.slice(from, until);
    const first = group[0];
    const last = group[group.length - 1];
    const open = points[from - 1]?.value ?? first.value;
    const close = last.value;
    const observations = [open, ...group.map((point) => point.value)];
    return {
      row: last.row,
      startDate: first.row.effective_date,
      endDate: last.row.effective_date,
      open,
      close,
      high: Math.max(...observations),
      low: Math.min(...observations),
      trend: close > open ? 'up' : close < open ? 'down' : 'flat',
    };
  });
  const width = 700;
  const height = 260;
  const left = 10;
  const right = 10;
  const top = 24;
  const bottom = 24;
  const min = Math.min(...candles.map((candle) => candle.low));
  const max = Math.max(...candles.map((candle) => candle.high));
  const pad = Math.max((max - min) * 0.08, max * 0.001);
  const low = min - pad;
  const high = max + pad;
  const y = (value: number) => top + ((high - value) / (high - low)) * (height - top - bottom);
  const pitch = (width - left - right) / Math.max(candles.length, 1);
  const bodyWidth = Math.min(pitch * 0.56, 28);
  const shown = points.some((point) => point.row === selected) ? selected! : points.at(-1)!.row;
  const selectWithKeyboard = (event: KeyboardEvent<SVGGElement>, row: ExchangeRatePairRow) => {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      setSelected(row);
    }
  };

  return (
    <div className="rate-chart-panel">
      <h3 className="rate-chart-title">K 线图</h3>
      <svg
        className="rate-chart"
        viewBox={`0 0 ${width} ${height}`}
        role="img"
        aria-label={`${base} 至 ${quote} 参考汇率 K 线图，${candleCount} 根`}
      >
        <title>
          每根蜡烛展示前一有效日参考汇率至当日参考汇率的变化；法币历史数据没有日内高低价
        </title>
        {Array.from({ length: 6 }, (_, index) => {
          const gridY = top + ((height - top - bottom) / 5) * index;
          return (
            <line
              key={gridY}
              className="rate-chart-grid-line"
              x1={left}
              x2={width - right}
              y1={gridY}
              y2={gridY}
            />
          );
        })}
        {candles.map((candle, index) => {
          const x = left + (index + 0.5) * pitch;
          return (
            <g
              key={candle.row.effective_date}
              data-rate-candle
              data-rate-date={candle.startDate}
              data-rate-end-date={candle.endDate}
              className={`rate-chart-candle rate-chart-candle-${candle.trend}`}
              role="button"
              tabIndex={0}
              aria-label={`${candle.row.effective_date}：1 ${base} = ${candle.row.rate} ${quote}`}
              onClick={() => setSelected(candle.row)}
              onFocus={() => setSelected(candle.row)}
              onKeyDown={(event) => selectWithKeyboard(event, candle.row)}
            >
              <title>
                {candle.startDate} 至 {candle.endDate}：开 {candle.open}，高 {candle.high}，低{' '}
                {candle.low}，收 {candle.close}
              </title>
              <line x1={x} x2={x} y1={y(candle.high)} y2={y(candle.low)} />
              <rect
                x={x - bodyWidth / 2}
                y={Math.min(y(candle.open), y(candle.close))}
                width={bodyWidth}
                height={Math.max(Math.abs(y(candle.open) - y(candle.close)), 1.5)}
              />
            </g>
          );
        })}
        <text x={left} y={height - 5}>
          {candles[0].startDate}
        </text>
        {candles.length > 1 && (
          <text x={width - right} y={height - 5} textAnchor="end">
            {candles[candles.length - 1].endDate}
          </text>
        )}
      </svg>
      <p className="rate-chart-selected" role="status">
        <span>{shown.effective_date}</span>
        <strong>
          1 {base} = {shown.rate} {quote}
        </strong>
      </p>
    </div>
  );
}
