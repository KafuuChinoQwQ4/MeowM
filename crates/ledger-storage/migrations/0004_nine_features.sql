CREATE INDEX pending_review ON transactions(status, occurred_at_utc_micros DESC, id);
CREATE INDEX postings_account ON postings(account_id, transaction_id);
CREATE INDEX transactions_category ON transactions(category_id, occurred_at_utc_micros);
CREATE TABLE transaction_metadata (
 transaction_id TEXT PRIMARY KEY REFERENCES transactions(id), updated_at_utc_micros INTEGER NOT NULL,
 edit_version INTEGER NOT NULL DEFAULT 0 CHECK(edit_version >= 0),
 reversal_of TEXT UNIQUE REFERENCES transactions(id), report_income_minor INTEGER, report_expense_minor INTEGER,
 CHECK ((report_income_minor IS NULL) = (report_expense_minor IS NULL)), CHECK(reversal_of IS NULL OR reversal_of <> transaction_id)
);
INSERT INTO transaction_metadata(transaction_id,updated_at_utc_micros) SELECT id,occurred_at_utc_micros FROM transactions;
CREATE TABLE budgets(id TEXT PRIMARY KEY, month TEXT NOT NULL, account_id TEXT REFERENCES accounts(id),from_date TEXT,until_date TEXT,enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)), category_id TEXT REFERENCES categories(id), amount_cny_minor INTEGER NOT NULL CHECK(amount_cny_minor>=0));
CREATE UNIQUE INDEX budgets_scope ON budgets(month,ifnull(category_id,''),ifnull(account_id,''));
CREATE TABLE recurring_templates(id TEXT PRIMARY KEY, account_id TEXT NOT NULL REFERENCES accounts(id),category_id TEXT REFERENCES categories(id),kind TEXT NOT NULL CHECK(kind IN ('income','expense')),amount_cny_minor INTEGER NOT NULL CHECK(amount_cny_minor>0),note TEXT,merchant TEXT,frequency TEXT NOT NULL DEFAULT 'monthly' CHECK(frequency IN ('weekly','monthly','yearly')),anchor_date TEXT,end_date TEXT, next_date TEXT NOT NULL, interval_months INTEGER NOT NULL CHECK(interval_months>0),enabled INTEGER NOT NULL CHECK(enabled IN (0,1)));
CREATE TABLE recurring_instances(template_id TEXT NOT NULL REFERENCES recurring_templates(id),scheduled_date TEXT NOT NULL,transaction_id TEXT NOT NULL UNIQUE REFERENCES transactions(id),PRIMARY KEY(template_id,scheduled_date));
CREATE TABLE reconciliation_sessions(id TEXT PRIMARY KEY,account_id TEXT NOT NULL REFERENCES accounts(id),cutoff_utc_micros INTEGER NOT NULL,expected_minor INTEGER NOT NULL,actual_minor INTEGER NOT NULL,adjustment_id TEXT UNIQUE REFERENCES transactions(id),status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','reviewed','closed')));
CREATE TABLE notification_settings(package_name TEXT PRIMARY KEY,enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),account_id TEXT REFERENCES accounts(id));
CREATE TABLE app_settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
CREATE TABLE audit_events(id TEXT PRIMARY KEY,occurred_at_utc_micros INTEGER NOT NULL,action TEXT NOT NULL,transaction_id TEXT REFERENCES transactions(id),rule_id TEXT REFERENCES classification_rules(id),import_batch_id TEXT REFERENCES import_batches(id));
