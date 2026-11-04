import { useCallback, useState } from 'react';
import { cryptoApi } from '../../../api/crypto';
import { useResource } from '../../../shared/useResource';
import { CandlePanel } from './CandlePanel';
import { ReadState, safe, useAction, time } from './shared';

export function MarketPanel() {
  const tickers = useResource(
    useCallback(() => safe(cryptoApi.tickers()), []),
    true,
  );
  const watchlist = useResource(useCallback(() => safe(cryptoApi.watchlist()), []));
  const [instrument, setInstrument] = useState('BTC-USDT');
  const [input, setInput] = useState('BTC-USDT');
  const [bar, setBar] = useState('1H');
  const action = useAction();
  const favorites = watchlist.data ?? [];
  const ticker = tickers.data?.find((row) => row.instrument === instrument);
  const displayed =
    tickers.data?.filter((row) =>
      favorites.length
        ? favorites.includes(row.instrument)
        : ['BTC-USDT', 'ETH-USDT', 'SOL-USDT'].includes(row.instrument),
    ) ?? [];
  const choose = (value: string) => {
    setInstrument(value);
    setInput(value);
  };
  return (
    <div className="okx-panel">
      <p className="field-hint">公共行情无需 API Key。价格以交易对的报价币计价，按刷新时点显示。</p>
      <form
        className="okx-controls"
        onSubmit={(event) => {
          event.preventDefault();
          if (/^[A-Z0-9]{1,32}-[A-Z0-9]{1,32}$/.test(input)) choose(input);
        }}
      >
        <label className="form-field">
          现货交易对
          <input
            aria-label="现货交易对"
            value={input}
            list="okx-products"
            pattern="[A-Z0-9]{1,32}-[A-Z0-9]{1,32}"
            maxLength={65}
            required
            onChange={(event) => setInput(event.target.value.toUpperCase())}
          />
        </label>
        <datalist id="okx-products">
          {tickers.data?.map((r) => (
            <option key={r.instrument} value={r.instrument} />
          ))}
        </datalist>
        <button className="button" type="submit">
          查看交易对
        </button>
        <button
          className="button"
          type="button"
          disabled={
            action.busy ||
            watchlist.loading ||
            !!watchlist.error ||
            (favorites.length >= 32 && !favorites.includes(instrument))
          }
          onClick={() =>
            void action.run(async () => {
              const next = favorites.includes(instrument)
                ? favorites.filter((v) => v !== instrument)
                : [...favorites, instrument];
              await cryptoApi.saveWatchlist(next);
              if (action.active.current) watchlist.reload();
            })
          }
        >
          {favorites.includes(instrument) ? '取消收藏' : '收藏'} {instrument}
        </button>
        <button
          className="button"
          type="button"
          disabled={tickers.loading}
          onClick={tickers.reload}
        >
          刷新行情
        </button>
      </form>
      <ReadState {...tickers} />
      <ReadState {...watchlist} />
      {action.error && <p role="alert">{action.error}</p>}
      <div className="okx-quote-list" aria-label="自选行情">
        {displayed.map((row) => (
          <button
            className={row.instrument === instrument ? 'selected' : ''}
            key={row.instrument}
            onClick={() => choose(row.instrument)}
          >
            <strong>{row.instrument}</strong>
            <span>{row.last}</span>
            <small>24h 开盘 {row.open_24h}</small>
          </button>
        ))}
      </div>
      {favorites.length > 0 && (
        <p className="field-hint">
          自选 {favorites.length}/32
          {favorites
            .filter((f) => !tickers.data?.some((r) => r.instrument === f))
            .map((f) => (
              <button className="button" key={f} onClick={() => choose(f)}>
                {f}（暂无行情）
              </button>
            ))}
        </p>
      )}
      {ticker && (
        <div className="okx-quote-summary">
          <strong>
            {ticker.instrument} · {ticker.last}
          </strong>
          <span>
            24h 高 {ticker.high_24h} / 低 {ticker.low_24h}
          </span>
          <small>报价时间 {time(ticker.timestamp)}</small>
        </div>
      )}
      <label className="form-field">
        K 线周期
        <select aria-label="K 线周期" value={bar} onChange={(event) => setBar(event.target.value)}>
          <option value="15m">15 分钟</option>
          <option value="1H">1 小时</option>
          <option value="4H">4 小时</option>
          <option value="1D">1 天（UTC+8）</option>
        </select>
      </label>
      <CandlePanel key={`${instrument}-${bar}`} instrument={instrument} bar={bar} />
    </div>
  );
}
