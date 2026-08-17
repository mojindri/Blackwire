ALTER TABLE websocket_settings DROP CHECK chk_websocket_kind;
ALTER TABLE websocket_settings
    ADD CONSTRAINT chk_websocket_kind CHECK (transport_kind IN ('ws', 'httpupgrade', 'splithttp'));

CREATE TABLE endpoint_tuning (
    revision_id BIGINT NOT NULL,
    endpoint_kind VARCHAR(16) NOT NULL,
    endpoint_id BIGINT NOT NULL,
    congestion_mode VARCHAR(64) NULL,
    min_ack_rate DOUBLE NULL,
    max_queue_delay_ms BIGINT UNSIGNED NULL,
    pacing_gain DOUBLE NULL,
    loss_compensation BOOLEAN NULL,
    quic_reuse_port BOOLEAN NULL,
    quic_endpoints VARCHAR(32) NULL,
    quic_recv_buffer_bytes BIGINT UNSIGNED NULL,
    quic_send_buffer_bytes BIGINT UNSIGNED NULL,
    datagram_enabled BOOLEAN NULL,
    udp_over_datagram BOOLEAN NULL,
    datagram_policy VARCHAR(64) NULL,
    fec_mode VARCHAR(64) NULL,
    fec_max_overhead_percent TINYINT UNSIGNED NULL,
    PRIMARY KEY (revision_id, endpoint_kind, endpoint_id),
    CONSTRAINT fk_endpoint_tuning_stream FOREIGN KEY (revision_id, endpoint_kind, endpoint_id)
        REFERENCES stream_settings(revision_id, endpoint_kind, endpoint_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

UPDATE blackwire_schema_version SET version = 4, updated_at = UTC_TIMESTAMP(6) WHERE singleton_id = 1;
