ALTER TABLE reality_settings
    ADD COLUMN fallback_upload_after_bytes BIGINT UNSIGNED NULL,
    ADD COLUMN fallback_upload_bytes_per_sec BIGINT UNSIGNED NULL,
    ADD COLUMN fallback_upload_burst_bytes_per_sec BIGINT UNSIGNED NULL,
    ADD COLUMN fallback_download_after_bytes BIGINT UNSIGNED NULL,
    ADD COLUMN fallback_download_bytes_per_sec BIGINT UNSIGNED NULL,
    ADD COLUMN fallback_download_burst_bytes_per_sec BIGINT UNSIGNED NULL;

UPDATE blackwire_schema_version SET version = 7, updated_at = UTC_TIMESTAMP(6) WHERE singleton_id = 1;
