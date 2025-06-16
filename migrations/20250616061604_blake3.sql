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

-- 4. move downloads table to new table
ALTER TABLE downloads RENAME TO downloads_old;

-- 5. create a new downloads table with file_id as a foreign key
CREATE TABLE downloads (
    file_id INTEGER NOT NULL UNIQUE,
    count INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (file_id) REFERENCES files (id)
);

-- 6. copy data from old downloads table to new downloads table
INSERT INTO downloads (file_id, count)
SELECT file_id, count FROM downloads_old;

-- 7. drop old tables
DROP TABLE files_old;
DROP TABLE downloads_old;