CREATE TABLE exchange_rates (
  effective_date TEXT NOT NULL CHECK(length(effective_date) = 10),
  base_currency_code TEXT NOT NULL CHECK(base_currency_code = 'CNY'),
  quote_currency_code TEXT NOT NULL CHECK(quote_currency_code IN ('USD','EUR','JPY','HKD','GBP','KRW','AUD','CAD','SGD','CHF')),
  rate TEXT NOT NULL,
  provider TEXT NOT NULL CHECK(provider = 'frankfurter'),
  provider_sources TEXT NOT NULL,
  fetched_at_utc_micros INTEGER NOT NULL CHECK(fetched_at_utc_micros >= 0),
  PRIMARY KEY(effective_date, base_currency_code, quote_currency_code)
);

CREATE TABLE exchange_rate_sync (
  singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
  last_attempt_utc_micros INTEGER,
  last_success_utc_micros INTEGER,
  status TEXT NOT NULL CHECK(status IN ('success','no_new_data','network_error','timeout','provider_error','invalid_data')),
  error_code TEXT CHECK(error_code IS NULL OR (length(error_code) BETWEEN 1 AND 64 AND error_code NOT GLOB '*[^a-z0-9_-]*'))
);
