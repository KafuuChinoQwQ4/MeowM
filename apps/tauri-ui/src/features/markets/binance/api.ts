import { array, decimal, object, requestMarket, text, type MarketCandle } from '../shared/api';

const endpoint = (name: string) => `binance.rest.spot_rest_api.get.api_v3_${name}`;
const requiredDecimal = (value: Parameters<typeof decimal>[0]) => {
  const result = decimal(value);
  if (result === null) throw new Error('币安返回了无法识别的价格或数量。');
  return result;
};

export interface BinanceQuote {
  symbol: string;
  instrument: string;
  base: string;
  quote: string;
  last: string;
  open: string;
  high: string;
  low: string;
  volume: string;
  change: string | null;
  timestamp: number | null;
}

export async function binanceQuotes(): Promise<BinanceQuote[]> {
  const [metadata, tickers] = await Promise.all([
    requestMarket('binance', endpoint('exchangeinfo'), {
      permissions: 'SPOT',
      showPermissionSets: false,
      symbolStatus: 'TRADING',
    }),
    requestMarket('binance', endpoint('ticker_24hr'), { symbolStatus: 'TRADING' }),
  ]);
  if (!Array.isArray(object(metadata).symbols) || !Array.isArray(tickers))
    throw new Error('币安返回的现货目录无法识别。');
  const prices = new Map(array(tickers).map((row) => [text(object(row).symbol), object(row)]));
  return array(object(metadata).symbols)
    .map(object)
    .filter((row) => row.status === 'TRADING' && row.isSpotTradingAllowed === true)
    .flatMap((row) => {
      const symbol = text(row.symbol);
      const base = text(row.baseAsset);
      const quote = text(row.quoteAsset);
      const ticker = prices.get(symbol);
      if (!ticker) return [];
      if (![symbol, base, quote].every((value) => /^[\p{L}\p{N}]{1,64}$/u.test(value)))
        throw new Error('币安返回的交易对代码无法识别。');
      return [
        {
          symbol,
          instrument: `${base}-${quote}`,
          base,
          quote,
          last: requiredDecimal(ticker.lastPrice),
          open: requiredDecimal(ticker.openPrice),
          high: requiredDecimal(ticker.highPrice),
          low: requiredDecimal(ticker.lowPrice),
          volume: requiredDecimal(ticker.volume),
          change: decimal(ticker.priceChangePercent),
          timestamp: typeof ticker.closeTime === 'number' ? ticker.closeTime : null,
        },
      ];
    })
    .sort((a, b) => a.instrument.localeCompare(b.instrument));
}

export async function binanceCandles(
  symbol: string,
  interval: string,
  before?: number,
): Promise<MarketCandle[]> {
  const value = await requestMarket('binance', endpoint('klines'), {
    symbol,
    interval,
    limit: 100,
    ...(before === undefined ? {} : { endTime: before - 1 }),
  });
  if (!Array.isArray(value)) throw new Error('币安返回的 K 线无法识别。');
  return value.map((value) => {
    const row = array(value);
    if (typeof row[0] !== 'number' || !Number.isSafeInteger(row[0]) || typeof row[6] !== 'number')
      throw new Error('币安返回的 K 线时间无法识别。');
    return {
      timestamp: row[0],
      open: requiredDecimal(row[1]),
      high: requiredDecimal(row[2]),
      low: requiredDecimal(row[3]),
      close: requiredDecimal(row[4]),
      volume: requiredDecimal(row[5]),
      confirmed: row[6] < Date.now(),
    };
  });
}

export async function binanceBalances() {
  const value = object(await requestMarket('binance', endpoint('account'), {}, 'read'));
  if (!Array.isArray(value.balances)) throw new Error('币安返回的账户余额无法识别。');
  return array(value.balances)
    .map((value) => {
      const row = object(value);
      const asset = text(row.asset);
      if (!/^[\p{L}\p{N}]{1,32}$/u.test(asset)) throw new Error('币安返回的资产代码无法识别。');
      return { asset, free: requiredDecimal(row.free), locked: requiredDecimal(row.locked) };
    })
    .filter((row) => /[1-9]/.test(row.free) || /[1-9]/.test(row.locked));
}
