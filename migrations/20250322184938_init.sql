CREATE TABLE files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    stored_name TEXT NOT NULL,
    size INTEGER NOT NULL,
    type TEXT NOT NULL,
    created_at TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    crc32 TEXT NOT NULL
);

CREATE TABLE downloads (
    file_id INTEGER NOT NULL UNIQUE,
    count INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (file_id) REFERENCES files (id)
);