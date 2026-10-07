CREATE TABLE mobile_settings (
 id INTEGER PRIMARY KEY CHECK(id=1), notification_wechat INTEGER NOT NULL DEFAULT 0,
 notification_alipay INTEGER NOT NULL DEFAULT 0, import_wechat INTEGER NOT NULL DEFAULT 0,
 import_alipay INTEGER NOT NULL DEFAULT 0, import_bank INTEGER NOT NULL DEFAULT 0,
 api_enabled INTEGER NOT NULL DEFAULT 0, backup_enabled INTEGER NOT NULL DEFAULT 0,
 default_account TEXT REFERENCES accounts(id) ON DELETE SET NULL
);
INSERT INTO mobile_settings(id) VALUES(1);
CREATE TABLE pending_details (
 transaction_id TEXT PRIMARY KEY REFERENCES transactions(id) ON DELETE CASCADE,
 amount_minor INTEGER, currency_code TEXT, currency_exponent INTEGER,
 source TEXT NOT NULL, account_hint TEXT, original_external_id TEXT
);
