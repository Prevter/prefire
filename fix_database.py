"""
This file does some operations on the database to clean up deleted files and fix file hashes.
"""
import os
import sys
import sqlite3
from blake3 import blake3

UPLOADS_DIR = "uploads"

def main(db_path):
    print(f"Opening database at {db_path}")
    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()

    # 1. Check "files" table and verify files exist
    cursor.execute("SELECT id, stored_name FROM files")
    files = cursor.fetchall()
    for id, stored_name in files:
        if not os.path.exists(f"{UPLOADS_DIR}/{stored_name}"):
            print(f"File {stored_name} (ID: {id}) does not exist, deleting from database.")
            cursor.execute("DELETE FROM files WHERE id = ?", (id,))

    # 2. Do the opposite: check if files exist in the database and delete them if not
    for filename in os.listdir(UPLOADS_DIR):
        file_path = os.path.join(UPLOADS_DIR, filename)
        if os.path.isfile(file_path):
            cursor.execute("SELECT id FROM files WHERE stored_name = ?", (filename,))
            if cursor.fetchone() is None:
                print(f"File {filename} exists in uploads but not in database, deleting.")
                os.remove(file_path)

    # 3. Fix file hashes
    cursor.execute("SELECT id, stored_name, hash FROM files")
    files = cursor.fetchall()
    for id, stored_name, hash_value in files:
        if hash_value is None or hash_value == "":
            print(f"File {stored_name} (ID: {id}) has no hash, calculating hash.")
            file_path = os.path.join(UPLOADS_DIR, stored_name)
            if os.path.exists(file_path):
                with open(file_path, "rb") as f:
                    file_hash = blake3(f.read()).hexdigest()
                print(f"Updating hash for file {stored_name} (ID: {id}) to {file_hash}.")
                cursor.execute("UPDATE files SET hash = ? WHERE id = ?", (file_hash, id))
            else:
                print(f"File {stored_name} (ID: {id}) does not exist, skipping hash update.")

    print("Committing changes to the database.")
    conn.commit()

if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("Usage: python fix_database.py <database_path>")
        sys.exit(1)

    database_path = sys.argv[1]

    if not os.path.exists(database_path):
        print(f"Database file '{database_path}' does not exist.")
        sys.exit(1)

    try:
        main(database_path)
    except Exception as e:
        print(f"An error occurred: {e}")
        sys.exit(1)