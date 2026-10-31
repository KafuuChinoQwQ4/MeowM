import { invoke } from './client';
import { currencyPairRate } from '../shared/currencyConversion';

export interface ExchangeRateHistoryRow {
  effective_date: string;
  quote_currency: string;
  rate: string;
  provider: string;
  provider_sources: string[];
}
export interface ExchangeRatePairRow {
  effective_date: string;
  base_currency: string;
  quote_currency: string;
  rate: string;
  base_reference_rate: string;
  quote_reference_rate: string;
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
function history(quoteCurrency: string, fromDate: string, untilDate: string) {
  return invoke<ExchangeRateHistoryRow[]>('exchange_rates_history', {
    quoteCurrency,
    fromDate,
    untilDate,
  });
}

export const exchangeRatesApi = {
  history,
  pairHistory: async (
    baseCurrency: string,
    quoteCurrency: string,
    fromDate: string,
    untilDate: string,
  ): Promise<ExchangeRatePairRow[]> => {
    if (baseCurrency === quoteCurrency) return [];
    const [baseRows, quoteRows] = await Promise.all([
      baseCurrency === 'CNY' ? Promise.resolve([]) : history(baseCurrency, fromDate, untilDate),
      quoteCurrency === 'CNY' ? Promise.resolve([]) : history(quoteCurrency, fromDate, untilDate),
    ]);
    const baseByDate = new Map(baseRows.map((row) => [row.effective_date, row]));
    const quoteByDate = new Map(quoteRows.map((row) => [row.effective_date, row]));
    const dates =
      baseCurrency === 'CNY'
        ? [...quoteByDate.keys()]
        : quoteCurrency === 'CNY'
          ? [...baseByDate.keys()]
          : [...baseByDate.keys()].filter((date) => quoteByDate.has(date));

    return dates.sort().flatMap((effective_date) => {
      const base = baseByDate.get(effective_date);
      const quote = quoteByDate.get(effective_date);
      const baseRate = baseCurrency === 'CNY' ? '1' : base?.rate;
      const quoteRate = quoteCurrency === 'CNY' ? '1' : quote?.rate;
      if (!baseRate || !quoteRate) return [];
      const rate = currencyPairRate(baseRate, quoteRate);
      if (!rate) return [];
      const sources = [
        ...new Set([...(base?.provider_sources ?? []), ...(quote?.provider_sources ?? [])]),
      ];
      const providers = [...new Set([base?.provider, quote?.provider].filter(Boolean))];
      return [
        {
          effective_date,
          base_currency: baseCurrency,
          quote_currency: quoteCurrency,
          rate,
          base_reference_rate: baseRate,
          quote_reference_rate: quoteRate,
          provider: providers.join(' + '),
          provider_sources: sources,
        },
      ];
    });
  },
  state: () => invoke<ExchangeRateSyncState>('exchange_rates_sync_state'),
  refresh: () => invoke<ExchangeRateSyncState>('exchange_rates_refresh'),
};
