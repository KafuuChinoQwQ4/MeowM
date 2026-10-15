-- Only digests and normalized result codes are retained, never notification text/keys.
CREATE TABLE notification_receipts (
 identity TEXT PRIMARY KEY CHECK(length(identity)=64),
 digest BLOB NOT NULL UNIQUE CHECK(length(digest)=32),
 transaction_id TEXT NOT NULL REFERENCES transactions(id)
);
CREATE TABLE notification_status (
 package_name TEXT PRIMARY KEY REFERENCES notification_settings(package_name),
 code TEXT NOT NULL CHECK(code IN ('pending','needs_confirmation','ignored','duplicate','error')),
 received_at_ms INTEGER NOT NULL,
 transaction_id TEXT REFERENCES transactions(id)
);
