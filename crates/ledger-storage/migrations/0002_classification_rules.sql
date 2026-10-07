-- Classification rules and editable category state.
ALTER TABLE categories ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1));
ALTER TABLE transactions ADD COLUMN merchant TEXT;
ALTER TABLE transactions ADD COLUMN description TEXT;

CREATE TABLE classification_rules (
    id TEXT PRIMARY KEY NOT NULL,
    priority INTEGER NOT NULL CHECK (priority >= 0),
    merchant_contains TEXT,
    description_contains TEXT,
    category_id TEXT NOT NULL REFERENCES categories(id),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1))
);

CREATE INDEX classification_rules_priority_idx
    ON classification_rules(priority, id);
