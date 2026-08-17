use blackwire_config::schema::{
    ApiConfig, BudgetConfig, DatagramConfig, FastConfig, FecConfig, FirstPacketBoostConfig,
    LimitsConfig, LogConfig, ProfileMode, QuicConfig, StatsConfig, TunConfig, VisionConfig,
};
use serde::{Deserialize, Serialize};

use crate::sqlx;
use crate::{ActivationClass, ActivationState, Database, MutationResult, StoreError, StoreResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreSettings {
    pub profile: ProfileMode,
    pub fast: Option<FastConfig>,
    pub budget: Option<BudgetConfig>,
    pub vision: Option<VisionConfig>,
    pub first_packet_boost: Option<FirstPacketBoostConfig>,
    pub log: LogConfig,
    pub metrics_addr: Option<String>,
    pub api: Option<ApiConfig>,
    pub stats: Option<StatsConfig>,
    pub limits: LimitsConfig,
    pub quic: Option<QuicConfig>,
    pub datagram: Option<DatagramConfig>,
    pub fec: Option<FecConfig>,
    pub tun: Option<TunConfig>,
}

impl Database {
    pub async fn core_settings(&self, revision: i64) -> StoreResult<CoreSettings> {
        let stored = self.load_config(revision).await?;
        let config = stored.config;
        Ok(CoreSettings {
            profile: config.profile,
            fast: config.fast,
            budget: config.budget,
            vision: config.vision,
            first_packet_boost: config.first_packet_boost,
            log: config.log,
            metrics_addr: config.metrics_addr,
            api: config.api,
            stats: config.stats,
            limits: config.limits,
            quic: config.quic,
            datagram: config.datagram,
            fec: config.fec,
            tun: config.tun,
        })
    }

    pub async fn save_core_settings(
        &self,
        actor: &str,
        expected_revision: i64,
        input: CoreSettings,
    ) -> StoreResult<MutationResult> {
        let state = self.state().await?;
        let class = ActivationClass::MaintenanceRequired;
        let (mut tx, revision) = self
            .fork_revision(
                expected_revision,
                actor,
                "Save Blackwire core settings",
                class,
            )
            .await?;

        let api_enabled = input.api.is_some();
        let api_listen = input.api.as_ref().map(|value| value.listen.trim());
        if api_enabled && api_listen == Some("") {
            return Err(StoreError::InvalidConfiguration(
                "API listen address must not be empty".into(),
            ));
        }
        sqlx::query("UPDATE global_config SET profile=?,metrics_enabled=?,metrics_address=?,api_enabled=?,api_listen_address=?,api_token_value=?,stats_enabled=?,log_level=?,log_structured=?,log_file=? WHERE revision_id=?")
            .bind(input.profile.to_string())
            .bind(input.metrics_addr.is_some()).bind(trimmed_option(input.metrics_addr.clone()))
            .bind(api_enabled).bind(api_listen).bind(input.api.as_ref().and_then(|value| value.token.as_deref()).map(str::as_bytes))
            .bind(input.stats.as_ref().map(|value| value.enabled))
            .bind(input.log.level.trim()).bind(input.log.json).bind(input.log.file.trim())
            .bind(revision).execute(&mut *tx).await?;

        sqlx::query("UPDATE global_limits SET max_connections=?,max_connections_per_inbound=?,max_connections_per_user=?,max_handshake_seconds=?,max_idle_seconds=? WHERE revision_id=?")
            .bind(input.limits.max_connections.map(|value| value as u64))
            .bind(input.limits.max_connections_per_inbound.map(|value| value as u64))
            .bind(input.limits.max_connections_per_user.map(|value| value as u64))
            .bind(input.limits.max_handshake_seconds).bind(input.limits.max_idle_seconds)
            .bind(revision).execute(&mut *tx).await?;

        let fast = input.fast.as_ref();
        let budget = input.budget.as_ref();
        let vision = input.vision.as_ref();
        let boost = input.first_packet_boost.as_ref();
        sqlx::query("UPDATE global_performance_settings SET fast_configured=?,fast_strict_production=?,fast_pool=?,fast_splice=?,fast_relay_engine=?,fast_relay_flush=?,fast_relay_initial_buffer=?,fast_relay_max_buffer=?,fast_linux_zerocopy=?,fast_linux_zerocopy_min_bytes=?,fast_linux_io_uring=?,fast_linux_af_xdp=?,budget_configured=?,budget_max_protocol_layers=?,budget_allow_sniffing=?,budget_allow_fake_ip=?,budget_max_route_rules=?,budget_max_handshake_ms=?,budget_prefer_direct_copy=?,budget_prefer_datagram_for_udp=?,vision_configured=?,vision_direct_copy=?,vision_max_packets_to_filter=?,vision_allow_splice_after_direct=?,first_packet_boost_configured=?,first_packet_boost_enabled=?,first_packet_boost_dns=?,first_packet_boost_tls_client_hello=?,first_packet_boost_send_early_payload=?,first_packet_boost_duplicate_control_on_badnet=?,first_packet_boost_priority=? WHERE revision_id=?")
            .bind(fast.is_some()).bind(fast.map(|v| v.strict_production).unwrap_or(true))
            .bind(fast.map(|v| scalar_text(&v.pool)).transpose()?.unwrap_or_else(|| "adaptive".into()))
            .bind(fast.map(|v| scalar_text(&v.splice)).transpose()?.unwrap_or_else(|| "adaptive".into()))
            .bind(fast.map(|v| scalar_text(&v.relay.engine)).transpose()?.unwrap_or_else(|| "v2".into()))
            .bind(fast.map(|v| scalar_text(&v.relay.flush)).transpose()?.unwrap_or_else(|| "adaptive".into()))
            .bind(fast.map(|v| v.relay.initial_buffer as u64).unwrap_or(16384)).bind(fast.map(|v| v.relay.max_buffer as u64).unwrap_or(262144))
            .bind(fast.map(|v| scalar_text(&v.linux.zerocopy)).transpose()?.unwrap_or_else(|| "disabled".into()))
            .bind(fast.map(|v| v.linux.zerocopy_min_bytes as u64).unwrap_or(16384))
            .bind(fast.map(|v| scalar_text(&v.linux.io_uring)).transpose()?.unwrap_or_else(|| "disabled".into()))
            .bind(fast.map(|v| scalar_text(&v.linux.af_xdp)).transpose()?.unwrap_or_else(|| "auto".into()))
            .bind(budget.is_some()).bind(budget.map(|v| v.max_protocol_layers as u64).unwrap_or(3))
            .bind(budget.map(|v| v.allow_sniffing).unwrap_or(false)).bind(budget.map(|v| v.allow_fake_ip).unwrap_or(false))
            .bind(budget.map(|v| v.max_route_rules as u64).unwrap_or(50)).bind(budget.map(|v| v.max_handshake_ms).unwrap_or(300))
            .bind(budget.map(|v| v.prefer_direct_copy).unwrap_or(true)).bind(budget.map(|v| v.prefer_datagram_for_udp).unwrap_or(true))
            .bind(vision.is_some()).bind(vision.map(|v| scalar_text(&v.direct_copy)).transpose()?.unwrap_or_else(|| "auto".into()))
            .bind(vision.map(|v| v.max_packets_to_filter).unwrap_or(8)).bind(vision.map(|v| v.allow_splice_after_direct).unwrap_or(true))
            .bind(boost.is_some()).bind(boost.map(|v| v.enabled).unwrap_or(false)).bind(boost.map(|v| v.dns).unwrap_or(true))
            .bind(boost.map(|v| v.tls_client_hello).unwrap_or(true)).bind(boost.map(|v| v.send_early_payload).unwrap_or(true))
            .bind(boost.map(|v| v.duplicate_control_on_badnet).unwrap_or(false))
            .bind(boost.map(|v| scalar_text(&v.priority)).transpose()?.unwrap_or_else(|| "high".into()))
            .bind(revision).execute(&mut *tx).await?;

        sqlx::query("DELETE FROM global_api_services WHERE revision_id=?")
            .bind(revision)
            .execute(&mut *tx)
            .await?;
        if let Some(api) = &input.api {
            for (position, service) in api
                .services
                .iter()
                .filter(|value| !value.trim().is_empty())
                .enumerate()
            {
                sqlx::query("INSERT INTO global_api_services (revision_id,position,service_name) VALUES (?,?,?)")
                    .bind(revision).bind(position as u32).bind(service.trim()).execute(&mut *tx).await?;
            }
        }

        let quic = input.quic.as_ref();
        let datagram = input.datagram.as_ref();
        let fec = input.fec.as_ref();
        sqlx::query("UPDATE global_transport_settings SET quic_configured=?,quic_reuse_port=?,quic_endpoints=?,quic_recv_buffer_bytes=?,quic_send_buffer_bytes=?,quic_max_datagram_size=?,datagram_configured=?,datagram_enabled=?,udp_over_datagram=?,tun_packets_over_datagram=?,datagram_policy=?,datagram_max_queue_delay_ms=?,fast_dns_retry=?,fast_dns_retry_delay_ms=?,fec_configured=?,fec_mode=?,fec_max_overhead_percent=?,fec_avoid_bulk_tcp=?,fec_disable_for_sequential_dns=?,fec_min_concurrency=?,fec_max_generation_packets=?,fec_max_generation_delay_ms=?,fec_recovery_deadline_ms=?,fec_dedup_window_packets=? WHERE revision_id=?")
            .bind(quic.is_some()).bind(quic.map(|v| v.reuse_port).unwrap_or(false))
            .bind(quic.map(|v| scalar_text(&v.endpoints)).transpose()?.unwrap_or_else(|| "1".into()))
            .bind(quic.map(|v| v.recv_buffer_bytes as u64).unwrap_or(8 * 1024 * 1024))
            .bind(quic.map(|v| v.send_buffer_bytes as u64).unwrap_or(8 * 1024 * 1024))
            .bind(quic.map(|v| scalar_text(&v.max_datagram_size)).transpose()?.unwrap_or_else(|| "auto".into()))
            .bind(datagram.is_some()).bind(datagram.map(|v| v.enabled).unwrap_or(true)).bind(datagram.map(|v| v.udp_over_datagram).unwrap_or(true)).bind(datagram.map(|v| v.tun_packets_over_datagram).unwrap_or(true))
            .bind(datagram.map(|v| scalar_text(&v.policy)).transpose()?.unwrap_or_else(|| "standard".into()))
            .bind(datagram.map(|v| v.max_queue_delay_ms).unwrap_or(25)).bind(datagram.map(|v| v.fast_dns_retry).unwrap_or(false)).bind(datagram.map(|v| v.fast_dns_retry_delay_ms).unwrap_or(20))
            .bind(fec.is_some()).bind(fec.map(|v| scalar_text(&v.mode)).transpose()?.unwrap_or_else(|| "off".into())).bind(fec.map(|v| v.max_overhead_percent).unwrap_or(20))
            .bind(fec.map(|v| v.avoid_bulk_tcp).unwrap_or(true)).bind(fec.map(|v| v.disable_for_sequential_dns).unwrap_or(true)).bind(fec.map(|v| v.min_concurrency_for_block_fec as u64).unwrap_or(4))
            .bind(fec.map(|v| v.max_generation_packets).unwrap_or(4)).bind(fec.map(|v| v.max_generation_delay_ms).unwrap_or(20)).bind(fec.map(|v| v.recovery_deadline_ms).unwrap_or(100)).bind(fec.map(|v| v.dedup_window_packets as u64).unwrap_or(1024))
            .bind(revision).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM global_fec_protect_classes WHERE revision_id=?")
            .bind(revision)
            .execute(&mut *tx)
            .await?;
        if let Some(fec) = fec {
            for (position, packet_class) in fec
                .protect_classes
                .iter()
                .filter(|value| !value.trim().is_empty())
                .enumerate()
            {
                sqlx::query("INSERT INTO global_fec_protect_classes (revision_id,position,packet_class) VALUES (?,?,?)")
                    .bind(revision).bind(position as u32).bind(packet_class.trim()).execute(&mut *tx).await?;
            }
        }

        sqlx::query("DELETE FROM tun_settings WHERE revision_id=?")
            .bind(revision)
            .execute(&mut *tx)
            .await?;
        if let Some(tun) = &input.tun {
            let linux = tun.linux.as_ref();
            sqlx::query("INSERT INTO tun_settings (revision_id,interface_name,address_value,netmask,mtu,bypass_mark,outbound_interface,redirect_port,dns_port,wintun_file,batch_enabled,batch_max_packets,batch_max_delay_us,batch_latency_flush_bytes,udp_max_sessions,udp_idle_timeout_sec,tcp_max_sessions,linux_configured,linux_backend,af_xdp_interface,af_xdp_queue_id,af_xdp_ring_entries,af_xdp_frame_count,af_xdp_frame_size,af_xdp_force_copy,af_xdp_force_zerocopy) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(revision).bind(tun.name.trim()).bind(tun.address.trim()).bind(tun.netmask.trim()).bind(u32::from(tun.mtu)).bind(u64::from(tun.bypass_mark))
                .bind(trimmed_option(tun.outbound_interface.clone())).bind(u32::from(tun.redirect_port)).bind(u32::from(tun.dns_port)).bind(trimmed_option(tun.wintun_file.clone()))
                .bind(tun.batch.enabled).bind(tun.batch.max_packets as u64).bind(tun.batch.max_delay_us).bind(tun.batch.latency_flush_bytes as u64)
                .bind(tun.sessions.udp_max as u64).bind(tun.sessions.udp_idle_timeout_sec).bind(tun.sessions.tcp_max as u64)
                .bind(linux.is_some()).bind(linux.map(|v| scalar_text(&v.backend)).transpose()?.unwrap_or_else(|| "tun".into()))
                .bind(linux.and_then(|v| v.af_xdp.interface.as_deref())).bind(linux.map(|v| u64::from(v.af_xdp.queue_id)).unwrap_or(0)).bind(linux.map(|v| u64::from(v.af_xdp.ring_entries)).unwrap_or(2048))
                .bind(linux.map(|v| u64::from(v.af_xdp.frame_count)).unwrap_or(4096)).bind(linux.map(|v| u64::from(v.af_xdp.frame_size)).unwrap_or(2048)).bind(linux.map(|v| v.af_xdp.force_copy).unwrap_or(true)).bind(linux.map(|v| v.af_xdp.force_zerocopy).unwrap_or(false))
                .execute(&mut *tx).await?;
        }

        Database::publish_revision(&mut tx, revision, class).await?;
        tx.commit().await?;
        Ok(MutationResult {
            revision,
            parent_revision: state.desired_revision,
            active_revision: state.active_revision,
            state: ActivationState::PendingMaintenance,
            activation_class: class,
            message: "Core settings revision saved; maintenance activation required".into(),
        })
    }
}

fn scalar_text<T: Serialize>(value: &T) -> StoreResult<String> {
    match serde_json::to_value(value)
        .map_err(|error| StoreError::InvalidConfiguration(error.to_string()))?
    {
        serde_json::Value::String(value) => Ok(value),
        serde_json::Value::Number(value) => Ok(value.to_string()),
        other => Err(StoreError::InvalidConfiguration(format!(
            "expected scalar setting, got {other}"
        ))),
    }
}

fn trimmed_option(value: Option<String>) -> Option<String> {
    value.and_then(|value| (!value.trim().is_empty()).then(|| value.trim().to_owned()))
}
