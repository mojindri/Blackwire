ALTER TABLE global_performance_settings
    DROP COLUMN budget_max_handshake_ms,
    DROP COLUMN budget_prefer_datagram_for_udp,
    DROP COLUMN first_packet_boost_tls_client_hello,
    DROP COLUMN first_packet_boost_duplicate_control_on_badnet,
    DROP COLUMN first_packet_boost_priority;

UPDATE blackwire_schema_version
SET version = 10, updated_at = UTC_TIMESTAMP(6)
WHERE singleton_id = 1;
