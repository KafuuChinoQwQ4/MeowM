import { invoke } from './client';

export interface ExchangeRateHistoryRow {
  effective_date: string;
  quote_currency: string;
  rate: string;
  provider: string;
  provider_sources: string[];
}
export interface ExchangeRateSyncState {
  status:
    | 'never'
    | 'success'
    | 'no_new_data'
    | 'network_error'
    | 'timeout'
    | 'provider_error'
    | 'invalid_data';
  latest_date: string | null;
  last_attempt_utc: string | null;
  last_success_utc: string | null;
  error_code: string | null;
}
export const exchangeRatesApi = {
  history: (quoteCurrency: string, fromDate: string, untilDate: string) =>
    invoke<ExchangeRateHistoryRow[]>('exchange_rates_history', {
      quoteCurrency,
      fromDate,
      untilDate,
    }),
  state: () => invoke<ExchangeRateSyncState>('exchange_rates_sync_state'),
  refresh: () => invoke<ExchangeRateSyncState>('exchange_rates_refresh'),
};
