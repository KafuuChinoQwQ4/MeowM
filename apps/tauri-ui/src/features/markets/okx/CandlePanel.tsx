import { useCallback, useState } from 'react';
import { cryptoApi, type CryptoCandle } from '../../../api/crypto';
import { useResource } from '../../../shared/useResource';
import { ReadState, safe, time, useAction } from './shared';

export function CandlePanel({ instrument, bar }: { instrument: string; bar: string }) {
  const data = useResource(
    useCallback(() => safe(cryptoApi.candles(instrument, bar)), [instrument, bar]),
  );
  const [older, setOlder] = useState<CryptoCandle[]>([]);
  const [finished, setFinished] = useState(false);
  const [selected, setSelected] = useState<number | null>(null);
  const action = useAction();
  const rows = [
    ...new Map([...older, ...(data.data ?? [])].map((r) => [r.timestamp, r])).values(),
  ].sort((a, b) => a.timestamp - b.timestamp);
  const current = rows.find((r) => r.timestamp === selected) ?? rows.at(-1);
  const refresh = () => {
    setOlder([]);
    setFinished(false);
    setSelected(null);
    data.reload();
  };
  const max = Math.max(...rows.map((r) => Number(r.high)));
  const min = Math.min(...rows.map((r) => Number(r.low)));
  const y = (value: string) => 185 - ((Number(value) - min) / (max - min || 1)) * 160;
  const width = 660 / Math.max(rows.length, 1);
  return (
    <section className="okx-candles" aria-label="价格走势">
      <ReadState {...data} />
      {data.data && rows.length === 0 && <p>该交易对暂无 K 线数据。</p>}
      {rows.length > 0 && (
        <>
          <svg
            viewBox="0 0 700 215"
            role="img"
            aria-label={`${instrument} K 线，${rows.length} 根`}
          >
            <title>{instrument} 开高低收，绿色上涨，红色下跌</title>
            {rows.map((r, index) => {
              const x = 20 + (index + 0.5) * width;
              const up = Number(r.close) >= Number(r.open);
              return (
                <g
                  key={r.timestamp}
                  data-candle={r.timestamp}
                  className={up ? 'okx-candle-up' : 'okx-candle-down'}
                >
                  <title>
                    {time(r.timestamp)} 开 {r.open} 高 {r.high} 低 {r.low} 收 {r.close}
                  </title>
                  <line x1={x} x2={x} y1={y(r.high)} y2={y(r.low)} />
                  <rect
                    x={x - Math.max(width * 0.6, 1) / 2}
                    y={Math.min(y(r.open), y(r.close))}
                    width={Math.max(width * 0.6, 1)}
                    height={Math.max(Math.abs(y(r.open) - y(r.close)), 1)}
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
                rows.findIndex((r) => r.timestamp === current?.timestamp),
              )}
              onChange={(event) => setSelected(rows[Number(event.target.value)].timestamp)}
            />
          </label>
          {current && (
            <div className="okx-candle-detail">
              <strong>
                {time(current.timestamp)} · {current.confirmed ? '已收盘' : '未收盘'}
              </strong>
              <span>
                开 {current.open} · 高 {current.high} · 低 {current.low} · 收 {current.close}
              </span>
              <span>成交量 {current.volume}</span>
            </div>
          )}
          <p className="field-hint">
            {time(rows[0].timestamp)} — {time(rows.at(-1)!.timestamp)} · {rows.length} 根
          </p>
        </>
      )}
      <div className="okx-actions">
        <button className="button" disabled={data.loading || action.busy} onClick={refresh}>
          刷新 K 线
        </button>
        <button
          className="button"
          disabled={!rows.length || data.loading || action.busy || finished || rows.length >= 1000}
          onClick={() =>
            void action.run(async () => {
              const result = await cryptoApi.candles(instrument, bar, String(rows[0].timestamp));
              if (!action.active.current) return;
              const earlier = result.filter((r) => r.timestamp < rows[0].timestamp);
              setOlder((old) => [...earlier, ...old]);
              setFinished(earlier.length === 0);
              setSelected(earlier[0]?.timestamp ?? null);
            })
          }
        >
          {action.busy ? '正在读取…' : finished ? '没有更早数据' : '加载更早 K 线'}
        </button>
      </div>
      {action.error && <p role="alert">{action.error}</p>}
    </section>
  );
}
