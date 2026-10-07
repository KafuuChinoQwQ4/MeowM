ALTER TABLE source_evidence RENAME TO source_evidence_v2;

CREATE TABLE source_evidence (
    id INTEGER PRIMARY KEY,
    transaction_id TEXT NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('alipay', 'wechat', 'notification', 'bank_csv', 'manual')),
    external_transaction_id TEXT,
    import_batch_id TEXT NOT NULL REFERENCES import_batches(id),
    source_fields_digest BLOB NOT NULL CHECK (length(source_fields_digest) = 32),
    UNIQUE (source_kind, external_transaction_id)
);

INSERT INTO source_evidence(transaction_id, source_kind, external_transaction_id, import_batch_id, source_fields_digest)
SELECT transaction_id, source_kind, external_transaction_id, import_batch_id, source_fields_digest FROM source_evidence_v2;

DROP TABLE source_evidence_v2;
