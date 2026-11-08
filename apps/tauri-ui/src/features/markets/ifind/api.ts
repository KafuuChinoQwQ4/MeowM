import { array, decimal, object, requestMarket, text, type MarketCandle } from '../shared/api';
import type { MarketJson } from '../../../api/marketApi';

export interface Stock {
  code: string;
  label: string;
}

export const isStockCode = (code: string) => /^\d{6}\.(?:SH|SZ|BJ)$/.test(code);
const endpoint = (name: string) => `ifind.rest.post.${name}`;
const at = (value: MarketJson | undefined, index = 0) =>
  Array.isArray(value) ? value[index] : value;

async function request(name: string, params: { [key: string]: MarketJson }) {
  const data = object(await requestMarket('ifind', endpoint(name), params));
  if (data.errorcode !== undefined && text(data.errorcode) !== '0')
    throw new Error(`${text(data.errmsg) || 'iFinD 数据读取失败'} (${text(data.errorcode)})`);
  if (!Array.isArray(data.tables)) throw new Error('iFinD 返回的数据表无法识别。');
  return data;
}

export async function searchStocks(query: string): Promise<Stock[]> {
  const code = query.trim().toUpperCase();
  if (isStockCode(code)) return [{ code, label: code }];
  const data = await request('smart_stock_picking', {
    searchstring: query.trim(),
    searchtype: 'stock',
  });
  const result = new Map<string, Stock>();
  const read = (value: MarketJson) => {
    const row = object(value);
    const keys = Object.keys(row);
    const normalized = (key: string) => key.replace(/\[.*?\]/g, '').toLowerCase();
    const codeKey = keys.find((key) =>
      /^(?:thscode|code|股票代码|证券代码|同花顺代码)$/.test(normalized(key)),
    );
    const nameKey = keys.find((key) =>
      /^(?:name|secname|证券简称|股票简称|证券名称|股票名称)$/.test(normalized(key)),
    );
    if (!codeKey) return;
    const codes = Array.isArray(row[codeKey]) ? (row[codeKey] as MarketJson[]) : [row[codeKey]];
    codes.forEach((value, index) => {
      const code = text(value).toUpperCase();
      if (isStockCode(code))
        result.set(code, { code, label: text(nameKey ? at(row[nameKey], index) : '') || code });
    });
  };
  for (const value of array(data.tables)) {
    const table = object(value).table;
    if (Array.isArray(table)) table.forEach(read);
    else if (table && typeof table === 'object') read(table);
    read(value);
  }
  if (
    !result.size &&
    array(data.tables).some((value) => Object.keys(object(object(value).table)).length > 0)
  )
    throw new Error('搜索结果未提供可识别的 A 股代码，请输入完整代码，如 300033.SZ。');
  return [...result.values()];
}

function findTable(data: { [key: string]: MarketJson }, code: string) {
  const row = array(data.tables)
    .map(object)
    .find((row) => text(row.thscode).toUpperCase() === code);
  if (!row) throw new Error(`iFinD 未返回 ${code} 的数据，请检查代码及数据权限。`);
  return row;
}

export async function stockQuote(code: string) {
  const data = await request('real_time_quotation', {
    codes: code,
    indicators: 'latest,open,high,low',
  });
  const row = findTable(data, code);
  const fields = object(row.table);
  return {
    last: decimal(at(fields.latest)),
    open: decimal(at(fields.open)),
    high: decimal(at(fields.high)),
    low: decimal(at(fields.low)),
    time: text(at(row.time) ?? at(data.time)),
  };
}

function dateAt(timestamp: number) {
  // iFinD A-share trading dates use China time, independently of the device zone.
  return new Date(timestamp + 8 * 3600000).toISOString().slice(0, 10);
}

export async function stockCandles(
  code: string,
  period: string,
  before?: number,
): Promise<MarketCandle[]> {
  const end = before === undefined ? Date.now() : before - 86400000;
  const days = period === 'M' ? 3650 : period === 'W' ? 1095 : 365;
  const data = await request('cmd_history_quotation', {
    codes: code,
    indicators: 'open,high,low,close,volume',
    startdate: dateAt(end - days * 86400000),
    enddate: dateAt(end),
    functionpara: { Interval: period, CPS: '1', Fill: 'Omit' },
  });
  const row = findTable(data, code);
  const fields = object(row.table);
  const times = row.time ?? data.time;
  if (!Array.isArray(times)) throw new Error('iFinD 未提供 K 线的交易日期。');
  return times.flatMap((value, index) => {
    const date = text(value)
      .replace(/^(\d{4})(\d{2})(\d{2})$/, '$1-$2-$3')
      .slice(0, 10);
    const timestamp = /^\d{4}-\d{2}-\d{2}$/.test(date) ? Date.parse(`${date}T00:00:00+08:00`) : NaN;
    const open = decimal(at(fields.open, index));
    const high = decimal(at(fields.high, index));
    const low = decimal(at(fields.low, index));
    const close = decimal(at(fields.close, index));
    // Non-trading dates may contain nulls; do not invent zero-price candles.
    if (
      !Number.isFinite(timestamp) ||
      open === null ||
      high === null ||
      low === null ||
      close === null
    )
      return [];
    return [
      {
        timestamp,
        open,
        high,
        low,
        close,
        volume: decimal(at(fields.volume, index)),
        confirmed: null,
      },
    ];
  });
}
