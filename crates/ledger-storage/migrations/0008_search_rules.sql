ALTER TABLE transactions ADD COLUMN merchant_normalized TEXT;
CREATE INDEX transactions_search_order ON transactions(occurred_at_utc_micros DESC,id);
CREATE INDEX transactions_search_amount ON transactions(report_amount_minor);
CREATE INDEX transactions_merchant_normalized ON transactions(merchant_normalized);
CREATE TABLE merchant_aliases (
 alias_normalized TEXT PRIMARY KEY NOT NULL,
 canonical_normalized TEXT NOT NULL CHECK(length(canonical_normalized)>0)
);
