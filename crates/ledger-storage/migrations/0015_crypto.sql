-- Crypto review records are deliberately independent of fiat transactions/postings.
CREATE TABLE crypto_watchlist (
    instrument TEXT PRIMARY KEY NOT NULL,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0 AND ordinal < 32)
);
CREATE TABLE crypto_records (
    record_key TEXT PRIMARY KEY NOT NULL,
    payload TEXT NOT NULL,
    review_status TEXT NOT NULL CHECK(review_status IN ('pending','confirmed','ignored')),
    occurred_at INTEGER NOT NULL CHECK(occurred_at > 0),
    imported_at_utc_micros INTEGER NOT NULL
);
CREATE INDEX crypto_records_review ON crypto_records(review_status, occurred_at DESC, record_key);
