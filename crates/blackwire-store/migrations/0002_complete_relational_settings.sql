CREATE TABLE inbound_protocol_settings (
    revision_id BIGINT NOT NULL,
    inbound_id BIGINT NOT NULL,
    decryption VARCHAR(64) NULL,
    method VARCHAR(64) NULL,
    auth_value VARBINARY(1024) NULL,
    up_mbps BIGINT UNSIGNED NULL,
    down_mbps BIGINT UNSIGNED NULL,
    endpoint_shards INT UNSIGNED NULL,
    PRIMARY KEY (revision_id, inbound_id),
    CONSTRAINT fk_inbound_protocol FOREIGN KEY (revision_id, inbound_id) REFERENCES inbounds(revision_id, inbound_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE outbound_protocol_settings (
    revision_id BIGINT NOT NULL,
    outbound_id BIGINT NOT NULL,
    password_value VARBINARY(1024) NULL,
    auth_value VARBINARY(1024) NULL,
    method VARCHAR(64) NULL,
    uuid_value CHAR(36) NULL,
    flow VARCHAR(64) NULL,
    server_name VARCHAR(255) NULL,
    skip_certificate_verify BOOLEAN NULL,
    endpoint_shards INT UNSIGNED NULL,
    PRIMARY KEY (revision_id, outbound_id),
    CONSTRAINT fk_outbound_protocol FOREIGN KEY (revision_id, outbound_id) REFERENCES outbounds(revision_id, outbound_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE websocket_settings (
    revision_id BIGINT NOT NULL,
    endpoint_kind VARCHAR(16) NOT NULL,
    endpoint_id BIGINT NOT NULL,
    transport_kind VARCHAR(16) NOT NULL,
    request_path VARCHAR(2048) NOT NULL,
    PRIMARY KEY (revision_id, endpoint_kind, endpoint_id, transport_kind),
    CONSTRAINT chk_websocket_kind CHECK (transport_kind IN ('ws', 'httpupgrade')),
    CONSTRAINT fk_websocket_stream FOREIGN KEY (revision_id, endpoint_kind, endpoint_id) REFERENCES stream_settings(revision_id, endpoint_kind, endpoint_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE transport_headers (
    revision_id BIGINT NOT NULL,
    endpoint_kind VARCHAR(16) NOT NULL,
    endpoint_id BIGINT NOT NULL,
    transport_kind VARCHAR(16) NOT NULL,
    header_name VARCHAR(255) NOT NULL,
    header_value TEXT NOT NULL,
    PRIMARY KEY (revision_id, endpoint_kind, endpoint_id, transport_kind, header_name),
    CONSTRAINT fk_header_stream FOREIGN KEY (revision_id, endpoint_kind, endpoint_id) REFERENCES stream_settings(revision_id, endpoint_kind, endpoint_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE grpc_settings (
    revision_id BIGINT NOT NULL,
    endpoint_kind VARCHAR(16) NOT NULL,
    endpoint_id BIGINT NOT NULL,
    service_name VARCHAR(255) NOT NULL,
    multi_mode BOOLEAN NOT NULL,
    PRIMARY KEY (revision_id, endpoint_kind, endpoint_id),
    CONSTRAINT fk_grpc_stream FOREIGN KEY (revision_id, endpoint_kind, endpoint_id) REFERENCES stream_settings(revision_id, endpoint_kind, endpoint_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE kcp_settings (
    revision_id BIGINT NOT NULL,
    endpoint_kind VARCHAR(16) NOT NULL,
    endpoint_id BIGINT NOT NULL,
    header_type VARCHAR(32) NOT NULL,
    mtu INT UNSIGNED NOT NULL,
    tti_ms BIGINT UNSIGNED NOT NULL,
    uplink_capacity INT UNSIGNED NOT NULL,
    downlink_capacity INT UNSIGNED NOT NULL,
    congestion BOOLEAN NOT NULL,
    read_buffer_size INT UNSIGNED NOT NULL,
    write_buffer_size INT UNSIGNED NOT NULL,
    PRIMARY KEY (revision_id, endpoint_kind, endpoint_id),
    CONSTRAINT fk_kcp_stream FOREIGN KEY (revision_id, endpoint_kind, endpoint_id) REFERENCES stream_settings(revision_id, endpoint_kind, endpoint_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE sniffing_settings (
    revision_id BIGINT NOT NULL,
    inbound_id BIGINT NOT NULL,
    enabled BOOLEAN NOT NULL,
    metadata_only BOOLEAN NOT NULL,
    route_only BOOLEAN NOT NULL,
    PRIMARY KEY (revision_id, inbound_id),
    CONSTRAINT fk_sniffing_inbound FOREIGN KEY (revision_id, inbound_id) REFERENCES inbounds(revision_id, inbound_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE sniffing_overrides (
    revision_id BIGINT NOT NULL,
    inbound_id BIGINT NOT NULL,
    position INT UNSIGNED NOT NULL,
    protocol VARCHAR(64) NOT NULL,
    PRIMARY KEY (revision_id, inbound_id, position),
    CONSTRAINT fk_sniffing_override FOREIGN KEY (revision_id, inbound_id) REFERENCES sniffing_settings(revision_id, inbound_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE inbound_limits (
    revision_id BIGINT NOT NULL,
    inbound_id BIGINT NOT NULL,
    max_connections BIGINT UNSIGNED NULL,
    max_handshake_seconds BIGINT UNSIGNED NULL,
    max_idle_seconds BIGINT UNSIGNED NULL,
    PRIMARY KEY (revision_id, inbound_id),
    CONSTRAINT fk_inbound_limits FOREIGN KEY (revision_id, inbound_id) REFERENCES inbounds(revision_id, inbound_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE routing_balancers (
    revision_id BIGINT NOT NULL,
    balancer_id BIGINT NOT NULL,
    tag VARCHAR(191) NOT NULL,
    strategy VARCHAR(32) NOT NULL,
    position INT UNSIGNED NOT NULL,
    failure_threshold INT UNSIGNED NULL,
    cooldown_seconds BIGINT UNSIGNED NULL,
    ewma_alpha DOUBLE NULL,
    switch_margin DOUBLE NULL,
    health_url VARCHAR(2048) NULL,
    health_interval_seconds BIGINT UNSIGNED NULL,
    health_timeout_seconds BIGINT UNSIGNED NULL,
    health_max_failures INT UNSIGNED NULL,
    PRIMARY KEY (revision_id, balancer_id),
    UNIQUE KEY uq_balancer_tag (revision_id, tag),
    CONSTRAINT fk_balancer_routing FOREIGN KEY (revision_id) REFERENCES routing_config(revision_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE routing_balancer_members (
    revision_id BIGINT NOT NULL,
    balancer_id BIGINT NOT NULL,
    position INT UNSIGNED NOT NULL,
    outbound_id BIGINT NOT NULL,
    profile_name VARCHAR(191) NULL,
    PRIMARY KEY (revision_id, balancer_id, position),
    CONSTRAINT fk_member_balancer FOREIGN KEY (revision_id, balancer_id) REFERENCES routing_balancers(revision_id, balancer_id) ON DELETE CASCADE,
    CONSTRAINT fk_member_outbound FOREIGN KEY (revision_id, outbound_id) REFERENCES outbounds(revision_id, outbound_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE panel_settings (
    singleton_id TINYINT UNSIGNED NOT NULL PRIMARY KEY,
    public_base_url VARCHAR(2048) NOT NULL,
    subscription_host VARCHAR(255) NOT NULL,
    firewall_auto_open BOOLEAN NOT NULL,
    enforcement_interval_seconds BIGINT UNSIGNED NOT NULL,
    adaptive_routing_enabled BOOLEAN NOT NULL,
    adaptive_tuning_mode VARCHAR(16) NOT NULL,
    adaptive_tuning_interval_seconds BIGINT UNSIGNED NOT NULL,
    adaptive_tuning_cooldown_seconds BIGINT UNSIGNED NOT NULL,
    adaptive_tuning_max_hysteria2_mbps BIGINT UNSIGNED NOT NULL,
    CONSTRAINT chk_panel_singleton CHECK (singleton_id = 1),
    CONSTRAINT chk_tuning_mode CHECK (adaptive_tuning_mode IN ('off', 'recommend', 'auto'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

INSERT INTO panel_settings
    (singleton_id, public_base_url, subscription_host, firewall_auto_open, enforcement_interval_seconds,
     adaptive_routing_enabled, adaptive_tuning_mode, adaptive_tuning_interval_seconds,
     adaptive_tuning_cooldown_seconds, adaptive_tuning_max_hysteria2_mbps)
VALUES (1, 'http://127.0.0.1:18080', '127.0.0.1', FALSE, 30, FALSE, 'off', 600, 600, 1000);

UPDATE blackwire_schema_version SET version = 2, updated_at = UTC_TIMESTAMP(6) WHERE singleton_id = 1;
