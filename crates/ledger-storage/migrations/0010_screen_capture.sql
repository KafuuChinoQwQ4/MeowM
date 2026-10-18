-- Allow provenance for local screenshot/OCR capture while preserving existing rows.
CREATE TABLE source_evidence_v10 (
 id INTEGER PRIMARY KEY,
 transaction_id TEXT NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
 source_kind TEXT NOT NULL CHECK(source_kind IN ('alipay','wechat','bank','notification','screen_capture','manual','recurring')),
 external_transaction_id TEXT,
 import_batch_id TEXT NOT NULL REFERENCES import_batches(id),
 source_fields_digest BLOB NOT NULL CHECK(length(source_fields_digest)=32),
 UNIQUE(source_kind,external_transaction_id)
);
INSERT INTO source_evidence_v10 SELECT * FROM source_evidence;
DROP TABLE source_evidence;
ALTER TABLE source_evidence_v10 RENAME TO source_evidence;
CREATE INDEX source_evidence_transaction ON source_evidence(transaction_id);

CREATE TABLE screen_settings(package_name TEXT PRIMARY KEY, enabled INTEGER NOT NULL CHECK(enabled IN (0,1)), account_id TEXT REFERENCES accounts(id));
CREATE TABLE screen_receipts(identity TEXT PRIMARY KEY CHECK(length(identity)=64), digest BLOB NOT NULL CHECK(length(digest)=32), transaction_id TEXT NOT NULL REFERENCES transactions(id));
CREATE TABLE screen_status(package_name TEXT PRIMARY KEY REFERENCES screen_settings(package_name), code TEXT NOT NULL, received_at_ms INTEGER NOT NULL, transaction_id TEXT REFERENCES transactions(id));
CREATE TABLE capture_matches(source_kind TEXT NOT NULL CHECK(source_kind IN ('notification','screen_capture')), package_name TEXT NOT NULL, transaction_id TEXT NOT NULL REFERENCES transactions(id), captured_at_ms INTEGER NOT NULL, PRIMARY KEY(source_kind,transaction_id));
