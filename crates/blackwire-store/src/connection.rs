use std::{path::PathBuf, str::FromStr, time::Duration};

use crate::sqlx;
use sqlx::{mysql::MySqlPoolOptions, MySql, MySqlPool, Row, Transaction};

use crate::{
    ActivationClass, ActivationState, ConfigurationState, RevisionSummary, StoreError, StoreResult,
};

pub const EXPECTED_SCHEMA_VERSION: i64 = 9;

const MIGRATIONS: &[(i64, &str)] = &[
    (
        1,
        include_str!("../migrations/0001_mysql_control_plane.sql"),
    ),
    (
        2,
        include_str!("../migrations/0002_complete_relational_settings.sql"),
    ),
    (
        3,
        include_str!("../migrations/0003_runtime_state_and_retention.sql"),
    ),
    (4, include_str!("../migrations/0004_endpoint_tuning.sql")),
    (
        5,
        include_str!("../migrations/0005_runtime_control_surface.sql"),
    ),
    (
        6,
        include_str!("../migrations/0006_performance_control_surface.sql"),
    ),
    (
        7,
        include_str!("../migrations/0007_reality_fallback_limits.sql"),
    ),
    (
        8,
        include_str!("../migrations/0008_inbound_protocol_network.sql"),
    ),
    (
        9,
        include_str!("../migrations/0009_automatic_reload_and_remove_xdp.sql"),
    ),
];

#[derive(Debug, Clone)]
pub struct DatabaseOptions {
    pub url: String,
    pub max_connections: u32,
    pub acquire_timeout: Duration,
}

impl DatabaseOptions {
    pub fn from_env() -> StoreResult<Self> {
        let url = if let Ok(value) = std::env::var("BLACKWIRE_DATABASE_URL") {
            value
        } else {
            let explicit = std::env::var("BLACKWIRE_DATABASE_URL_FILE")
                .ok()
                .map(PathBuf::from);
            let systemd = std::env::var("CREDENTIALS_DIRECTORY")
                .ok()
                .map(|dir| PathBuf::from(dir).join("database-url"));
            let path = explicit.or(systemd).ok_or(StoreError::MissingDatabaseUrl)?;
            std::fs::read_to_string(&path).map_err(|source| StoreError::CredentialFile {
                path: path.display().to_string(),
                source,
            })?
        };
        let url = url.trim().to_string();
        if url.is_empty() {
            return Err(StoreError::MissingDatabaseUrl);
        }
        Ok(Self {
            url,
            max_connections: 16,
            acquire_timeout: Duration::from_secs(10),
        })
    }
}

#[derive(Clone)]
pub struct Database {
    pool: MySqlPool,
}

impl Database {
    pub async fn connect(options: DatabaseOptions) -> StoreResult<Self> {
        let connect_options = sqlx::mysql::MySqlConnectOptions::from_str(&options.url)?
            .timezone(Some("+00:00".into()))
            .charset("utf8mb4")
            .collation("utf8mb4_0900_ai_ci");
        let pool = MySqlPoolOptions::new()
            .max_connections(options.max_connections)
            .acquire_timeout(options.acquire_timeout)
            .connect_with(connect_options)
            .await?;
        Ok(Self { pool })
    }

    pub async fn connect_from_env() -> StoreResult<Self> {
        Self::connect(DatabaseOptions::from_env()?).await
    }

    pub fn pool(&self) -> &MySqlPool {
        &self.pool
    }

    pub async fn migrate(&self) -> StoreResult<()> {
        let mut conn = self.pool.acquire().await?;
        let locked: Option<i64> =
            sqlx::query_scalar("SELECT GET_LOCK('blackwire-schema-migrate', 30)")
                .fetch_one(&mut *conn)
                .await?;
        if locked != Some(1) {
            return Err(StoreError::Sql(sqlx::Error::Protocol(
                "timed out acquiring Blackwire migration lock".into(),
            )));
        }
        let current = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE((SELECT version FROM blackwire_schema_version WHERE singleton_id=1), 0)",
        )
        .fetch_one(&mut *conn)
        .await
        .or_else(|error| match &error {
            sqlx::Error::Database(db) if db.code().as_deref() == Some("42S02") => Ok(0),
            _ => Err(error),
        });
        let result = match current {
            Ok(current) => {
                let mut result = Ok(());
                for (version, script) in MIGRATIONS.iter().filter(|(version, _)| *version > current)
                {
                    if let Err(error) = execute_migration_script(&mut conn, script).await {
                        result = Err(error);
                        break;
                    }
                    let applied: i64 = sqlx::query_scalar(
                        "SELECT version FROM blackwire_schema_version WHERE singleton_id=1",
                    )
                    .fetch_one(&mut *conn)
                    .await?;
                    if applied != *version {
                        result = Err(StoreError::SchemaVersion {
                            expected: *version,
                            actual: applied,
                        });
                        break;
                    }
                }
                result
            }
            Err(error) => Err(StoreError::Sql(error)),
        };
        let _ = sqlx::query("SELECT RELEASE_LOCK('blackwire-schema-migrate')")
            .execute(&mut *conn)
            .await;
        result?;
        Ok(())
    }

    pub async fn verify_schema(&self) -> StoreResult<()> {
        let version: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM blackwire_schema_version WHERE singleton_id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| match error {
            sqlx::Error::Database(ref db) if db.code().as_deref() == Some("42S02") => {
                StoreError::SchemaMissing
            }
            other => StoreError::Sql(other),
        })?;
        let actual = version.ok_or(StoreError::SchemaMissing)?;
        if actual != EXPECTED_SCHEMA_VERSION {
            return Err(StoreError::SchemaVersion {
                expected: EXPECTED_SCHEMA_VERSION,
                actual,
            });
        }
        Ok(())
    }

    pub async fn state(&self) -> StoreResult<ConfigurationState> {
        let row = sqlx::query(
            "SELECT desired_revision, active_revision, activation_state, last_error, updated_at FROM configuration_state WHERE singleton_id = 1",
        )
        .fetch_one(&self.pool)
        .await?;
        let state: String = row.try_get("activation_state")?;
        Ok(ConfigurationState {
            desired_revision: row.try_get("desired_revision")?,
            active_revision: row.try_get("active_revision")?,
            activation_state: parse_activation_state(&state)?,
            last_error: row.try_get("last_error")?,
            updated_at: row.try_get("updated_at")?,
        })
    }

    pub async fn history(&self, limit: u32) -> StoreResult<Vec<RevisionSummary>> {
        let rows = sqlx::query(
            "SELECT revision, parent_revision, actor, summary, activation_class, created_at FROM configuration_revisions ORDER BY revision DESC LIMIT ?",
        )
        .bind(limit.min(100))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let class: String = row.try_get("activation_class")?;
                Ok(RevisionSummary {
                    revision: row.try_get("revision")?,
                    parent_revision: row.try_get("parent_revision")?,
                    actor: row.try_get("actor")?,
                    summary: row.try_get("summary")?,
                    activation_class: parse_activation_class(&class)?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect()
    }

    pub async fn begin_revision(
        &self,
        expected_revision: i64,
    ) -> StoreResult<Transaction<'_, MySql>> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT desired_revision FROM configuration_state WHERE singleton_id = 1 FOR UPDATE",
        )
        .fetch_one(&mut *tx)
        .await?;
        let actual: i64 = row.try_get("desired_revision")?;
        if actual != expected_revision {
            return Err(StoreError::RevisionConflict {
                expected: expected_revision,
                actual,
            });
        }
        Ok(tx)
    }

    /// Fork the desired immutable snapshot while holding the state-row lock.
    pub async fn fork_revision(
        &self,
        expected_revision: i64,
        actor: &str,
        summary: &str,
        class: ActivationClass,
    ) -> StoreResult<(Transaction<'_, MySql>, i64)> {
        const MAX_ATTEMPTS: u32 = 3;
        for attempt in 1..=MAX_ATTEMPTS {
            match self
                .fork_revision_once(expected_revision, actor, summary, class)
                .await
            {
                Ok(result) => return Ok(result),
                Err(error) if attempt < MAX_ATTEMPTS && is_retryable_transaction_error(&error) => {
                    tokio::time::sleep(Duration::from_millis(20 * u64::from(attempt))).await;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("bounded revision retry loop always returns")
    }

    async fn fork_revision_once(
        &self,
        expected_revision: i64,
        actor: &str,
        summary: &str,
        class: ActivationClass,
    ) -> StoreResult<(Transaction<'_, MySql>, i64)> {
        let mut tx = self.begin_revision(expected_revision).await?;
        let revision =
            Self::create_revision_metadata(&mut tx, expected_revision, actor, summary, class)
                .await?;
        copy_revision_rows(&mut tx, expected_revision, revision).await?;
        Ok((tx, revision))
    }

    pub async fn mark_active(&self, revision: i64) -> StoreResult<()> {
        sqlx::query(
            "UPDATE configuration_state SET active_revision = ?, activation_state = 'active', last_error = NULL, updated_at = UTC_TIMESTAMP(6) WHERE singleton_id = 1 AND desired_revision = ?",
        )
        .bind(revision)
        .bind(revision)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn record_activation_failure(&self, revision: i64, error: &str) -> StoreResult<()> {
        sqlx::query(
            "UPDATE configuration_state SET activation_state = 'failed', last_error = ?, updated_at = UTC_TIMESTAMP(6) WHERE singleton_id = 1 AND desired_revision = ?",
        )
        .bind(error)
        .bind(revision)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn rollback(
        &self,
        target_revision: i64,
        actor: &str,
    ) -> StoreResult<crate::MutationResult> {
        let state = self.state().await?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM configuration_revisions WHERE revision = ?)",
        )
        .bind(target_revision)
        .fetch_one(&self.pool)
        .await?;
        if !exists {
            return Err(StoreError::Sql(sqlx::Error::RowNotFound));
        }
        let mut tx = self.begin_revision(state.desired_revision).await?;
        let revision = Self::create_revision_metadata(
            &mut tx,
            state.desired_revision,
            actor,
            &format!("Rollback to revision {target_revision}"),
            ActivationClass::ListenerHandover,
        )
        .await?;
        copy_revision_rows(&mut tx, target_revision, revision).await?;
        Self::publish_revision(&mut tx, revision, ActivationClass::ListenerHandover).await?;
        tx.commit().await?;
        Ok(crate::MutationResult {
            revision,
            parent_revision: state.desired_revision,
            active_revision: state.active_revision,
            state: ActivationState::Activating,
            activation_class: ActivationClass::ListenerHandover,
            message: format!(
                "Rollback revision created from {target_revision}; applying automatically"
            ),
        })
    }

    pub async fn create_revision_metadata(
        tx: &mut Transaction<'_, MySql>,
        parent_revision: i64,
        actor: &str,
        summary: &str,
        class: ActivationClass,
    ) -> StoreResult<i64> {
        let class = match class {
            ActivationClass::HotSwap => "hot_swap",
            ActivationClass::ListenerHandover => "listener_handover",
        };
        let result = sqlx::query(
            "INSERT INTO configuration_revisions (parent_revision, actor, summary, activation_class, created_at) VALUES (?, ?, ?, ?, UTC_TIMESTAMP(6))",
        )
        .bind(parent_revision)
        .bind(actor)
        .bind(summary)
        .bind(class)
        .execute(&mut **tx)
        .await?;
        Ok(result.last_insert_id() as i64)
    }

    pub async fn publish_revision(
        tx: &mut Transaction<'_, MySql>,
        revision: i64,
        _class: ActivationClass,
    ) -> StoreResult<()> {
        sqlx::query(
            "UPDATE configuration_state SET desired_revision = ?, activation_state = 'activating', last_error = NULL, updated_at = UTC_TIMESTAMP(6) WHERE singleton_id = 1",
        )
        .bind(revision)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}

fn is_retryable_transaction_error(error: &StoreError) -> bool {
    matches!(
        error,
        StoreError::Sql(sqlx::Error::Database(database))
            if matches!(database.code().as_deref(), Some("1205" | "1213"))
    )
}

async fn execute_migration_script(
    conn: &mut sqlx::pool::PoolConnection<MySql>,
    script: &str,
) -> StoreResult<()> {
    for statement in script
        .split(';')
        .map(str::trim)
        .filter(|statement| !statement.is_empty())
    {
        sqlx::query(statement).execute(&mut **conn).await?;
    }
    Ok(())
}

fn parse_activation_state(value: &str) -> StoreResult<ActivationState> {
    match value {
        "active" => Ok(ActivationState::Active),
        "activating" => Ok(ActivationState::Activating),
        "failed" => Ok(ActivationState::Failed),
        other => Err(StoreError::Sql(sqlx::Error::Decode(
            format!("invalid activation state '{other}'").into(),
        ))),
    }
}

async fn copy_revision_rows(
    tx: &mut Transaction<'_, MySql>,
    source: i64,
    target: i64,
) -> StoreResult<()> {
    // Parent tables must be copied before their foreign-key children.
    let statements = [
        "INSERT INTO global_config SELECT ?, profile, metrics_enabled, metrics_address, api_enabled, api_listen_address, api_token_value, stats_enabled, log_level, log_structured, log_file FROM global_config WHERE revision_id=?",
        "INSERT INTO global_limits SELECT ?, max_connections, max_connections_per_inbound, max_connections_per_user, max_handshake_seconds, max_idle_seconds FROM global_limits WHERE revision_id=?",
        "INSERT INTO global_api_services SELECT ?, position, service_name FROM global_api_services WHERE revision_id=?",
        "INSERT INTO global_transport_settings SELECT ?, quic_configured, quic_reuse_port, quic_endpoints, quic_recv_buffer_bytes, quic_send_buffer_bytes, quic_max_datagram_size, datagram_configured, datagram_enabled, udp_over_datagram, tun_packets_over_datagram, datagram_policy, datagram_max_queue_delay_ms, fast_dns_retry, fast_dns_retry_delay_ms, fec_configured, fec_mode, fec_max_overhead_percent, fec_avoid_bulk_tcp, fec_disable_for_sequential_dns, fec_min_concurrency, fec_max_generation_packets, fec_max_generation_delay_ms, fec_recovery_deadline_ms, fec_dedup_window_packets FROM global_transport_settings WHERE revision_id=?",
        "INSERT INTO global_performance_settings SELECT ?, fast_configured, fast_strict_production, fast_pool, fast_splice, fast_relay_engine, fast_relay_flush, fast_relay_initial_buffer, fast_relay_max_buffer, fast_linux_zerocopy, fast_linux_zerocopy_min_bytes, fast_linux_io_uring, budget_configured, budget_max_protocol_layers, budget_allow_sniffing, budget_allow_fake_ip, budget_max_route_rules, budget_max_handshake_ms, budget_prefer_direct_copy, budget_prefer_datagram_for_udp, vision_configured, vision_direct_copy, vision_max_packets_to_filter, vision_allow_splice_after_direct, first_packet_boost_configured, first_packet_boost_enabled, first_packet_boost_dns, first_packet_boost_tls_client_hello, first_packet_boost_send_early_payload, first_packet_boost_duplicate_control_on_badnet, first_packet_boost_priority FROM global_performance_settings WHERE revision_id=?",
        "INSERT INTO global_fec_protect_classes SELECT ?, position, packet_class FROM global_fec_protect_classes WHERE revision_id=?",
        "INSERT INTO tun_settings SELECT ?, interface_name, address_value, netmask, mtu, bypass_mark, outbound_interface, redirect_port, dns_port, wintun_file, batch_enabled, batch_max_packets, batch_max_delay_us, batch_latency_flush_bytes, udp_max_sessions, udp_idle_timeout_sec, tcp_max_sessions FROM tun_settings WHERE revision_id=?",
        "INSERT INTO inbounds SELECT ?, inbound_id, tag, listen_address, listen_port, protocol, enabled, position FROM inbounds WHERE revision_id=?",
        "INSERT INTO outbounds SELECT ?, outbound_id, tag, protocol, enabled, position, server_address, server_port, domain_strategy, deny_loopback, reject_ipv6_literal FROM outbounds WHERE revision_id=?",
        "INSERT INTO stream_settings SELECT ?, endpoint_kind, endpoint_id, network, security FROM stream_settings WHERE revision_id=?",
        "INSERT INTO tls_settings SELECT ?, endpoint_kind, endpoint_id, server_name, allow_insecure, certificate_file, key_file FROM tls_settings WHERE revision_id=?",
        "INSERT INTO tls_alpn SELECT ?, endpoint_kind, endpoint_id, position, protocol FROM tls_alpn WHERE revision_id=?",
        "INSERT INTO reality_settings SELECT ?, endpoint_kind, endpoint_id, show_details, destination, private_key, public_key, short_id, fingerprint, server_name, max_time_diff_seconds, fallback_upload_after_bytes, fallback_upload_bytes_per_sec, fallback_upload_burst_bytes_per_sec, fallback_download_after_bytes, fallback_download_bytes_per_sec, fallback_download_burst_bytes_per_sec FROM reality_settings WHERE revision_id=?",
        "INSERT INTO reality_server_names SELECT ?, endpoint_kind, endpoint_id, position, server_name FROM reality_server_names WHERE revision_id=?",
        "INSERT INTO reality_short_ids SELECT ?, endpoint_kind, endpoint_id, position, short_id FROM reality_short_ids WHERE revision_id=?",
        "INSERT INTO shadowtls_settings SELECT ?, endpoint_kind, endpoint_id, password_value, destination, version FROM shadowtls_settings WHERE revision_id=?",
        "INSERT INTO inbound_protocol_settings SELECT ?, inbound_id, decryption, method, auth_value, up_mbps, down_mbps, endpoint_shards, network, auth_timeout_ms FROM inbound_protocol_settings WHERE revision_id=?",
        "INSERT INTO outbound_protocol_settings SELECT ?, outbound_id, password_value, auth_value, method, uuid_value, flow, server_name, skip_certificate_verify, endpoint_shards FROM outbound_protocol_settings WHERE revision_id=?",
        "INSERT INTO websocket_settings SELECT ?, endpoint_kind, endpoint_id, transport_kind, request_path FROM websocket_settings WHERE revision_id=?",
        "INSERT INTO transport_headers SELECT ?, endpoint_kind, endpoint_id, transport_kind, header_name, header_value FROM transport_headers WHERE revision_id=?",
        "INSERT INTO splithttp_settings SELECT ?, endpoint_kind, endpoint_id, method_value, mode_value, uplink_http_method, padding_kind, padding_fixed, padding_range, padding_min, padding_max, padding_from, padding_to, padding_method, padding_header, padding_key, padding_placement, session_placement, session_key, seq_placement, seq_key, uplink_data_placement, uplink_data_key, uplink_chunk_size, sc_max_buffered_posts, xmux_configured, xmux_max_concurrency, xmux_max_connections, xmux_c_max_reuse_times, xmux_h_max_request_times, xmux_h_max_reusable_secs, xmux_h_keep_alive_period, download_configured, download_network, download_security FROM splithttp_settings WHERE revision_id=?",
        "INSERT INTO splithttp_hosts SELECT ?, endpoint_kind, endpoint_id, position, host_value FROM splithttp_hosts WHERE revision_id=?",
        "INSERT INTO grpc_settings SELECT ?, endpoint_kind, endpoint_id, service_name, multi_mode FROM grpc_settings WHERE revision_id=?",
        "INSERT INTO kcp_settings SELECT ?, endpoint_kind, endpoint_id, header_type, mtu, tti_ms, uplink_capacity, downlink_capacity, congestion, read_buffer_size, write_buffer_size FROM kcp_settings WHERE revision_id=?",
        "INSERT INTO endpoint_tuning SELECT ?, endpoint_kind, endpoint_id, congestion_mode, min_ack_rate, max_queue_delay_ms, pacing_gain, loss_compensation, quic_reuse_port, quic_endpoints, quic_recv_buffer_bytes, quic_send_buffer_bytes, datagram_enabled, udp_over_datagram, datagram_policy, fec_mode, fec_max_overhead_percent FROM endpoint_tuning WHERE revision_id=?",
        "INSERT INTO sniffing_settings SELECT ?, inbound_id, enabled, metadata_only, route_only FROM sniffing_settings WHERE revision_id=?",
        "INSERT INTO sniffing_overrides SELECT ?, inbound_id, position, protocol FROM sniffing_overrides WHERE revision_id=?",
        "INSERT INTO inbound_limits SELECT ?, inbound_id, max_connections, max_handshake_seconds, max_idle_seconds FROM inbound_limits WHERE revision_id=?",
        "INSERT INTO users SELECT ?, user_id, inbound_id, email, enabled, flow, note, traffic_limit_bytes, expiry_at, subscription_token FROM users WHERE revision_id=?",
        "INSERT INTO user_credentials SELECT ?, user_id, credential_kind, uuid_value, password_value, method, auth_value FROM user_credentials WHERE revision_id=?",
        "INSERT INTO dns_config SELECT ?, enabled, fake_ip_enabled, fake_ip_pool FROM dns_config WHERE revision_id=?",
        "INSERT INTO dns_servers SELECT ?, position, address FROM dns_servers WHERE revision_id=?",
        "INSERT INTO routing_config SELECT ?, enabled, domain_strategy, geoip_file, geosite_file FROM routing_config WHERE revision_id=?",
        "INSERT INTO routing_rules SELECT ?, rule_id, position, rule_type, port_expression, outbound_id FROM routing_rules WHERE revision_id=?",
        "INSERT INTO routing_rule_values SELECT ?, rule_id, value_kind, position, value_text FROM routing_rule_values WHERE revision_id=?",
        "INSERT INTO routing_balancers SELECT ?, balancer_id, tag, strategy, position, failure_threshold, cooldown_seconds, ewma_alpha, switch_margin, health_url, health_interval_seconds, health_timeout_seconds, health_max_failures FROM routing_balancers WHERE revision_id=?",
        "INSERT INTO routing_balancer_members SELECT ?, balancer_id, position, outbound_id, profile_name FROM routing_balancer_members WHERE revision_id=?",
    ];
    for statement in statements {
        sqlx::query(statement)
            .bind(target)
            .bind(source)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

fn parse_activation_class(value: &str) -> StoreResult<ActivationClass> {
    match value {
        "hot_swap" => Ok(ActivationClass::HotSwap),
        "listener_handover" => Ok(ActivationClass::ListenerHandover),
        other => Err(StoreError::Sql(sqlx::Error::Decode(
            format!("invalid activation class '{other}'").into(),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{EXPECTED_SCHEMA_VERSION, MIGRATIONS};

    #[test]
    fn embedded_migrations_advance_the_schema_version() {
        assert_eq!(
            MIGRATIONS.last().map(|(version, _)| *version),
            Some(EXPECTED_SCHEMA_VERSION)
        );
        for (version, script) in MIGRATIONS {
            let update_marker = format!("version = {version}");
            let initial_insert_marker = format!("VALUES (1, {version},");
            assert!(
                script.contains(&update_marker) || script.contains(&initial_insert_marker),
                "migration {version} does not advance blackwire_schema_version"
            );
        }
    }
}
