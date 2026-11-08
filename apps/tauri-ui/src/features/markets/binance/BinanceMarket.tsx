import { useCallback, useEffect, useRef, useState } from 'react';
import { useResource } from '../../../shared/useResource';
import { binanceCandles, binanceQuotes } from './api';
import { MarketChart } from '../shared/MarketChart';
import { ReadState } from '../shared/ReadState';
import { useWatchlist } from '../shared/useWatchlist';

export function BinanceMarket({ favoritesOnly }: { favoritesOnly: boolean }) {
  const quotes = useResource(useCallback(binanceQuotes, []));
  const watchlist = useWatchlist('binance');
  const [symbol, setSymbol] = useState('BTCUSDT');
  const [search, setSearch] = useState('');
  const [interval, setInterval] = useState('1h');
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (quotes.data?.length && !quotes.data.some((row) => row.symbol === symbol))
      setSymbol(quotes.data[0].symbol);
  }, [quotes.data, symbol]);
  useEffect(() => {
    if (favoritesOnly && !watchlist.items.some((item) => item.code === symbol)) {
      const first = watchlist.items.find((item) =>
        quotes.data?.some((row) => row.symbol === item.code),
      );
      if (first) setSymbol(first.code);
    }
  }, [favoritesOnly, watchlist.items, quotes.data, symbol]);
  useEffect(() => {
    setSearch('');
    if (list.current) list.current.scrollTop = 0;
  }, [favoritesOnly]);
  const favorite = watchlist.items.some((row) => row.code === symbol);
  const selected =
    !favoritesOnly || favorite ? quotes.data?.find((row) => row.symbol === symbol) : undefined;
  const query = search.trim().toUpperCase();
  const visible = (quotes.data ?? []).filter(
    (row) =>
      (!favoritesOnly || watchlist.items.some((item) => item.code === row.symbol)) &&
      (row.instrument.includes(query) || row.symbol.includes(query)),
  );
  const loadCandles = useCallback(
    (before?: number) => binanceCandles(symbol, interval, before),
    [symbol, interval],
  );
  const changeSearch = (value: string) => {
    setSearch(value);
    if (list.current) list.current.scrollTop = 0;
  };
  return (
    <div className="okx-panel">
      <div className="okx-controls">
        <label className="form-field">
          搜索现货交易对
          <input
            type="search"
            aria-label="搜索现货交易对"
            aria-controls="binance-market-list"
            value={search}
            placeholder="币种或交易对，如 BTC、USDC、ETH-BTC"
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => changeSearch(event.target.value)}
          />
        </label>
        {search && (
          <button className="button" type="button" onClick={() => changeSearch('')}>
            清空搜索
          </button>
        )}
        <button className="button" type="button" disabled={quotes.loading} onClick={quotes.reload}>
          刷新行情
        </button>
      </div>
      <ReadState {...quotes} />
      {watchlist.error && <p role="alert">{watchlist.error}</p>}
      {quotes.data && (
        <p className="field-hint" role="status">
          显示 {visible.length} / {quotes.data.length} 个现货交易对
        </p>
      )}
      {favoritesOnly && !watchlist.items.length && <p>尚未添加自选交易对。</p>}
      <div
        className="okx-quote-list"
        role="region"
        aria-label="币安现货行情列表"
        id="binance-market-list"
        ref={list}
        tabIndex={0}
      >
        {visible.map((row) => (
          <button
            type="button"
            key={row.symbol}
            className={symbol === row.symbol ? 'selected' : ''}
            aria-pressed={symbol === row.symbol}
            onClick={() => setSymbol(row.symbol)}
          >
            <strong>
              {row.instrument}
              {watchlist.items.some((item) => item.code === row.symbol) ? ' ★' : ''}
            </strong>
            <span>
              {row.last} {row.quote}
            </span>
            <small>24h {row.change === null ? '—' : `${row.change}%`}</small>
          </button>
        ))}
      </div>
      {quotes.data && !visible.length && (!favoritesOnly || !!watchlist.items.length) && (
        <p>没有匹配的现货交易对。</p>
      )}
      {favoritesOnly &&
        watchlist.items
          .filter((item) => quotes.data && !quotes.data.some((row) => row.symbol === item.code))
          .map((item) => (
            <p key={item.code}>
              {item.label} 暂无现货行情。{' '}
              <button className="text-button" type="button" onClick={() => watchlist.toggle(item)}>
                移除 {item.label}
              </button>
            </p>
          ))}
      {selected && (
        <>
          <div className="okx-controls">
            <h4>{selected.instrument}</h4>
            <button
              className="button"
              type="button"
              onClick={() => watchlist.toggle({ code: symbol, label: selected.instrument })}
            >
              {favorite ? '取消收藏' : '收藏'} {selected.instrument}
            </button>
          </div>
          <dl className="okx-values">
            <div>
              <dt>最新价 · {selected.quote}</dt>
              <dd>{selected.last}</dd>
            </div>
            <div>
              <dt>24h 开盘</dt>
              <dd>{selected.open}</dd>
            </div>
            <div>
              <dt>24h 最高</dt>
              <dd>{selected.high}</dd>
            </div>
            <div>
              <dt>24h 最低</dt>
              <dd>{selected.low}</dd>
            </div>
            <div>
              <dt>24h 成交量 · {selected.base}</dt>
              <dd>{selected.volume}</dd>
            </div>
          </dl>
          {selected.timestamp !== null && (
            <p className="field-hint">
              行情时间 {new Date(selected.timestamp).toLocaleString('zh-CN')}
            </p>
          )}
          <label className="form-field">
            K 线周期
            <select
              aria-label="K 线周期"
              value={interval}
              onChange={(event) => setInterval(event.target.value)}
            >
              <option value="15m">15 分钟</option>
              <option value="1h">1 小时</option>
              <option value="4h">4 小时</option>
              <option value="1d">日线</option>
              <option value="1w">周线</option>
            </select>
          </label>
          <MarketChart
            key={`${symbol}-${interval}`}
            instrument={selected.instrument}
            load={loadCandles}
          />
        </>
      )}
    </div>
  );
}
