CREATE TABLE archived_client_tun_settings LIKE tun_settings;

INSERT INTO archived_client_tun_settings
SELECT * FROM tun_settings;

DROP TABLE tun_settings;

CREATE TABLE archived_client_fake_ip_settings (
    revision_id BIGINT NOT NULL PRIMARY KEY,
    fake_ip_enabled BOOLEAN NOT NULL,
    fake_ip_pool VARCHAR(64) NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

INSERT INTO archived_client_fake_ip_settings (
    revision_id,
    fake_ip_enabled,
    fake_ip_pool
)
SELECT revision_id, fake_ip_enabled, fake_ip_pool
FROM dns_config;

ALTER TABLE dns_config
    DROP COLUMN fake_ip_enabled,
    DROP COLUMN fake_ip_pool;

UPDATE blackwire_schema_version
SET version = 12, updated_at = UTC_TIMESTAMP(6)
WHERE singleton_id = 1;
