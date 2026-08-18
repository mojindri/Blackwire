ALTER TABLE inbound_protocol_settings
    ADD COLUMN network VARCHAR(32) NULL,
    ADD COLUMN auth_timeout_ms BIGINT UNSIGNED NULL;

UPDATE blackwire_schema_version SET version = 8, updated_at = UTC_TIMESTAMP(6) WHERE singleton_id = 1;
