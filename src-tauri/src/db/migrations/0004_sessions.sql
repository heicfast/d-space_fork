-- DiskWizard v0.4.0 — persisted recovery scan sessions.
-- One row per auto-saved or explicitly saved scan; the full result set
-- lives in %LOCALAPPDATA%\DiskWizard\sessions\<id>.dws next to the DB.
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    file_name TEXT NOT NULL,
    target_label TEXT NOT NULL,
    volume_id TEXT NOT NULL,
    saved_at_unix INTEGER NOT NULL,
    files_found INTEGER NOT NULL DEFAULT 0,
    data_found_bytes INTEGER NOT NULL DEFAULT 0,
    deleted_found INTEGER NOT NULL DEFAULT 0,
    completed INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_sessions_saved_at ON sessions(saved_at_unix DESC);
