UPDATE configuration_revisions
SET activation_class = 'listener_handover'
WHERE activation_class = 'maintenance_required';

UPDATE configuration_state
SET activation_state = 'activating'
WHERE activation_state = 'pending_maintenance';

ALTER TABLE configuration_state
    DROP FOREIGN KEY fk_pending_revision,
    DROP CHECK chk_activation_state,
    DROP COLUMN pending_maintenance_revision,
    ADD CONSTRAINT chk_activation_state CHECK (activation_state IN ('active', 'activating', 'failed'));

ALTER TABLE configuration_revisions
    DROP CHECK chk_activation_class,
    ADD CONSTRAINT chk_activation_class CHECK (activation_class IN ('hot_swap', 'listener_handover'));

ALTER TABLE global_performance_settings
    DROP COLUMN fast_linux_af_xdp;

ALTER TABLE tun_settings
    DROP COLUMN linux_configured,
    DROP COLUMN linux_backend,
    DROP COLUMN af_xdp_interface,
    DROP COLUMN af_xdp_queue_id,
    DROP COLUMN af_xdp_ring_entries,
    DROP COLUMN af_xdp_frame_count,
    DROP COLUMN af_xdp_frame_size,
    DROP COLUMN af_xdp_force_copy,
    DROP COLUMN af_xdp_force_zerocopy;

UPDATE blackwire_schema_version
SET version = 9, updated_at = UTC_TIMESTAMP(6)
WHERE singleton_id = 1;
