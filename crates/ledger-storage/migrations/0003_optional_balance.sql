CREATE TABLE accounts_new (
    id TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('asset', 'liability')),
    currency_code TEXT NOT NULL CHECK (length(currency_code) = 3),
    currency_exponent INTEGER NOT NULL CHECK (currency_exponent BETWEEN 0 AND 9),
    opening_balance_minor INTEGER,
    opening_balance_effective_at_utc_micros INTEGER,
    CHECK ((opening_balance_minor IS NULL) = (opening_balance_effective_at_utc_micros IS NULL))
);
INSERT INTO accounts_new SELECT * FROM accounts;
DROP TABLE accounts;
ALTER TABLE accounts_new RENAME TO accounts;
