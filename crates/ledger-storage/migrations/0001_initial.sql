CREATE TABLE accounts (
    id TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('asset', 'liability')),
    currency_code TEXT NOT NULL CHECK (length(currency_code) = 3),
    currency_exponent INTEGER NOT NULL CHECK (currency_exponent BETWEEN 0 AND 9),
    opening_balance_minor INTEGER NOT NULL,
    opening_balance_effective_at_utc_micros INTEGER NOT NULL
);

CREATE TABLE categories (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL
);

CREATE TABLE transactions (
    id TEXT PRIMARY KEY NOT NULL,
    occurred_at_utc_micros INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('income', 'expense', 'transfer', 'refund', 'balance_adjustment')),
    status TEXT NOT NULL CHECK (status IN ('pending', 'confirmed', 'ignored')),
    category_id TEXT REFERENCES categories(id) ON DELETE SET NULL,
    original_transaction_id TEXT REFERENCES transactions(id),
    report_amount_minor INTEGER,
    report_currency_code TEXT,
    report_currency_exponent INTEGER
);

CREATE TABLE postings (
    transaction_id TEXT NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    delta_minor INTEGER NOT NULL,
    currency_code TEXT NOT NULL CHECK (length(currency_code) = 3),
    currency_exponent INTEGER NOT NULL CHECK (currency_exponent BETWEEN 0 AND 9),
    PRIMARY KEY (transaction_id, ordinal)
);

CREATE TABLE import_batches (
    id TEXT PRIMARY KEY NOT NULL,
    created_at_utc_micros INTEGER NOT NULL DEFAULT (CAST(strftime('%s','now') AS INTEGER) * 1000000)
);

CREATE TABLE source_evidence (
    id INTEGER PRIMARY KEY,
    transaction_id TEXT NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('alipay', 'wechat', 'notification', 'manual')),
    external_transaction_id TEXT,
    import_batch_id TEXT NOT NULL REFERENCES import_batches(id),
    source_fields_digest BLOB NOT NULL CHECK (length(source_fields_digest) = 32),
    UNIQUE (source_kind, external_transaction_id)
);

CREATE TABLE schema_version (
    version INTEGER PRIMARY KEY NOT NULL,
    applied_at_utc_micros INTEGER NOT NULL
);
