-- Preserve template IDs, existing candidate IDs, provenance IDs and future cursors.
ALTER TABLE recurring_templates ADD COLUMN edit_version INTEGER NOT NULL DEFAULT 0 CHECK(edit_version>=0);
UPDATE recurring_templates SET anchor_date=next_date WHERE anchor_date IS NULL;
CREATE TABLE recurring_instances_v7 (
 template_id TEXT NOT NULL REFERENCES recurring_templates(id),
 scheduled_date TEXT NOT NULL,
 transaction_id TEXT UNIQUE REFERENCES transactions(id),
 skipped INTEGER NOT NULL DEFAULT 0 CHECK(skipped IN (0,1)),
 PRIMARY KEY(template_id,scheduled_date),
 CHECK((skipped=1 AND transaction_id IS NULL) OR (skipped=0 AND transaction_id IS NOT NULL))
);
INSERT INTO recurring_instances_v7 SELECT template_id,scheduled_date,transaction_id,0 FROM recurring_instances;
DROP TABLE recurring_instances;
ALTER TABLE recurring_instances_v7 RENAME TO recurring_instances;
CREATE INDEX recurring_due ON recurring_templates(enabled,next_date);
CREATE TABLE source_evidence_v7 (
 id INTEGER PRIMARY KEY,
 transaction_id TEXT NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
 source_kind TEXT NOT NULL CHECK(source_kind IN ('alipay','wechat','bank','notification','manual','recurring')),
 external_transaction_id TEXT,
 import_batch_id TEXT NOT NULL REFERENCES import_batches(id),
 source_fields_digest BLOB NOT NULL CHECK(length(source_fields_digest)=32),
 UNIQUE(source_kind,external_transaction_id)
);
INSERT INTO source_evidence_v7 SELECT * FROM source_evidence;
DROP TABLE source_evidence;
ALTER TABLE source_evidence_v7 RENAME TO source_evidence;
CREATE INDEX source_evidence_transaction ON source_evidence(transaction_id);
