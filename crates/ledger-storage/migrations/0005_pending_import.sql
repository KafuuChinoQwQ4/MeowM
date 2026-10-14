-- Preserve all provenance IDs while adding bank imports to the source CHECK.
CREATE TABLE source_evidence_v5 (
 id INTEGER PRIMARY KEY,
 transaction_id TEXT NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
 source_kind TEXT NOT NULL CHECK(source_kind IN ('alipay','wechat','bank','notification','manual')),
 external_transaction_id TEXT,
 import_batch_id TEXT NOT NULL REFERENCES import_batches(id),
 source_fields_digest BLOB NOT NULL CHECK(length(source_fields_digest)=32),
 UNIQUE(source_kind,external_transaction_id)
);
INSERT INTO source_evidence_v5 SELECT * FROM source_evidence;
DROP TABLE source_evidence;
ALTER TABLE source_evidence_v5 RENAME TO source_evidence;
CREATE INDEX source_evidence_transaction ON source_evidence(transaction_id);
CREATE TABLE import_receipt_rows (
 batch_id TEXT NOT NULL REFERENCES import_batches(id),
 ordinal INTEGER NOT NULL,
 transaction_id TEXT NOT NULL REFERENCES transactions(id),
 source_fields_digest BLOB NOT NULL CHECK(length(source_fields_digest)=32),
 PRIMARY KEY(batch_id,ordinal)
);
