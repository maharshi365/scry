CREATE TABLE local_roots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL UNIQUE,
    scan_generation INTEGER NOT NULL DEFAULT 0,
    online BOOLEAN NOT NULL DEFAULT 1
);
CREATE TABLE local_files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL REFERENCES local_roots(id) ON DELETE CASCADE,
    relative_path TEXT NOT NULL,
    size INTEGER NOT NULL,
    modified INTEGER NOT NULL,
    hints TEXT NOT NULL,
    seen INTEGER NOT NULL,
    available BOOLEAN NOT NULL DEFAULT 1,
    namespace TEXT,
    kind TEXT,
    external_id TEXT,
    title TEXT,
    metadata TEXT,
    manual BOOLEAN NOT NULL DEFAULT 0,
    UNIQUE(root_id, relative_path)
);
CREATE INDEX local_files_identity ON local_files(namespace, kind, external_id, available);
CREATE TABLE local_groups (
    root_id INTEGER NOT NULL REFERENCES local_roots(id) ON DELETE CASCADE,
    folder TEXT NOT NULL,
    title_hint TEXT NOT NULL,
    namespace TEXT NOT NULL,
    metadata TEXT NOT NULL,
    PRIMARY KEY(root_id, folder, title_hint)
);
CREATE TABLE local_titles (
    namespace TEXT NOT NULL,
    kind TEXT NOT NULL,
    external_id TEXT NOT NULL,
    metadata TEXT NOT NULL,
    PRIMARY KEY(namespace, kind, external_id)
);
