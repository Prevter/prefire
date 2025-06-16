-- 1. rename old table
ALTER TABLE files RENAME TO files_old;

-- 2. create a new table with hash column
CREATE TABLE files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    stored_name TEXT NOT NULL,
    size INTEGER NOT NULL,
    type TEXT NOT NULL,
    created_at TEXT NOT NULL,
    hash TEXT NOT NULL
);

-- 3. copy data from old table to new table
INSERT INTO files (id, name, stored_name, size, type, created_at, hash)
SELECT id, name, stored_name, size, type, created_at, '' AS hash FROM files_old;

-- 4. drop old table
DROP TABLE files_old;