CREATE TABLE global_performance_settings (
    revision_id BIGINT NOT NULL PRIMARY KEY,
    fast_configured BOOLEAN NOT NULL DEFAULT FALSE,
    fast_strict_production BOOLEAN NOT NULL DEFAULT TRUE,
    fast_pool VARCHAR(16) NOT NULL DEFAULT 'adaptive',
    fast_splice VARCHAR(16) NOT NULL DEFAULT 'adaptive',
    fast_relay_engine VARCHAR(16) NOT NULL DEFAULT 'v2',
    fast_relay_flush VARCHAR(16) NOT NULL DEFAULT 'adaptive',
    fast_relay_initial_buffer BIGINT UNSIGNED NOT NULL DEFAULT 16384,
    fast_relay_max_buffer BIGINT UNSIGNED NOT NULL DEFAULT 262144,
    fast_linux_zerocopy VARCHAR(16) NOT NULL DEFAULT 'disabled',
    fast_linux_zerocopy_min_bytes BIGINT UNSIGNED NOT NULL DEFAULT 16384,
    fast_linux_io_uring VARCHAR(16) NOT NULL DEFAULT 'disabled',
    fast_linux_af_xdp VARCHAR(16) NOT NULL DEFAULT 'auto',
    budget_configured BOOLEAN NOT NULL DEFAULT FALSE,
    budget_max_protocol_layers BIGINT UNSIGNED NOT NULL DEFAULT 3,
    budget_allow_sniffing BOOLEAN NOT NULL DEFAULT FALSE,
    budget_allow_fake_ip BOOLEAN NOT NULL DEFAULT FALSE,
    budget_max_route_rules BIGINT UNSIGNED NOT NULL DEFAULT 50,
    budget_max_handshake_ms BIGINT UNSIGNED NOT NULL DEFAULT 300,
    budget_prefer_direct_copy BOOLEAN NOT NULL DEFAULT TRUE,
    budget_prefer_datagram_for_udp BOOLEAN NOT NULL DEFAULT TRUE,
    vision_configured BOOLEAN NOT NULL DEFAULT FALSE,
    vision_direct_copy VARCHAR(16) NOT NULL DEFAULT 'auto',
    vision_max_packets_to_filter TINYINT UNSIGNED NOT NULL DEFAULT 8,
    vision_allow_splice_after_direct BOOLEAN NOT NULL DEFAULT TRUE,
    first_packet_boost_configured BOOLEAN NOT NULL DEFAULT FALSE,
    first_packet_boost_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    first_packet_boost_dns BOOLEAN NOT NULL DEFAULT TRUE,
    first_packet_boost_tls_client_hello BOOLEAN NOT NULL DEFAULT TRUE,
    first_packet_boost_send_early_payload BOOLEAN NOT NULL DEFAULT TRUE,
    first_packet_boost_duplicate_control_on_badnet BOOLEAN NOT NULL DEFAULT FALSE,
    first_packet_boost_priority VARCHAR(16) NOT NULL DEFAULT 'high',
    CONSTRAINT fk_global_performance FOREIGN KEY (revision_id) REFERENCES global_config(revision_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

INSERT INTO global_performance_settings (revision_id)
SELECT revision_id FROM global_config;

UPDATE blackwire_schema_version SET version = 6, updated_at = UTC_TIMESTAMP(6) WHERE singleton_id = 1;
