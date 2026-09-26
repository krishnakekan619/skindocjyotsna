-- 0002: staff accounts and the audit log (Phase 1).
-- Timestamps are INTEGER Unix seconds (UTC); the UI formats them for display.

CREATE TABLE app_user (
    id                  INTEGER PRIMARY KEY,
    username            TEXT    NOT NULL UNIQUE COLLATE NOCASE CHECK (length(username) BETWEEN 3 AND 32),
    full_name           TEXT    NOT NULL CHECK (length(full_name) BETWEEN 1 AND 80),
    role                TEXT    NOT NULL CHECK (role IN ('ADMIN', 'RECEPTIONIST')),
    password_hash       TEXT    NOT NULL,      -- Argon2id PHC string, never the password
    pin_hash            TEXT,                  -- Argon2id PHC string of the unlock PIN, if set
    is_active           INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    failed_login_count  INTEGER NOT NULL DEFAULT 0 CHECK (failed_login_count >= 0),
    locked_until        INTEGER,               -- set after too many wrong passwords
    pin_failed_count    INTEGER NOT NULL DEFAULT 0 CHECK (pin_failed_count >= 0),
    last_login_at       INTEGER,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL
) STRICT;

-- Sensitive actions (logins, user and settings changes, backups, ...). Append-only:
-- the triggers below make the database itself refuse edits and deletions.
CREATE TABLE audit_log (
    id            INTEGER PRIMARY KEY,
    occurred_at   INTEGER NOT NULL,
    user_id       INTEGER REFERENCES app_user (id) ON DELETE RESTRICT,
    username      TEXT,                        -- who acted (or the name tried, for failed logins)
    action        TEXT    NOT NULL,
    entity_type   TEXT,
    entity_id     TEXT,
    details_json  TEXT                         -- IDs and non-sensitive values only, never patient data
) STRICT;

CREATE INDEX idx_audit_log_occurred_at ON audit_log (occurred_at);
CREATE INDEX idx_audit_log_user ON audit_log (user_id);

CREATE TRIGGER audit_log_no_update BEFORE UPDATE ON audit_log
BEGIN
    SELECT RAISE(ABORT, 'audit_log is append-only');
END;

CREATE TRIGGER audit_log_no_delete BEFORE DELETE ON audit_log
BEGIN
    SELECT RAISE(ABORT, 'audit_log is append-only');
END;
