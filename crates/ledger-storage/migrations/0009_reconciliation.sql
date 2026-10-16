-- Existing placeholder sessions remain readable but cannot be used as complete snapshots.
ALTER TABLE reconciliation_sessions ADD COLUMN created_at_utc_micros INTEGER NOT NULL DEFAULT 0;
ALTER TABLE reconciliation_sessions ADD COLUMN version INTEGER NOT NULL DEFAULT 0 CHECK(version >= 0);
ALTER TABLE reconciliation_sessions ADD COLUMN reference_minor INTEGER NOT NULL DEFAULT 0;
ALTER TABLE reconciliation_sessions ADD COLUMN reference_at_utc_micros INTEGER NOT NULL DEFAULT 0;
ALTER TABLE reconciliation_sessions ADD COLUMN currency_code TEXT NOT NULL DEFAULT 'CNY';
ALTER TABLE reconciliation_sessions ADD COLUMN currency_exponent INTEGER NOT NULL DEFAULT 2;
ALTER TABLE reconciliation_sessions ADD COLUMN period_from_utc_micros INTEGER;
ALTER TABLE reconciliation_sessions ADD COLUMN snapshot_ready INTEGER NOT NULL DEFAULT 0 CHECK(snapshot_ready IN (0,1));
CREATE INDEX reconciliation_account_cutoff ON reconciliation_sessions(account_id, cutoff_utc_micros DESC, id);
CREATE TABLE reconciliation_entries (
 session_id TEXT NOT NULL REFERENCES reconciliation_sessions(id),
 transaction_id TEXT NOT NULL REFERENCES transactions(id),
 occurred_at_utc_micros INTEGER NOT NULL,
 kind TEXT NOT NULL, status TEXT NOT NULL, delta_minor INTEGER NOT NULL,
 merchant TEXT, description TEXT,
 reviewed INTEGER NOT NULL DEFAULT 0 CHECK(reviewed IN (0,1)),
 PRIMARY KEY(session_id,transaction_id)
);
CREATE TABLE reconciliation_events (
 id INTEGER PRIMARY KEY,
 session_id TEXT NOT NULL REFERENCES reconciliation_sessions(id),
 occurred_at_utc_micros INTEGER NOT NULL, action TEXT NOT NULL, reason TEXT
);
CREATE TRIGGER reconciliation_snapshot_immutable
BEFORE UPDATE OF account_id,cutoff_utc_micros,expected_minor,actual_minor,created_at_utc_micros,
 reference_minor,reference_at_utc_micros,currency_code,currency_exponent,period_from_utc_micros,snapshot_ready
ON reconciliation_sessions BEGIN SELECT RAISE(ABORT,'reconciliation snapshot is immutable'); END;
CREATE TRIGGER reconciliation_entry_immutable
BEFORE UPDATE OF session_id,transaction_id,occurred_at_utc_micros,kind,status,delta_minor,merchant,description
ON reconciliation_entries BEGIN SELECT RAISE(ABORT,'reconciliation entry is immutable'); END;
-- Applied adjustments belong to their reconciliation; generic ledger editing/reversal cannot detach them.
CREATE TRIGGER reconciliation_adjustment_immutable BEFORE UPDATE ON transactions
WHEN EXISTS(SELECT 1 FROM reconciliation_sessions WHERE adjustment_id=OLD.id)
BEGIN SELECT RAISE(ABORT,'对账调整不可编辑或冲销，请新建对账会话'); END;
CREATE TRIGGER reconciliation_adjustment_postings_immutable BEFORE DELETE ON postings
WHEN EXISTS(SELECT 1 FROM reconciliation_sessions WHERE adjustment_id=OLD.transaction_id)
BEGIN SELECT RAISE(ABORT,'对账调整不可编辑或冲销，请新建对账会话'); END;
CREATE TRIGGER reconciliation_adjustment_no_reversal BEFORE INSERT ON transaction_metadata
WHEN NEW.reversal_of IS NOT NULL AND EXISTS(SELECT 1 FROM reconciliation_sessions WHERE adjustment_id=NEW.reversal_of)
BEGIN SELECT RAISE(ABORT,'对账调整不可编辑或冲销，请新建对账会话'); END;
CREATE TRIGGER reconciliation_adjustment_no_reversal_update BEFORE UPDATE OF reversal_of ON transaction_metadata
WHEN NEW.reversal_of IS NOT NULL AND EXISTS(SELECT 1 FROM reconciliation_sessions WHERE adjustment_id=NEW.reversal_of)
BEGIN SELECT RAISE(ABORT,'对账调整不可编辑或冲销，请新建对账会话'); END;
