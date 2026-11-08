import { useCallback, useEffect, useRef, useState } from 'react';
import { useResource } from '../../../shared/useResource';
import { marketError, type MarketCandle } from './api';
import { ReadState } from './ReadState';

export function MarketChart({
  instrument,
  load,
  timeZone,
}: {
  instrument: string;
  load: (before?: number) => Promise<MarketCandle[]>;
  timeZone?: string;
}) {
  const time = (value: number) => new Date(value).toLocaleString('zh-CN', { timeZone });
  const data = useResource(useCallback(() => load(), [load]));
  const [older, setOlder] = useState<MarketCandle[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [finished, setFinished] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const active = useRef(true);
  const pending = useRef(false);
  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);
  const rows = [
    ...new Map([...older, ...(data.data ?? [])].map((row) => [row.timestamp, row])).values(),
  ]
    .sort((a, b) => a.timestamp - b.timestamp)
    .slice(-1000);
  const current = rows.find((row) => row.timestamp === selected) ?? rows.at(-1);
  const max = Math.max(...rows.map((row) => Number(row.high)));
  const min = Math.min(...rows.map((row) => Number(row.low)));
  const y = (value: string) => 185 - ((Number(value) - min) / (max - min || 1)) * 160;
  const width = 660 / Math.max(rows.length, 1);
  const refresh = () => {
    setOlder([]);
    setSelected(null);
    setFinished(false);
    setError('');
    data.reload();
  };
  const earlier = async () => {
    if (pending.current || !rows.length) return;
    pending.current = true;
    setBusy(true);
    setError('');
    try {
      const result = (await load(rows[0].timestamp)).filter(
        (row) => row.timestamp < rows[0].timestamp,
      );
      if (!active.current) return;
      setOlder((old) => [...result, ...old]);
      setSelected(result[0]?.timestamp ?? current?.timestamp ?? null);
      setFinished(result.length === 0);
    } catch (cause) {
      if (active.current) setError(marketError(cause));
    } finally {
      pending.current = false;
      if (active.current) setBusy(false);
    }
  };
  return (
    <section className="okx-candles" aria-label="价格走势">
      <ReadState {...data} />
      {data.data && rows.length === 0 && <p>暂无 K 线数据。</p>}
      {!!rows.length && (
        <>
          <svg
            viewBox="0 0 700 215"
            role="img"
            aria-label={`${instrument} K 线，${rows.length} 根`}
          >
            <title>{instrument} 开高低收，绿色上涨，红色下跌</title>
            {rows.map((row, index) => {
              const x = 20 + (index + 0.5) * width;
              const bodyWidth = Math.max(width * 0.6, 1);
              return (
                <g
                  key={row.timestamp}
                  data-candle={row.timestamp}
                  className={
                    Number(row.close) >= Number(row.open) ? 'okx-candle-up' : 'okx-candle-down'
                  }
                >
                  <title>
                    {time(row.timestamp)} 开 {row.open} 高 {row.high} 低 {row.low} 收 {row.close}
                  </title>
                  <line x1={x} x2={x} y1={y(row.high)} y2={y(row.low)} />
                  <rect
                    x={x - bodyWidth / 2}
                    y={Math.min(y(row.open), y(row.close))}
                    width={bodyWidth}
                    height={Math.max(Math.abs(y(row.open) - y(row.close)), 1)}
                  />
                </g>
              );
            })}
          </svg>
          <label className="form-field">
            查看 K 线
            <input
              type="range"
              aria-label="查看 K 线"
              min={0}
              max={rows.length - 1}
              value={Math.max(
                0,
                rows.findIndex((row) => row.timestamp === current?.timestamp),
              )}
              onChange={(event) => setSelected(rows[Number(event.target.value)].timestamp)}
            />
          </label>
          {current && (
            <div className="okx-candle-detail">
              <strong>
                {time(current.timestamp)}
                {current.confirmed === null ? '' : current.confirmed ? ' · 已收盘' : ' · 未收盘'}
              </strong>
              <span>
                开 {current.open} · 高 {current.high} · 低 {current.low} · 收 {current.close}
              </span>
              <span>成交量 {current.volume ?? '—'}</span>
            </div>
          )}
          <p className="field-hint">
            {time(rows[0].timestamp)} — {time(rows.at(-1)!.timestamp)} · {rows.length} 根
          </p>
        </>
      )}
      <div className="okx-actions">
        <button className="button" type="button" disabled={data.loading || busy} onClick={refresh}>
          刷新 K 线
        </button>
        <button
          className="button"
          type="button"
          disabled={!rows.length || data.loading || busy || finished || rows.length >= 1000}
          onClick={() => void earlier()}
        >
          {busy ? '正在读取…' : finished ? '没有更早数据' : '加载更早 K 线'}
        </button>
      </div>
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
