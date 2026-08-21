DROP TABLE IF EXISTS kcp_settings;

UPDATE blackwire_schema_version
SET version = 11, updated_at = UTC_TIMESTAMP(6)
WHERE singleton_id = 1;
