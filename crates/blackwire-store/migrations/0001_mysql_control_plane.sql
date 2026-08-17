CREATE TABLE blackwire_schema_version (
    singleton_id TINYINT UNSIGNED NOT NULL PRIMARY KEY,
    version BIGINT NOT NULL,
    updated_at DATETIME(6) NOT NULL,
    CONSTRAINT chk_schema_singleton CHECK (singleton_id = 1)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE configuration_revisions (
    revision BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    parent_revision BIGINT NULL,
    actor VARCHAR(191) NOT NULL,
    summary VARCHAR(512) NOT NULL,
    activation_class VARCHAR(32) NOT NULL,
    created_at DATETIME(6) NOT NULL,
    CONSTRAINT fk_revision_parent FOREIGN KEY (parent_revision) REFERENCES configuration_revisions(revision),
    CONSTRAINT chk_activation_class CHECK (activation_class IN ('hot_swap', 'listener_handover', 'maintenance_required'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE configuration_state (
    singleton_id TINYINT UNSIGNED NOT NULL PRIMARY KEY,
    desired_revision BIGINT NOT NULL,
    active_revision BIGINT NULL,
    pending_maintenance_revision BIGINT NULL,
    activation_state VARCHAR(32) NOT NULL,
    last_error TEXT NULL,
    updated_at DATETIME(6) NOT NULL,
    CONSTRAINT chk_configuration_singleton CHECK (singleton_id = 1),
    CONSTRAINT chk_activation_state CHECK (activation_state IN ('active', 'activating', 'pending_maintenance', 'failed')),
    CONSTRAINT fk_desired_revision FOREIGN KEY (desired_revision) REFERENCES configuration_revisions(revision),
    CONSTRAINT fk_active_revision FOREIGN KEY (active_revision) REFERENCES configuration_revisions(revision),
    CONSTRAINT fk_pending_revision FOREIGN KEY (pending_maintenance_revision) REFERENCES configuration_revisions(revision)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE runtime_instances (
    instance_id VARCHAR(191) NOT NULL PRIMARY KEY,
    active_revision BIGINT NULL,
    state VARCHAR(32) NOT NULL,
    last_error TEXT NULL,
    heartbeat_at DATETIME(6) NOT NULL,
    CONSTRAINT fk_runtime_revision FOREIGN KEY (active_revision) REFERENCES configuration_revisions(revision)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE global_config (
    revision_id BIGINT NOT NULL PRIMARY KEY,
    profile VARCHAR(16) NOT NULL,
    metrics_enabled BOOLEAN NOT NULL,
    metrics_address VARCHAR(255) NULL,
    api_enabled BOOLEAN NOT NULL,
    api_listen_address VARCHAR(255) NULL,
    log_level VARCHAR(16) NOT NULL,
    log_structured BOOLEAN NOT NULL,
    log_file VARCHAR(1024) NOT NULL,
    CONSTRAINT fk_global_revision FOREIGN KEY (revision_id) REFERENCES configuration_revisions(revision) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE global_limits (
    revision_id BIGINT NOT NULL PRIMARY KEY,
    max_connections BIGINT UNSIGNED NULL,
    max_connections_per_inbound BIGINT UNSIGNED NULL,
    max_connections_per_user BIGINT UNSIGNED NULL,
    max_handshake_seconds BIGINT UNSIGNED NULL,
    max_idle_seconds BIGINT UNSIGNED NULL,
    CONSTRAINT fk_limits_revision FOREIGN KEY (revision_id) REFERENCES configuration_revisions(revision) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE inbounds (
    revision_id BIGINT NOT NULL,
    inbound_id BIGINT NOT NULL,
    tag VARCHAR(191) NOT NULL,
    listen_address VARCHAR(255) NOT NULL,
    listen_port INT UNSIGNED NOT NULL,
    protocol VARCHAR(32) NOT NULL,
    enabled BOOLEAN NOT NULL,
    position INT UNSIGNED NOT NULL,
    PRIMARY KEY (revision_id, inbound_id),
    UNIQUE KEY uq_inbound_tag (revision_id, tag),
    CONSTRAINT chk_inbound_port CHECK (listen_port BETWEEN 1 AND 65535),
    CONSTRAINT fk_inbound_revision FOREIGN KEY (revision_id) REFERENCES configuration_revisions(revision) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE outbounds (
    revision_id BIGINT NOT NULL,
    outbound_id BIGINT NOT NULL,
    tag VARCHAR(191) NOT NULL,
    protocol VARCHAR(32) NOT NULL,
    enabled BOOLEAN NOT NULL,
    position INT UNSIGNED NOT NULL,
    server_address VARCHAR(255) NULL,
    server_port INT UNSIGNED NULL,
    domain_strategy VARCHAR(32) NULL,
    deny_loopback BOOLEAN NULL,
    reject_ipv6_literal BOOLEAN NULL,
    PRIMARY KEY (revision_id, outbound_id),
    UNIQUE KEY uq_outbound_tag (revision_id, tag),
    CONSTRAINT chk_outbound_port CHECK (server_port IS NULL OR server_port BETWEEN 1 AND 65535),
    CONSTRAINT fk_outbound_revision FOREIGN KEY (revision_id) REFERENCES configuration_revisions(revision) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE stream_settings (
    revision_id BIGINT NOT NULL,
    endpoint_kind VARCHAR(16) NOT NULL,
    endpoint_id BIGINT NOT NULL,
    network VARCHAR(32) NOT NULL,
    security VARCHAR(32) NOT NULL,
    PRIMARY KEY (revision_id, endpoint_kind, endpoint_id),
    CONSTRAINT chk_endpoint_kind CHECK (endpoint_kind IN ('inbound', 'outbound')),
    CONSTRAINT fk_stream_revision FOREIGN KEY (revision_id) REFERENCES configuration_revisions(revision) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE tls_settings (
    revision_id BIGINT NOT NULL,
    endpoint_kind VARCHAR(16) NOT NULL,
    endpoint_id BIGINT NOT NULL,
    server_name VARCHAR(255) NOT NULL,
    allow_insecure BOOLEAN NOT NULL,
    certificate_file VARCHAR(1024) NOT NULL,
    key_file VARCHAR(1024) NOT NULL,
    PRIMARY KEY (revision_id, endpoint_kind, endpoint_id),
    CONSTRAINT fk_tls_stream FOREIGN KEY (revision_id, endpoint_kind, endpoint_id) REFERENCES stream_settings(revision_id, endpoint_kind, endpoint_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE tls_alpn (
    revision_id BIGINT NOT NULL,
    endpoint_kind VARCHAR(16) NOT NULL,
    endpoint_id BIGINT NOT NULL,
    position INT UNSIGNED NOT NULL,
    protocol VARCHAR(64) NOT NULL,
    PRIMARY KEY (revision_id, endpoint_kind, endpoint_id, position),
    CONSTRAINT fk_alpn_tls FOREIGN KEY (revision_id, endpoint_kind, endpoint_id) REFERENCES tls_settings(revision_id, endpoint_kind, endpoint_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE reality_settings (
    revision_id BIGINT NOT NULL,
    endpoint_kind VARCHAR(16) NOT NULL,
    endpoint_id BIGINT NOT NULL,
    show_details BOOLEAN NOT NULL,
    destination VARCHAR(255) NOT NULL,
    private_key VARCHAR(255) NOT NULL,
    public_key VARCHAR(255) NOT NULL,
    short_id VARCHAR(255) NOT NULL,
    fingerprint VARCHAR(64) NOT NULL,
    server_name VARCHAR(255) NOT NULL,
    max_time_diff_seconds BIGINT UNSIGNED NULL,
    PRIMARY KEY (revision_id, endpoint_kind, endpoint_id),
    CONSTRAINT fk_reality_stream FOREIGN KEY (revision_id, endpoint_kind, endpoint_id) REFERENCES stream_settings(revision_id, endpoint_kind, endpoint_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE reality_server_names (
    revision_id BIGINT NOT NULL,
    endpoint_kind VARCHAR(16) NOT NULL,
    endpoint_id BIGINT NOT NULL,
    position INT UNSIGNED NOT NULL,
    server_name VARCHAR(255) NOT NULL,
    PRIMARY KEY (revision_id, endpoint_kind, endpoint_id, position),
    CONSTRAINT fk_reality_name FOREIGN KEY (revision_id, endpoint_kind, endpoint_id) REFERENCES reality_settings(revision_id, endpoint_kind, endpoint_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE reality_short_ids (
    revision_id BIGINT NOT NULL,
    endpoint_kind VARCHAR(16) NOT NULL,
    endpoint_id BIGINT NOT NULL,
    position INT UNSIGNED NOT NULL,
    short_id VARCHAR(255) NOT NULL,
    PRIMARY KEY (revision_id, endpoint_kind, endpoint_id, position),
    CONSTRAINT fk_reality_short_id FOREIGN KEY (revision_id, endpoint_kind, endpoint_id) REFERENCES reality_settings(revision_id, endpoint_kind, endpoint_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE users (
    revision_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    inbound_id BIGINT NOT NULL,
    email VARCHAR(320) NOT NULL,
    enabled BOOLEAN NOT NULL,
    flow VARCHAR(64) NOT NULL,
    note TEXT NOT NULL,
    traffic_limit_bytes BIGINT NULL,
    expiry_at DATETIME(6) NULL,
    subscription_token VARCHAR(191) NOT NULL,
    PRIMARY KEY (revision_id, user_id),
    UNIQUE KEY uq_user_email (revision_id, email),
    UNIQUE KEY uq_user_subscription (revision_id, subscription_token),
    CONSTRAINT fk_user_inbound FOREIGN KEY (revision_id, inbound_id) REFERENCES inbounds(revision_id, inbound_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE user_credentials (
    revision_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    credential_kind VARCHAR(32) NOT NULL,
    uuid_value CHAR(36) NULL,
    password_value VARBINARY(1024) NULL,
    method VARCHAR(64) NULL,
    auth_value VARBINARY(1024) NULL,
    PRIMARY KEY (revision_id, user_id),
    CONSTRAINT fk_credential_user FOREIGN KEY (revision_id, user_id) REFERENCES users(revision_id, user_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE dns_config (
    revision_id BIGINT NOT NULL PRIMARY KEY,
    enabled BOOLEAN NOT NULL,
    fake_ip_enabled BOOLEAN NOT NULL,
    fake_ip_pool VARCHAR(64) NULL,
    CONSTRAINT fk_dns_revision FOREIGN KEY (revision_id) REFERENCES configuration_revisions(revision) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE dns_servers (
    revision_id BIGINT NOT NULL,
    position INT UNSIGNED NOT NULL,
    address VARCHAR(1024) NOT NULL,
    PRIMARY KEY (revision_id, position),
    CONSTRAINT fk_dns_server_revision FOREIGN KEY (revision_id) REFERENCES dns_config(revision_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE routing_config (
    revision_id BIGINT NOT NULL PRIMARY KEY,
    enabled BOOLEAN NOT NULL,
    domain_strategy VARCHAR(64) NULL,
    geoip_file VARCHAR(1024) NULL,
    geosite_file VARCHAR(1024) NULL,
    CONSTRAINT fk_routing_revision FOREIGN KEY (revision_id) REFERENCES configuration_revisions(revision) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE routing_rules (
    revision_id BIGINT NOT NULL,
    rule_id BIGINT NOT NULL,
    position INT UNSIGNED NOT NULL,
    rule_type VARCHAR(32) NOT NULL,
    port_expression VARCHAR(255) NULL,
    outbound_id BIGINT NOT NULL,
    PRIMARY KEY (revision_id, rule_id),
    UNIQUE KEY uq_routing_position (revision_id, position),
    CONSTRAINT fk_rule_routing FOREIGN KEY (revision_id) REFERENCES routing_config(revision_id) ON DELETE CASCADE,
    CONSTRAINT fk_rule_outbound FOREIGN KEY (revision_id, outbound_id) REFERENCES outbounds(revision_id, outbound_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE routing_rule_values (
    revision_id BIGINT NOT NULL,
    rule_id BIGINT NOT NULL,
    value_kind VARCHAR(32) NOT NULL,
    position INT UNSIGNED NOT NULL,
    value_text VARCHAR(1024) NOT NULL,
    PRIMARY KEY (revision_id, rule_id, value_kind, position),
    CONSTRAINT chk_rule_value_kind CHECK (value_kind IN ('domain', 'ip', 'inbound_tag', 'protocol', 'user')),
    CONSTRAINT fk_rule_value FOREIGN KEY (revision_id, rule_id) REFERENCES routing_rules(revision_id, rule_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE panel_admins (
    admin_id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    username VARCHAR(191) NOT NULL UNIQUE,
    password_hash VARBINARY(255) NOT NULL,
    password_salt VARBINARY(255) NOT NULL,
    created_at DATETIME(6) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE panel_sessions (
    token_hash BINARY(32) NOT NULL PRIMARY KEY,
    admin_id BIGINT NOT NULL,
    created_at DATETIME(6) NOT NULL,
    expires_at DATETIME(6) NOT NULL,
    CONSTRAINT fk_session_admin FOREIGN KEY (admin_id) REFERENCES panel_admins(admin_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE user_traffic (
    user_id BIGINT NOT NULL PRIMARY KEY,
    upload_bytes BIGINT UNSIGNED NOT NULL DEFAULT 0,
    download_bytes BIGINT UNSIGNED NOT NULL DEFAULT 0,
    last_runtime_upload_bytes BIGINT UNSIGNED NOT NULL DEFAULT 0,
    last_runtime_download_bytes BIGINT UNSIGNED NOT NULL DEFAULT 0,
    updated_at DATETIME(6) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

INSERT INTO blackwire_schema_version (singleton_id, version, updated_at)
VALUES (1, 1, UTC_TIMESTAMP(6));

INSERT INTO configuration_revisions
    (revision, parent_revision, actor, summary, activation_class, created_at)
VALUES
    (1, NULL, 'system', 'Initial idle configuration', 'hot_swap', UTC_TIMESTAMP(6));

INSERT INTO configuration_state
    (singleton_id, desired_revision, active_revision, pending_maintenance_revision, activation_state, last_error, updated_at)
VALUES
    (1, 1, NULL, NULL, 'activating', NULL, UTC_TIMESTAMP(6));

INSERT INTO global_config
    (revision_id, profile, metrics_enabled, metrics_address, api_enabled, api_listen_address, log_level, log_structured, log_file)
VALUES
    (1, 'compat', FALSE, NULL, TRUE, '127.0.0.1:62789', 'info', FALSE, '');

INSERT INTO global_limits
    (revision_id, max_connections, max_connections_per_inbound, max_connections_per_user, max_handshake_seconds, max_idle_seconds)
VALUES
    (1, NULL, NULL, NULL, NULL, NULL);

INSERT INTO dns_config (revision_id, enabled, fake_ip_enabled, fake_ip_pool)
VALUES (1, TRUE, FALSE, NULL);

INSERT INTO dns_servers (revision_id, position, address)
VALUES (1, 0, '1.1.1.1'), (1, 1, '8.8.8.8');

INSERT INTO outbounds
    (revision_id, outbound_id, tag, protocol, enabled, position, server_address, server_port, domain_strategy, deny_loopback, reject_ipv6_literal)
VALUES
    (1, 1, 'freedom', 'freedom', TRUE, 0, NULL, NULL, 'PreferIPv4', TRUE, FALSE);

INSERT INTO routing_config (revision_id, enabled, domain_strategy, geoip_file, geosite_file)
VALUES (1, TRUE, NULL, NULL, NULL);

INSERT INTO routing_rules
    (revision_id, rule_id, position, rule_type, port_expression, outbound_id)
VALUES
    (1, 1, 0, 'field', NULL, 1);
