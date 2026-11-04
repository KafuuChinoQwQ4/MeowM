import { invoke } from './client';

export interface CryptoTicker {
  instrument: string;
  last: string;
  open_24h: string;
  high_24h: string;
  low_24h: string;
  volume_24h: string;
  timestamp: number;
}
export interface CryptoCandle {
  timestamp: number;
  open: string;
  high: string;
  low: string;
  close: string;
  volume: string;
  confirmed: boolean;
}
export interface CryptoBalance {
  account: 'trading' | 'funding';
  asset: string;
  quantity: string | null;
  available: string | null;
  frozen: string | null;
  equity: string | null;
  equity_usd: string | null;
}
export interface CryptoAssets {
  balances: CryptoBalance[];
  valuation: {
    total_usd: string;
    funding_usd: string;
    trading_usd: string;
    earn_usd: string | null;
    timestamp: number;
  };
  fetched_at: number;
}
export type CryptoHistorySource = 'bills' | 'bills_archive' | 'fills' | 'deposits' | 'withdrawals';
export type CryptoReviewStatus = 'pending' | 'confirmed' | 'ignored';
export interface CryptoHistoryRequest {
  source: CryptoHistorySource;
  begin: number;
  end: number;
  after: string | null;
  account_uid: string | null;
}
export interface CryptoLeg {
  asset: string;
  quantity: string;
}
export interface CryptoRecord {
  key: string;
  account_uid: string;
  source: 'bill' | 'fill' | 'deposit' | 'withdrawal';
  source_id: string;
  occurred_at: number;
  instrument: string | null;
  quote_asset?: string | null;
  legs: CryptoLeg[];
  fee: CryptoLeg | null;
  price: string | null;
  platform_state: string;
}
export interface ReviewedCryptoRecord {
  record: CryptoRecord;
  status: CryptoReviewStatus;
}
export interface CryptoRecordPage {
  rows: ReviewedCryptoRecord[];
  total: number;
}
export interface CryptoImportResult {
  inserted: number;
  duplicates: number;
  fetched: number;
  next_after: string | null;
  account_uid: string;
}
export interface CryptoPosition {
  position_id: string;
  instrument: string;
  side: string;
  margin_mode: string;
  currency: string;
  quantity: string | null;
  open_price: string | null;
  close_price: string | null;
  unrealized_pnl: string | null;
  realized_pnl: string | null;
  pnl: string | null;
  fee: string | null;
  funding_fee: string | null;
  liquidation_penalty: string | null;
  updated_at: number;
}
export interface CryptoPositions {
  current: CryptoPosition[];
  history: CryptoPosition[];
  next_after: string | null;
  fetched_at: number;
}

export const cryptoApi = {
  tickers: () => invoke<CryptoTicker[]>('okx_tickers'),
  candles: (instrument: string, bar: string, after: string | null = null) =>
    invoke<CryptoCandle[]>('okx_candles', { instrument, bar, after }),
  watchlist: () => invoke<string[]>('okx_watchlist'),
  saveWatchlist: (instruments: string[]) => invoke<void>('okx_save_watchlist', { instruments }),
  assets: () => invoke<CryptoAssets>('okx_assets'),
  positions: (instrumentType: string, after: string | null = null) =>
    invoke<CryptoPositions>('okx_positions', { instrumentType, after }),
  importHistory: (request: CryptoHistoryRequest) =>
    invoke<CryptoImportResult>('okx_import_history', { request }),
  records: (status: CryptoReviewStatus | null, offset: number) =>
    invoke<CryptoRecordPage>('okx_records', { status, offset }),
  review: (keys: string[], status: CryptoReviewStatus) =>
    invoke<void>('okx_review_records', { keys, status }),
};
