ALTER TABLE export_history ADD COLUMN sql_text TEXT NOT NULL DEFAULT '';
ALTER TABLE export_history ADD COLUMN options_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE export_history ADD COLUMN duration_ms INTEGER;
ALTER TABLE export_history ADD COLUMN rows_written INTEGER NOT NULL DEFAULT 0 CHECK(rows_written >= 0);
ALTER TABLE export_history ADD COLUMN files_written INTEGER NOT NULL DEFAULT 0 CHECK(files_written >= 0);
ALTER TABLE export_history ADD COLUMN bytes_written INTEGER NOT NULL DEFAULT 0 CHECK(bytes_written >= 0);
ALTER TABLE export_history ADD COLUMN error_code TEXT;
