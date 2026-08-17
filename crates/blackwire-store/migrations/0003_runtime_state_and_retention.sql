ALTER TABLE configuration_revisions DROP FOREIGN KEY fk_revision_parent;
ALTER TABLE configuration_revisions
    ADD CONSTRAINT fk_revision_parent FOREIGN KEY (parent_revision)
    REFERENCES configuration_revisions(revision) ON DELETE SET NULL;

CREATE TABLE inbound_traffic (
    inbound_id BIGINT NOT NULL PRIMARY KEY,
    upload_bytes BIGINT UNSIGNED NOT NULL DEFAULT 0,
    download_bytes BIGINT UNSIGNED NOT NULL DEFAULT 0,
    last_runtime_upload_bytes BIGINT UNSIGNED NOT NULL DEFAULT 0,
    last_runtime_download_bytes BIGINT UNSIGNED NOT NULL DEFAULT 0,
    updated_at DATETIME(6) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE enforcement_state (
    user_id BIGINT NOT NULL PRIMARY KEY,
    status VARCHAR(32) NOT NULL,
    reason VARCHAR(255) NULL,
    evaluated_at DATETIME(6) NOT NULL,
    CONSTRAINT chk_enforcement_status CHECK (status IN ('current', 'expired', 'traffic_limited', 'disabled'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

UPDATE blackwire_schema_version SET version = 3, updated_at = UTC_TIMESTAMP(6) WHERE singleton_id = 1;
