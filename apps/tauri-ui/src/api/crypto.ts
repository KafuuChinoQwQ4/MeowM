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

export type OkxEnvironment = 'demo' | 'production';
export type OkxInstrumentType = 'SPOT' | 'SWAP' | 'FUTURES';
export type OkxOrderSide = 'buy' | 'sell';
export type OkxOrderType = 'market' | 'limit';
export type OkxTradeMode = 'cash' | 'cross' | 'isolated';
export type OkxPositionAction = 'open_long' | 'open_short' | 'close_long' | 'close_short';
export type OkxOrderStatus = 'accepted' | 'rejected' | 'unknown';

export interface OkxOrderDraft {
  environment: OkxEnvironment;
  inst_type: OkxInstrumentType;
  inst_id: string;
  side: OkxOrderSide;
  ord_type: OkxOrderType;
  size: string;
  price: string | null;
  slippage_pct: string | null;
  td_mode: OkxTradeMode | null;
  position_action: OkxPositionAction | null;
  cl_ord_id: string;
}

export interface OkxInstrumentSpec {
  inst_type: OkxInstrumentType;
  inst_id: string;
  state: string;
  tick_sz: string;
  lot_sz: string;
  min_sz: string;
  ct_val: string | null;
  ct_val_ccy: string | null;
  ct_type: string | null;
  settle_ccy: string | null;
  base_ccy: string | null;
  quote_ccy: string | null;
}

export interface OkxTradePosition {
  inst_id: string;
  pos_side: 'net' | 'long' | 'short';
  quantity: string;
  margin_mode: OkxTradeMode;
}

export interface OkxPendingOrder {
  inst_id: string;
  ord_id: string;
  state: 'live' | 'partially_filled';
  side: OkxOrderSide;
  pos_side: 'net' | 'long' | 'short' | null;
  margin_mode: OkxTradeMode;
  reduce_only: boolean;
  remaining_size: string;
}

export interface OkxInstrumentLeverage {
  inst_id: string;
  margin_mode: OkxTradeMode;
  pos_side: 'net' | 'long' | 'short' | null;
  leverage: string;
}

export interface OkxTradeContext {
  instrument: OkxInstrumentSpec;
  account: {
    acct_lv: string;
    pos_mode: 'net_mode' | 'long_short_mode';
    positions: OkxTradePosition[];
    pending_orders: OkxPendingOrder[];
    leverage: OkxInstrumentLeverage[];
  };
}

export interface OkxOrderAcknowledgement {
  order_id: string;
  client_order_id: string;
  code: string;
  message: string;
}

export interface OkxOrderSubmission {
  status: OkxOrderStatus;
  client_order_id: string;
  acknowledgement: OkxOrderAcknowledgement | null;
  error_code: string | null;
}

export interface OkxOrderSnapshot {
  inst_id: string;
  order_id: string;
  client_order_id: string;
  inst_type: string;
  side: string;
  order_type: string;
  trade_mode: string;
  sz: string;
  price: string;
  average_price: string;
  accumulated_fill_size: string;
  state: string;
  position_side: string;
  reduce_only: string;
  target_currency: string;
  created_at: string;
  updated_at: string;
}

export interface OkxOrderActionResult {
  status: OkxOrderStatus;
  order_id: string;
  client_order_id: string | null;
  code: string | null;
  message: string | null;
  error_code: string | null;
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
  tradeContext: (instType: OkxInstrumentType, instId: string, environment: OkxEnvironment) =>
    invoke<OkxTradeContext>('okx_trade_context', { instType, instId, environment }),
  placeOrder: (draft: OkxOrderDraft) => invoke<OkxOrderSubmission>('okx_place_order', { draft }),
  orderDetail: (instId: string, clOrdId: string, environment: OkxEnvironment) =>
    invoke<OkxOrderSnapshot | null>('okx_order_detail', { instId, clOrdId, environment }),
  orderHistory: (instType: OkxInstrumentType, environment: OkxEnvironment) =>
    invoke<OkxOrderSnapshot[]>('okx_order_history', { instType, environment }),
  cancelOrder: (instId: string, ordId: string, environment: OkxEnvironment) =>
    invoke<OkxOrderActionResult>('okx_cancel_order', { instId, ordId, environment }),
};
