-- 0001: foundation. The full clinic schema arrives in Phase 1 as further migrations.
-- Migrations are forward-only and run automatically when the app starts.

CREATE TABLE app_setting (
    key         TEXT PRIMARY KEY NOT NULL,
    value_json  TEXT NOT NULL,
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

INSERT INTO app_setting (key, value_json) VALUES ('clinic.name', '"SkinDocJyotsna"');
