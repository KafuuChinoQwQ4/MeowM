CREATE TABLE transaction_fx_snapshots (
  transaction_id TEXT PRIMARY KEY REFERENCES transactions(id) ON DELETE CASCADE,
  base_currency_code TEXT NOT NULL CHECK(base_currency_code = 'CNY'),
  quote_currency_code TEXT NOT NULL CHECK(quote_currency_code IN ('USD','EUR','JPY','HKD','GBP','KRW','AUD','CAD','SGD','CHF')),
  rate TEXT NOT NULL CHECK(length(rate) BETWEEN 1 AND 64 AND rate NOT GLOB '*[^0-9.]*' AND rate NOT LIKE '.%' AND rate NOT LIKE '%.' AND rate NOT GLOB '*.*.*' AND CAST(rate AS REAL) > 0),
  effective_date TEXT NOT NULL CHECK(length(effective_date) = 10),
  source_kind TEXT NOT NULL CHECK(source_kind IN ('reference','manual')),
  provider TEXT,
  provider_sources TEXT NOT NULL CHECK(json_valid(provider_sources) AND json_type(provider_sources) = 'array'),
  CHECK((source_kind = 'reference' AND provider IS NOT NULL AND length(trim(provider)) > 0 AND json_array_length(provider_sources) > 0)
     OR (source_kind = 'manual' AND provider IS NULL AND json_array_length(provider_sources) = 0))
);
