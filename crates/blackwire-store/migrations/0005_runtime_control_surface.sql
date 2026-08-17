ALTER TABLE global_config
    ADD COLUMN api_token_value VARBINARY(2048) NULL AFTER api_listen_address,
    ADD COLUMN stats_enabled BOOLEAN NULL AFTER api_token_value;

CREATE TABLE global_api_services (
    revision_id BIGINT NOT NULL,
    position INT UNSIGNED NOT NULL,
    service_name VARCHAR(191) NOT NULL,
    PRIMARY KEY (revision_id, position),
    CONSTRAINT fk_api_service_global FOREIGN KEY (revision_id) REFERENCES global_config(revision_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

INSERT INTO global_api_services (revision_id, position, service_name)
SELECT revision_id, 0, 'HandlerService' FROM global_config WHERE api_enabled=TRUE;

INSERT INTO global_api_services (revision_id, position, service_name)
SELECT revision_id, 1, 'StatsService' FROM global_config WHERE api_enabled=TRUE;

CREATE TABLE global_transport_settings (
    revision_id BIGINT NOT NULL PRIMARY KEY,
    quic_configured BOOLEAN NOT NULL DEFAULT FALSE,
    quic_reuse_port BOOLEAN NOT NULL DEFAULT FALSE,
    quic_endpoints VARCHAR(64) NOT NULL DEFAULT '1',
    quic_recv_buffer_bytes BIGINT UNSIGNED NOT NULL DEFAULT 8388608,
    quic_send_buffer_bytes BIGINT UNSIGNED NOT NULL DEFAULT 8388608,
    quic_max_datagram_size VARCHAR(64) NOT NULL DEFAULT 'auto',
    datagram_configured BOOLEAN NOT NULL DEFAULT FALSE,
    datagram_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    udp_over_datagram BOOLEAN NOT NULL DEFAULT TRUE,
    tun_packets_over_datagram BOOLEAN NOT NULL DEFAULT TRUE,
    datagram_policy VARCHAR(32) NOT NULL DEFAULT 'standard',
    datagram_max_queue_delay_ms BIGINT UNSIGNED NOT NULL DEFAULT 25,
    fast_dns_retry BOOLEAN NOT NULL DEFAULT FALSE,
    fast_dns_retry_delay_ms BIGINT UNSIGNED NOT NULL DEFAULT 20,
    fec_configured BOOLEAN NOT NULL DEFAULT FALSE,
    fec_mode VARCHAR(32) NOT NULL DEFAULT 'off',
    fec_max_overhead_percent TINYINT UNSIGNED NOT NULL DEFAULT 20,
    fec_avoid_bulk_tcp BOOLEAN NOT NULL DEFAULT TRUE,
    fec_disable_for_sequential_dns BOOLEAN NOT NULL DEFAULT TRUE,
    fec_min_concurrency BIGINT UNSIGNED NOT NULL DEFAULT 4,
    fec_max_generation_packets TINYINT UNSIGNED NOT NULL DEFAULT 4,
    fec_max_generation_delay_ms BIGINT UNSIGNED NOT NULL DEFAULT 20,
    fec_recovery_deadline_ms BIGINT UNSIGNED NOT NULL DEFAULT 100,
    fec_dedup_window_packets BIGINT UNSIGNED NOT NULL DEFAULT 1024,
    CONSTRAINT fk_global_transport FOREIGN KEY (revision_id) REFERENCES global_config(revision_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

INSERT INTO global_transport_settings (revision_id)
SELECT revision_id FROM global_config;

CREATE TABLE global_fec_protect_classes (
    revision_id BIGINT NOT NULL,
    position INT UNSIGNED NOT NULL,
    packet_class VARCHAR(191) NOT NULL,
    PRIMARY KEY (revision_id, position),
    CONSTRAINT fk_fec_class_transport FOREIGN KEY (revision_id) REFERENCES global_transport_settings(revision_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE tun_settings (
    revision_id BIGINT NOT NULL PRIMARY KEY,
    interface_name VARCHAR(191) NOT NULL,
    address_value VARCHAR(64) NOT NULL,
    netmask VARCHAR(64) NOT NULL,
    mtu INT UNSIGNED NOT NULL,
    bypass_mark BIGINT UNSIGNED NOT NULL,
    outbound_interface VARCHAR(191) NULL,
    redirect_port INT UNSIGNED NOT NULL,
    dns_port INT UNSIGNED NOT NULL,
    wintun_file VARCHAR(2048) NULL,
    batch_enabled BOOLEAN NOT NULL,
    batch_max_packets BIGINT UNSIGNED NOT NULL,
    batch_max_delay_us BIGINT UNSIGNED NOT NULL,
    batch_latency_flush_bytes BIGINT UNSIGNED NOT NULL,
    udp_max_sessions BIGINT UNSIGNED NOT NULL,
    udp_idle_timeout_sec BIGINT UNSIGNED NOT NULL,
    tcp_max_sessions BIGINT UNSIGNED NOT NULL,
    linux_configured BOOLEAN NOT NULL DEFAULT FALSE,
    linux_backend VARCHAR(16) NOT NULL DEFAULT 'tun',
    af_xdp_interface VARCHAR(191) NULL,
    af_xdp_queue_id BIGINT UNSIGNED NOT NULL DEFAULT 0,
    af_xdp_ring_entries BIGINT UNSIGNED NOT NULL DEFAULT 2048,
    af_xdp_frame_count BIGINT UNSIGNED NOT NULL DEFAULT 4096,
    af_xdp_frame_size BIGINT UNSIGNED NOT NULL DEFAULT 2048,
    af_xdp_force_copy BOOLEAN NOT NULL DEFAULT TRUE,
    af_xdp_force_zerocopy BOOLEAN NOT NULL DEFAULT FALSE,
    CONSTRAINT fk_tun_global FOREIGN KEY (revision_id) REFERENCES global_config(revision_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE shadowtls_settings (
    revision_id BIGINT NOT NULL,
    endpoint_kind VARCHAR(16) NOT NULL,
    endpoint_id BIGINT NOT NULL,
    password_value VARBINARY(2048) NOT NULL,
    destination VARCHAR(2048) NOT NULL,
    version TINYINT UNSIGNED NOT NULL,
    PRIMARY KEY (revision_id, endpoint_kind, endpoint_id),
    CONSTRAINT fk_shadowtls_stream FOREIGN KEY (revision_id, endpoint_kind, endpoint_id)
        REFERENCES stream_settings(revision_id, endpoint_kind, endpoint_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE splithttp_settings (
    revision_id BIGINT NOT NULL,
    endpoint_kind VARCHAR(16) NOT NULL,
    endpoint_id BIGINT NOT NULL,
    method_value VARCHAR(32) NOT NULL DEFAULT '',
    mode_value VARCHAR(32) NOT NULL DEFAULT '',
    uplink_http_method VARCHAR(32) NOT NULL DEFAULT '',
    padding_kind VARCHAR(16) NULL,
    padding_fixed BIGINT UNSIGNED NULL,
    padding_range VARCHAR(191) NULL,
    padding_min BIGINT UNSIGNED NULL,
    padding_max BIGINT UNSIGNED NULL,
    padding_from BIGINT UNSIGNED NULL,
    padding_to BIGINT UNSIGNED NULL,
    padding_method VARCHAR(64) NOT NULL DEFAULT '',
    padding_header VARCHAR(191) NOT NULL DEFAULT '',
    padding_key VARCHAR(191) NOT NULL DEFAULT '',
    padding_placement VARCHAR(64) NOT NULL DEFAULT '',
    session_placement VARCHAR(64) NOT NULL DEFAULT '',
    session_key VARCHAR(191) NOT NULL DEFAULT '',
    seq_placement VARCHAR(64) NOT NULL DEFAULT '',
    seq_key VARCHAR(191) NOT NULL DEFAULT '',
    uplink_data_placement VARCHAR(64) NOT NULL DEFAULT '',
    uplink_data_key VARCHAR(191) NOT NULL DEFAULT '',
    uplink_chunk_size BIGINT UNSIGNED NOT NULL DEFAULT 0,
    sc_max_buffered_posts BIGINT UNSIGNED NOT NULL DEFAULT 0,
    xmux_configured BOOLEAN NOT NULL DEFAULT FALSE,
    xmux_max_concurrency BIGINT UNSIGNED NULL,
    xmux_max_connections BIGINT UNSIGNED NULL,
    xmux_c_max_reuse_times BIGINT UNSIGNED NULL,
    xmux_h_max_request_times BIGINT UNSIGNED NULL,
    xmux_h_max_reusable_secs BIGINT UNSIGNED NULL,
    xmux_h_keep_alive_period BIGINT UNSIGNED NULL,
    download_configured BOOLEAN NOT NULL DEFAULT FALSE,
    download_network VARCHAR(32) NULL,
    download_security VARCHAR(32) NULL,
    PRIMARY KEY (revision_id, endpoint_kind, endpoint_id),
    CONSTRAINT fk_splithttp_stream FOREIGN KEY (revision_id, endpoint_kind, endpoint_id)
        REFERENCES stream_settings(revision_id, endpoint_kind, endpoint_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

INSERT INTO splithttp_settings (revision_id, endpoint_kind, endpoint_id)
SELECT revision_id, endpoint_kind, endpoint_id FROM websocket_settings WHERE transport_kind='splithttp';

CREATE TABLE splithttp_hosts (
    revision_id BIGINT NOT NULL,
    endpoint_kind VARCHAR(16) NOT NULL,
    endpoint_id BIGINT NOT NULL,
    position INT UNSIGNED NOT NULL,
    host_value VARCHAR(2048) NOT NULL,
    PRIMARY KEY (revision_id, endpoint_kind, endpoint_id, position),
    CONSTRAINT fk_splithttp_host FOREIGN KEY (revision_id, endpoint_kind, endpoint_id)
        REFERENCES splithttp_settings(revision_id, endpoint_kind, endpoint_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

UPDATE blackwire_schema_version SET version = 5, updated_at = UTC_TIMESTAMP(6) WHERE singleton_id = 1;
