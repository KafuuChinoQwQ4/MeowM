import { useCallback, useEffect, useRef, useState } from 'react';
import { useResource } from '../../../shared/useResource';
import { isStockCode, searchStocks, stockCandles, stockQuote, type Stock } from './api';
import { MarketChart } from '../shared/MarketChart';
import { ReadState } from '../shared/ReadState';
import { useWatchlist } from '../shared/useWatchlist';

function StockDetail({ stock }: { stock: Stock }) {
  const quote = useResource(useCallback(() => stockQuote(stock.code), [stock.code]));
  const [period, setPeriod] = useState('D');
  const loadCandles = useCallback(
    (before?: number) => stockCandles(stock.code, period, before),
    [stock.code, period],
  );
  return (
    <>
      <button className="button" type="button" disabled={quote.loading} onClick={quote.reload}>
        刷新行情
      </button>
      <ReadState {...quote} />
      {quote.data && (
        <>
          <dl className="okx-values">
            <div>
              <dt>最新价</dt>
              <dd>{quote.data.last ?? '—'}</dd>
            </div>
            <div>
              <dt>开盘</dt>
              <dd>{quote.data.open ?? '—'}</dd>
            </div>
            <div>
              <dt>最高</dt>
              <dd>{quote.data.high ?? '—'}</dd>
            </div>
            <div>
              <dt>最低</dt>
              <dd>{quote.data.low ?? '—'}</dd>
            </div>
          </dl>
          <p className="field-hint">
            行情时间 {quote.data.time || '接口未提供'} · A 股价格单位为人民币元
          </p>
        </>
      )}
      <label className="form-field">
        K 线周期
        <select
          aria-label="K 线周期"
          value={period}
          onChange={(event) => setPeriod(event.target.value)}
        >
          <option value="D">日线</option>
          <option value="W">周线</option>
          <option value="M">月线</option>
        </select>
      </label>
      <p className="field-hint">不复权 · 北京时间</p>
      <MarketChart
        key={`${stock.code}-${period}`}
        instrument={stock.code}
        load={loadCandles}
        timeZone="Asia/Shanghai"
      />
    </>
  );
}

export function IfindMarket({
  configured,
  favoritesOnly,
  onConfigure,
}: {
  configured: boolean;
  favoritesOnly: boolean;
  onConfigure: () => void;
}) {
  const watchlist = useWatchlist('ifind');
  const [input, setInput] = useState('');
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<Stock | null>(null);
  const list = useRef<HTMLDivElement>(null);
  const results = useResource(
    useCallback(
      () => (configured && query ? searchStocks(query) : Promise.resolve([])),
      [configured, query],
    ),
  );
  useEffect(() => {
    if (!favoritesOnly && results.data?.length && isStockCode(query.toUpperCase()))
      setSelected(results.data[0]);
  }, [results.data, query, favoritesOnly]);
  useEffect(() => {
    if (favoritesOnly && !watchlist.items.some((item) => item.code === selected?.code))
      setSelected(watchlist.items[0] ?? null);
  }, [favoritesOnly, watchlist.items, selected?.code]);
  useEffect(() => {
    if (list.current) list.current.scrollTop = 0;
  }, [results.data, favoritesOnly]);
  const stocks = favoritesOnly ? watchlist.items : (results.data ?? []);
  const favorite = watchlist.items.some((row) => row.code === selected?.code);
  const search = (value: string) => {
    const next = value.trim();
    if (!next || !configured) return;
    setSelected(null);
    if (next === query) results.reload();
    else setQuery(next);
  };
  return (
    <div className="okx-panel">
      {!configured && (
        <p className="field-hint">
          请配置 iFinD Refresh Token 以搜索股票和读取行情。{' '}
          <button className="text-button" type="button" onClick={onConfigure}>
            配置行情凭据
          </button>
        </p>
      )}
      {!favoritesOnly && (
        <>
          <form
            className="okx-controls"
            onSubmit={(event) => {
              event.preventDefault();
              search(input);
            }}
          >
            <label className="form-field">
              搜索股票
              <input
                type="search"
                aria-label="搜索股票"
                aria-controls="ifind-market-list"
                value={input}
                maxLength={100}
                placeholder="名称或完整代码，如 同花顺、300033.SZ"
                disabled={!configured}
                onChange={(event) => setInput(event.target.value)}
              />
            </label>
            <button className="button" type="submit" disabled={!configured || !input.trim()}>
              搜索股票
            </button>
            <button
              className="button"
              type="button"
              disabled={!configured || results.loading}
              onClick={() => {
                setInput('全部A股');
                search('全部A股');
              }}
            >
              浏览 A 股
            </button>
          </form>
          <ReadState {...results} />
        </>
      )}
      {watchlist.error && <p role="alert">{watchlist.error}</p>}
      {favoritesOnly && !stocks.length && <p>尚未添加自选股票。</p>}
      {!favoritesOnly && query && results.data && !results.data.length && (
        <p>没有找到匹配的股票，请尝试完整代码，如 300033.SZ。</p>
      )}
      {!!stocks.length && (
        <p className="field-hint" role="status">
          {favoritesOnly ? '自选' : '返回'} {stocks.length} 只股票
        </p>
      )}
      <div
        className="okx-quote-list"
        role="region"
        aria-label="股票行情列表"
        id="ifind-market-list"
        ref={list}
        tabIndex={0}
      >
        {stocks.map((stock) => (
          <button
            type="button"
            key={stock.code}
            className={stock.code === selected?.code ? 'selected' : ''}
            aria-pressed={stock.code === selected?.code}
            onClick={() => setSelected(stock)}
          >
            <strong>{stock.label}</strong>
            <span>{stock.code}</span>
            {watchlist.items.some((row) => row.code === stock.code) && <small>★ 自选</small>}
          </button>
        ))}
      </div>
      {selected && (!favoritesOnly || favorite) && (
        <>
          <div className="okx-controls">
            <h4>
              {selected.label === selected.code
                ? selected.code
                : `${selected.label} · ${selected.code}`}
            </h4>
            <button className="button" type="button" onClick={() => watchlist.toggle(selected)}>
              {favorite ? '取消收藏' : '收藏'} {selected.code}
            </button>
          </div>
          {configured && <StockDetail key={selected.code} stock={selected} />}
        </>
      )}
    </div>
  );
}
