-- Merchant order references are advisory, never a replacement for provider transaction IDs.
ALTER TABLE source_evidence ADD COLUMN merchant_order_id TEXT;
CREATE INDEX source_evidence_merchant_order ON source_evidence(source_kind, merchant_order_id);
