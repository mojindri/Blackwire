DROP TABLE global_api_services;

ALTER TABLE global_config
    DROP COLUMN api_token_value,
    DROP COLUMN api_listen_address,
    DROP COLUMN api_enabled;

UPDATE blackwire_schema_version
SET version = 14, updated_at = UTC_TIMESTAMP(6)
WHERE singleton_id = 1;
