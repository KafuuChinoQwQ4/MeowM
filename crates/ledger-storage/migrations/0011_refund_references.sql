-- Optional original order reference from a refund bill, without retaining raw rows.
ALTER TABLE source_evidence ADD COLUMN original_external_transaction_id TEXT;
