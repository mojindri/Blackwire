ALTER TABLE global_transport_settings
    DROP COLUMN tun_packets_over_datagram;

ALTER TABLE global_performance_settings
    DROP COLUMN budget_allow_fake_ip;

UPDATE blackwire_schema_version
SET version = 13, updated_at = UTC_TIMESTAMP(6)
WHERE singleton_id = 1;
