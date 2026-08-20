DROP TABLE global_fec_protect_classes;

ALTER TABLE global_transport_settings
    DROP COLUMN datagram_configured,
    DROP COLUMN datagram_enabled,
    DROP COLUMN udp_over_datagram,
    DROP COLUMN datagram_policy,
    DROP COLUMN datagram_max_queue_delay_ms,
    DROP COLUMN fast_dns_retry,
    DROP COLUMN fast_dns_retry_delay_ms,
    DROP COLUMN fec_configured,
    DROP COLUMN fec_mode,
    DROP COLUMN fec_max_overhead_percent,
    DROP COLUMN fec_avoid_bulk_tcp,
    DROP COLUMN fec_disable_for_sequential_dns,
    DROP COLUMN fec_min_concurrency,
    DROP COLUMN fec_max_generation_packets,
    DROP COLUMN fec_max_generation_delay_ms,
    DROP COLUMN fec_recovery_deadline_ms,
    DROP COLUMN fec_dedup_window_packets;

ALTER TABLE endpoint_tuning
    DROP COLUMN datagram_enabled,
    DROP COLUMN udp_over_datagram,
    DROP COLUMN datagram_policy,
    DROP COLUMN fec_mode,
    DROP COLUMN fec_max_overhead_percent;

UPDATE blackwire_schema_version
SET version = 16, updated_at = UTC_TIMESTAMP(6)
WHERE singleton_id = 1;
