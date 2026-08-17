ALTER TABLE reality_settings
    ADD COLUMN fallback_upload_after_bytes BIGINT UNSIGNED NULL,
    ADD COLUMN fallback_upload_bytes_per_sec BIGINT UNSIGNED NULL,
    ADD COLUMN fallback_upload_burst_bytes_per_sec BIGINT UNSIGNED NULL,
    ADD COLUMN fallback_download_after_bytes BIGINT UNSIGNED NULL,
    ADD COLUMN fallback_download_bytes_per_sec BIGINT UNSIGNED NULL,
    ADD COLUMN fallback_download_burst_bytes_per_sec BIGINT UNSIGNED NULL;
