-- The removed specialist profiles never selected a distinct relay path; they
-- behaved like compatibility mode plus advisory explain-cost thresholds.
UPDATE global_config
SET profile = 'compat'
WHERE profile NOT IN ('compat', 'fast');

ALTER TABLE global_performance_settings
    DROP COLUMN budget_configured,
    DROP COLUMN budget_max_protocol_layers,
    DROP COLUMN budget_allow_sniffing,
    DROP COLUMN budget_max_route_rules,
    DROP COLUMN budget_prefer_direct_copy;

UPDATE blackwire_schema_version
SET version = 15, updated_at = UTC_TIMESTAMP(6)
WHERE singleton_id = 1;
