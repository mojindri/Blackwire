use std::net::IpAddr;

use crate::sqlx;
use blackwire_config::schema::{
    AdaptiveBalancerConfig, ApiConfig, BalancerConfig, BalancerProfileConfig, BudgetConfig, Config,
    CongestionSettings, DatagramConfig, DatagramOverrides, DatagramSize, DnsConfig,
    DownloadSettings, EndpointCount, EndpointSettings, EndpointUser, FakeIpConfig, FastConfig,
    FastLinuxConfig, FastRelayConfig, FecConfig, FecOverrides, FirstPacketBoostConfig, GrpcConfig,
    HealthCheckConfig, InboundConfig, InboundLimitsConfig, KcpConfig, LimitsConfig, LogConfig,
    NetworkType, OutboundConfig, PaddingBounds, PaddingBytes, ProfileMode, Protocol, QuicConfig,
    QuicSocketOverrides, RealityConfig, RealityFallbackLimitConfig, RoutingConfig, RoutingRule,
    SecurityType, ShadowTlsConfig, SniffingConfig, SplitHttpConfig, StatsConfig,
    StreamSettingsConfig, TlsConfig, TunAfXdpConfig, TunBatchConfig, TunConfig, TunLinuxConfig,
    TunSessionConfig, VisionConfig, WsConfig, XmuxConfig,
};
use sqlx::{MySqlPool, Row};

use crate::{Database, StoreError, StoreResult};

/// A typed configuration snapshot reconstructed solely from relational rows.
#[derive(Debug, Clone)]
pub struct StoredConfig {
    pub revision: i64,
    pub config: Config,
}

impl Database {
    pub async fn load_desired_config(&self) -> StoreResult<StoredConfig> {
        let state = self.state().await?;
        self.load_config(state.desired_revision).await
    }

    pub async fn load_config(&self, revision: i64) -> StoreResult<StoredConfig> {
        let global = sqlx::query(
            "SELECT profile, metrics_enabled, metrics_address, api_enabled, api_listen_address, api_token_value, stats_enabled, log_level, log_structured, log_file FROM global_config WHERE revision_id = ?",
        )
        .bind(revision)
        .fetch_one(self.pool())
        .await?;
        let profile: String = global.try_get("profile")?;
        let api_enabled: bool = global.try_get("api_enabled")?;
        let api_address: Option<String> = global.try_get("api_listen_address")?;
        let metrics_enabled: bool = global.try_get("metrics_enabled")?;
        let performance = load_performance(self.pool(), revision).await?;
        StoredConfig {
            revision,
            config: Config {
                profile: parse_profile(&profile)?,
                fast: performance.fast,
                budget: performance.budget,
                vision: performance.vision,
                first_packet_boost: performance.first_packet_boost,
                quic: load_quic(self.pool(), revision).await?,
                datagram: load_datagram(self.pool(), revision).await?,
                fec: load_fec(self.pool(), revision).await?,
                log: LogConfig {
                    level: global.try_get("log_level")?,
                    json: global.try_get("log_structured")?,
                    file: global.try_get("log_file")?,
                },
                dns: load_dns(self.pool(), revision).await?,
                routing: load_routing(self.pool(), revision).await?,
                tun: load_tun(self.pool(), revision).await?,
                limits: load_limits(self.pool(), revision).await?,
                inbounds: load_inbounds(self.pool(), revision).await?,
                outbounds: load_outbounds(self.pool(), revision).await?,
                stats: global
                    .try_get::<Option<bool>, _>("stats_enabled")?
                    .map(|enabled| StatsConfig { enabled }),
                api: api_enabled.then(|| ApiConfig {
                    listen: api_address.unwrap_or_else(|| "127.0.0.1:62789".into()),
                    token: bytes_string(&global, "api_token_value").ok().flatten(),
                    services: Vec::new(),
                }),
                metrics_addr: if metrics_enabled {
                    global.try_get("metrics_address")?
                } else {
                    None
                },
            },
        }
        .with_api_services(self.pool())
        .await
    }
}

struct PerformanceSettings {
    fast: Option<FastConfig>,
    budget: Option<BudgetConfig>,
    vision: Option<VisionConfig>,
    first_packet_boost: Option<FirstPacketBoostConfig>,
}

async fn load_performance(pool: &MySqlPool, revision: i64) -> StoreResult<PerformanceSettings> {
    let row = sqlx::query("SELECT * FROM global_performance_settings WHERE revision_id=?")
        .bind(revision)
        .fetch_one(pool)
        .await?;
    let fast = row
        .try_get::<bool, _>("fast_configured")?
        .then(|| {
            Ok::<_, StoreError>(FastConfig {
                strict_production: row.try_get("fast_strict_production")?,
                pool: parse_string_enum(&row.try_get::<String, _>("fast_pool")?)?,
                splice: parse_string_enum(&row.try_get::<String, _>("fast_splice")?)?,
                relay: FastRelayConfig {
                    engine: parse_string_enum(&row.try_get::<String, _>("fast_relay_engine")?)?,
                    flush: parse_string_enum(&row.try_get::<String, _>("fast_relay_flush")?)?,
                    initial_buffer: usize::try_from(
                        row.try_get::<u64, _>("fast_relay_initial_buffer")?,
                    )
                    .map_err(decode_error)?,
                    max_buffer: usize::try_from(row.try_get::<u64, _>("fast_relay_max_buffer")?)
                        .map_err(decode_error)?,
                },
                linux: FastLinuxConfig {
                    zerocopy: parse_string_enum(&row.try_get::<String, _>("fast_linux_zerocopy")?)?,
                    zerocopy_min_bytes: usize::try_from(
                        row.try_get::<u64, _>("fast_linux_zerocopy_min_bytes")?,
                    )
                    .map_err(decode_error)?,
                    io_uring: parse_string_enum(&row.try_get::<String, _>("fast_linux_io_uring")?)?,
                    af_xdp: parse_string_enum(&row.try_get::<String, _>("fast_linux_af_xdp")?)?,
                },
            })
        })
        .transpose()?;
    let budget = row
        .try_get::<bool, _>("budget_configured")?
        .then(|| {
            Ok::<_, StoreError>(BudgetConfig {
                max_protocol_layers: usize::try_from(
                    row.try_get::<u64, _>("budget_max_protocol_layers")?,
                )
                .map_err(decode_error)?,
                allow_sniffing: row.try_get("budget_allow_sniffing")?,
                allow_fake_ip: row.try_get("budget_allow_fake_ip")?,
                max_route_rules: usize::try_from(row.try_get::<u64, _>("budget_max_route_rules")?)
                    .map_err(decode_error)?,
                max_handshake_ms: row.try_get("budget_max_handshake_ms")?,
                prefer_direct_copy: row.try_get("budget_prefer_direct_copy")?,
                prefer_datagram_for_udp: row.try_get("budget_prefer_datagram_for_udp")?,
            })
        })
        .transpose()?;
    let vision = row
        .try_get::<bool, _>("vision_configured")?
        .then(|| {
            Ok::<_, StoreError>(VisionConfig {
                direct_copy: parse_string_enum(&row.try_get::<String, _>("vision_direct_copy")?)?,
                max_packets_to_filter: row.try_get("vision_max_packets_to_filter")?,
                allow_splice_after_direct: row.try_get("vision_allow_splice_after_direct")?,
            })
        })
        .transpose()?;
    let first_packet_boost = row
        .try_get::<bool, _>("first_packet_boost_configured")?
        .then(|| {
            Ok::<_, StoreError>(FirstPacketBoostConfig {
                enabled: row.try_get("first_packet_boost_enabled")?,
                dns: row.try_get("first_packet_boost_dns")?,
                tls_client_hello: row.try_get("first_packet_boost_tls_client_hello")?,
                send_early_payload: row.try_get("first_packet_boost_send_early_payload")?,
                duplicate_control_on_badnet: row
                    .try_get("first_packet_boost_duplicate_control_on_badnet")?,
                priority: parse_string_enum(
                    &row.try_get::<String, _>("first_packet_boost_priority")?,
                )?,
            })
        })
        .transpose()?;
    Ok(PerformanceSettings {
        fast,
        budget,
        vision,
        first_packet_boost,
    })
}

impl StoredConfig {
    async fn with_api_services(mut self, pool: &MySqlPool) -> StoreResult<Self> {
        if let Some(api) = self.config.api.as_mut() {
            api.services = sqlx::query_scalar("SELECT service_name FROM global_api_services WHERE revision_id=? ORDER BY position")
                .bind(self.revision).fetch_all(pool).await?;
        }
        Ok(self)
    }
}

async fn global_transport_row(
    pool: &MySqlPool,
    revision: i64,
) -> StoreResult<sqlx::mysql::MySqlRow> {
    sqlx::query("SELECT * FROM global_transport_settings WHERE revision_id=?")
        .bind(revision)
        .fetch_one(pool)
        .await
        .map_err(StoreError::Sql)
}

async fn load_quic(pool: &MySqlPool, revision: i64) -> StoreResult<Option<QuicConfig>> {
    let row = global_transport_row(pool, revision).await?;
    if !row.try_get::<bool, _>("quic_configured")? {
        return Ok(None);
    }
    Ok(Some(QuicConfig {
        reuse_port: row.try_get("quic_reuse_port")?,
        endpoints: parse_number_or_string(&row.try_get::<String, _>("quic_endpoints")?)?,
        recv_buffer_bytes: usize::try_from(row.try_get::<u64, _>("quic_recv_buffer_bytes")?)
            .map_err(decode_error)?,
        send_buffer_bytes: usize::try_from(row.try_get::<u64, _>("quic_send_buffer_bytes")?)
            .map_err(decode_error)?,
        max_datagram_size: parse_number_or_string::<DatagramSize>(
            &row.try_get::<String, _>("quic_max_datagram_size")?,
        )?,
    }))
}

async fn load_datagram(pool: &MySqlPool, revision: i64) -> StoreResult<Option<DatagramConfig>> {
    let row = global_transport_row(pool, revision).await?;
    if !row.try_get::<bool, _>("datagram_configured")? {
        return Ok(None);
    }
    Ok(Some(DatagramConfig {
        enabled: row.try_get("datagram_enabled")?,
        udp_over_datagram: row.try_get("udp_over_datagram")?,
        tun_packets_over_datagram: row.try_get("tun_packets_over_datagram")?,
        policy: parse_string_enum(&row.try_get::<String, _>("datagram_policy")?)?,
        max_queue_delay_ms: row.try_get("datagram_max_queue_delay_ms")?,
        fast_dns_retry: row.try_get("fast_dns_retry")?,
        fast_dns_retry_delay_ms: row.try_get("fast_dns_retry_delay_ms")?,
    }))
}

async fn load_fec(pool: &MySqlPool, revision: i64) -> StoreResult<Option<FecConfig>> {
    let row = global_transport_row(pool, revision).await?;
    if !row.try_get::<bool, _>("fec_configured")? {
        return Ok(None);
    }
    let protect_classes = sqlx::query_scalar(
        "SELECT packet_class FROM global_fec_protect_classes WHERE revision_id=? ORDER BY position",
    )
    .bind(revision)
    .fetch_all(pool)
    .await?;
    Ok(Some(FecConfig {
        mode: parse_string_enum(&row.try_get::<String, _>("fec_mode")?)?,
        max_overhead_percent: row.try_get("fec_max_overhead_percent")?,
        protect_classes,
        avoid_bulk_tcp: row.try_get("fec_avoid_bulk_tcp")?,
        disable_for_sequential_dns: row.try_get("fec_disable_for_sequential_dns")?,
        min_concurrency_for_block_fec: usize::try_from(
            row.try_get::<u64, _>("fec_min_concurrency")?,
        )
        .map_err(decode_error)?,
        max_generation_packets: row.try_get("fec_max_generation_packets")?,
        max_generation_delay_ms: row.try_get("fec_max_generation_delay_ms")?,
        recovery_deadline_ms: row.try_get("fec_recovery_deadline_ms")?,
        dedup_window_packets: usize::try_from(row.try_get::<u64, _>("fec_dedup_window_packets")?)
            .map_err(decode_error)?,
    }))
}

async fn load_tun(pool: &MySqlPool, revision: i64) -> StoreResult<Option<TunConfig>> {
    let Some(row) = sqlx::query("SELECT * FROM tun_settings WHERE revision_id=?")
        .bind(revision)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };
    let linux = row
        .try_get::<bool, _>("linux_configured")?
        .then(|| TunLinuxConfig {
            backend: parse_string_enum(
                &row.try_get::<String, _>("linux_backend")
                    .unwrap_or_else(|_| "tun".into()),
            )
            .unwrap_or_default(),
            af_xdp: TunAfXdpConfig {
                interface: row.try_get("af_xdp_interface").ok().flatten(),
                queue_id: row.try_get::<u64, _>("af_xdp_queue_id").unwrap_or(0) as u32,
                ring_entries: row.try_get::<u64, _>("af_xdp_ring_entries").unwrap_or(2048) as u32,
                frame_count: row.try_get::<u64, _>("af_xdp_frame_count").unwrap_or(4096) as u32,
                frame_size: row.try_get::<u64, _>("af_xdp_frame_size").unwrap_or(2048) as u32,
                force_copy: row.try_get("af_xdp_force_copy").unwrap_or(true),
                force_zerocopy: row.try_get("af_xdp_force_zerocopy").unwrap_or(false),
            },
        });
    Ok(Some(TunConfig {
        name: row.try_get("interface_name")?,
        address: row.try_get("address_value")?,
        netmask: row.try_get("netmask")?,
        mtu: u16::try_from(row.try_get::<u32, _>("mtu")?).map_err(decode_error)?,
        bypass_mark: u32::try_from(row.try_get::<u64, _>("bypass_mark")?).map_err(decode_error)?,
        outbound_interface: row.try_get("outbound_interface")?,
        redirect_port: u16::try_from(row.try_get::<u32, _>("redirect_port")?)
            .map_err(decode_error)?,
        dns_port: u16::try_from(row.try_get::<u32, _>("dns_port")?).map_err(decode_error)?,
        wintun_file: row.try_get("wintun_file")?,
        batch: TunBatchConfig {
            enabled: row.try_get("batch_enabled")?,
            max_packets: usize::try_from(row.try_get::<u64, _>("batch_max_packets")?)
                .map_err(decode_error)?,
            max_delay_us: row.try_get("batch_max_delay_us")?,
            latency_flush_bytes: usize::try_from(
                row.try_get::<u64, _>("batch_latency_flush_bytes")?,
            )
            .map_err(decode_error)?,
        },
        sessions: TunSessionConfig {
            udp_max: usize::try_from(row.try_get::<u64, _>("udp_max_sessions")?)
                .map_err(decode_error)?,
            udp_idle_timeout_sec: row.try_get("udp_idle_timeout_sec")?,
            tcp_max: usize::try_from(row.try_get::<u64, _>("tcp_max_sessions")?)
                .map_err(decode_error)?,
        },
        linux,
    }))
}

async fn load_limits(pool: &MySqlPool, revision: i64) -> StoreResult<LimitsConfig> {
    let row = sqlx::query("SELECT max_connections, max_connections_per_inbound, max_connections_per_user, max_handshake_seconds, max_idle_seconds FROM global_limits WHERE revision_id = ?")
        .bind(revision).fetch_one(pool).await?;
    Ok(LimitsConfig {
        max_connections: optional_usize(&row, "max_connections")?,
        max_connections_per_inbound: optional_usize(&row, "max_connections_per_inbound")?,
        max_connections_per_user: optional_usize(&row, "max_connections_per_user")?,
        max_handshake_seconds: row.try_get("max_handshake_seconds")?,
        max_idle_seconds: row.try_get("max_idle_seconds")?,
    })
}

async fn load_inbounds(pool: &MySqlPool, revision: i64) -> StoreResult<Vec<InboundConfig>> {
    let rows = sqlx::query("SELECT inbound_id, tag, listen_address, listen_port, protocol FROM inbounds WHERE revision_id = ? AND enabled = TRUE ORDER BY position")
        .bind(revision).fetch_all(pool).await?;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let inbound_id: i64 = row.try_get("inbound_id")?;
        let protocol = parse_protocol(&row.try_get::<String, _>("protocol")?)?;
        let clients = load_clients(pool, revision, inbound_id, &protocol).await?;
        let mut settings = EndpointSettings::default();
        if let Some(protocol_row) = sqlx::query("SELECT decryption, method, auth_value, up_mbps, down_mbps, endpoint_shards FROM inbound_protocol_settings WHERE revision_id=? AND inbound_id=?")
            .bind(revision).bind(inbound_id).fetch_optional(pool).await? {
            settings.decryption = protocol_row.try_get("decryption")?;
            settings.method = protocol_row.try_get("method")?;
            settings.auth = bytes_string(&protocol_row, "auth_value")?;
            settings.up_mbps = protocol_row.try_get("up_mbps")?;
            settings.down_mbps = protocol_row.try_get("down_mbps")?;
            settings.endpoint_shards = protocol_row.try_get::<Option<u32>, _>("endpoint_shards")?.map(usize::try_from).transpose().map_err(decode_error)?;
        }
        load_endpoint_tuning(pool, revision, "inbound", inbound_id, &mut settings).await?;
        match protocol {
            Protocol::Tuic => settings.users = clients,
            Protocol::Hysteria2 => {
                if settings.auth.is_none() {
                    settings.auth = clients
                        .first()
                        .and_then(|user| user.auth.clone().or_else(|| user.password.clone()));
                }
                settings.clients = clients;
            }
            Protocol::Shadowsocks => {
                settings.password = clients.first().and_then(|user| user.password.clone());
                settings.clients = clients;
            }
            Protocol::Vless | Protocol::Vmess | Protocol::Trojan => settings.clients = clients,
            _ => {}
        }
        result.push(InboundConfig {
            tag: row.try_get("tag")?,
            protocol,
            listen: row
                .try_get::<String, _>("listen_address")?
                .parse::<IpAddr>()
                .map_err(decode_error)?,
            port: u16::try_from(row.try_get::<u32, _>("listen_port")?).map_err(decode_error)?,
            settings,
            stream_settings: load_stream(pool, revision, "inbound", inbound_id).await?,
            limits: load_inbound_limits(pool, revision, inbound_id).await?,
            sniffing: load_sniffing(pool, revision, inbound_id).await?,
        });
    }
    Ok(result)
}

async fn load_outbounds(pool: &MySqlPool, revision: i64) -> StoreResult<Vec<OutboundConfig>> {
    let rows = sqlx::query("SELECT outbound_id, tag, protocol, server_address, server_port, domain_strategy, deny_loopback, reject_ipv6_literal FROM outbounds WHERE revision_id = ? AND enabled = TRUE ORDER BY position")
        .bind(revision).fetch_all(pool).await?;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let outbound_id: i64 = row.try_get("outbound_id")?;
        let port = row
            .try_get::<Option<u32>, _>("server_port")?
            .map(u16::try_from)
            .transpose()
            .map_err(decode_error)?;
        let mut settings = EndpointSettings {
            address: row.try_get("server_address")?,
            port,
            domain_strategy: row.try_get("domain_strategy")?,
            deny_loopback: row
                .try_get::<Option<bool>, _>("deny_loopback")?
                .unwrap_or(false),
            reject_ipv6_literal: row
                .try_get::<Option<bool>, _>("reject_ipv6_literal")?
                .unwrap_or(false),
            ..Default::default()
        };
        if let Some(protocol) = sqlx::query("SELECT password_value, auth_value, method, uuid_value, flow, server_name, skip_certificate_verify, endpoint_shards FROM outbound_protocol_settings WHERE revision_id=? AND outbound_id=?")
            .bind(revision).bind(outbound_id).fetch_optional(pool).await? {
            settings.password = bytes_string(&protocol, "password_value")?;
            settings.auth = bytes_string(&protocol, "auth_value")?;
            settings.method = protocol.try_get("method")?;
            settings.uuid = protocol.try_get("uuid_value")?;
            settings.flow = protocol.try_get::<Option<String>, _>("flow")?.unwrap_or_default();
            settings.server_name = protocol.try_get("server_name")?;
            settings.skip_cert_verify = protocol.try_get::<Option<bool>, _>("skip_certificate_verify")?.unwrap_or(false);
            settings.endpoint_shards = protocol.try_get::<Option<u32>, _>("endpoint_shards")?.map(usize::try_from).transpose().map_err(decode_error)?;
        }
        load_endpoint_tuning(pool, revision, "outbound", outbound_id, &mut settings).await?;
        if matches!(
            parse_protocol(&row.try_get::<String, _>("protocol")?)?,
            Protocol::Vless | Protocol::Vmess
        ) && settings.uuid.is_some()
        {
            settings.users.push(EndpointUser {
                id: settings.uuid.clone(),
                flow: settings.flow.clone(),
                ..Default::default()
            });
        }
        result.push(OutboundConfig {
            tag: row.try_get("tag")?,
            protocol: parse_protocol(&row.try_get::<String, _>("protocol")?)?,
            settings,
            stream_settings: load_stream(pool, revision, "outbound", outbound_id).await?,
        });
    }
    Ok(result)
}

async fn load_clients(
    pool: &MySqlPool,
    revision: i64,
    inbound_id: i64,
    protocol: &Protocol,
) -> StoreResult<Vec<EndpointUser>> {
    let rows = sqlx::query("SELECT u.email, u.flow, c.uuid_value, c.password_value, c.method, c.auth_value FROM users u JOIN user_credentials c ON c.revision_id = u.revision_id AND c.user_id = u.user_id WHERE u.revision_id = ? AND u.inbound_id = ? AND u.enabled = TRUE ORDER BY u.user_id")
        .bind(revision).bind(inbound_id).fetch_all(pool).await?;
    rows.into_iter()
        .map(|row| {
            let email: String = row.try_get("email")?;
            let uuid: Option<String> = row.try_get("uuid_value")?;
            let password = bytes_string(&row, "password_value")?;
            let auth = bytes_string(&row, "auth_value")?;
            let method: Option<String> = row.try_get("method")?;
            Ok(match protocol {
                Protocol::Vless | Protocol::Vmess => EndpointUser {
                    id: uuid,
                    email: Some(email),
                    flow: row.try_get("flow")?,
                    ..Default::default()
                },
                Protocol::Trojan => EndpointUser {
                    password,
                    email: Some(email),
                    ..Default::default()
                },
                Protocol::Shadowsocks => EndpointUser {
                    password,
                    email: Some(email),
                    name: method,
                    ..Default::default()
                },
                Protocol::Hysteria2 => EndpointUser {
                    auth: auth.or(password),
                    email: Some(email),
                    ..Default::default()
                },
                Protocol::Tuic => EndpointUser {
                    id: uuid,
                    password,
                    email: Some(email),
                    ..Default::default()
                },
                _ => EndpointUser {
                    email: Some(email),
                    ..Default::default()
                },
            })
        })
        .collect()
}

async fn load_stream(
    pool: &MySqlPool,
    revision: i64,
    kind: &str,
    id: i64,
) -> StoreResult<Option<StreamSettingsConfig>> {
    let Some(row) = sqlx::query("SELECT network, security FROM stream_settings WHERE revision_id = ? AND endpoint_kind = ? AND endpoint_id = ?")
        .bind(revision).bind(kind).bind(id).fetch_optional(pool).await? else { return Ok(None); };
    let mut stream = StreamSettingsConfig {
        network: parse_network(&row.try_get::<String, _>("network")?)?,
        security: parse_security(&row.try_get::<String, _>("security")?)?,
        ..Default::default()
    };
    if let Some(tls) = sqlx::query("SELECT server_name, allow_insecure, certificate_file, key_file FROM tls_settings WHERE revision_id = ? AND endpoint_kind = ? AND endpoint_id = ?")
        .bind(revision).bind(kind).bind(id).fetch_optional(pool).await? {
        let alpn = sqlx::query_scalar("SELECT protocol FROM tls_alpn WHERE revision_id = ? AND endpoint_kind = ? AND endpoint_id = ? ORDER BY position")
            .bind(revision).bind(kind).bind(id).fetch_all(pool).await?;
        stream.tls_settings = Some(TlsConfig { server_name: tls.try_get("server_name")?, allow_insecure: tls.try_get("allow_insecure")?, alpn, certificate_file: tls.try_get("certificate_file")?, key_file: tls.try_get("key_file")? });
    }
    if let Some(reality) = sqlx::query("SELECT show_details, destination, private_key, public_key, short_id, fingerprint, server_name, max_time_diff_seconds, fallback_upload_after_bytes, fallback_upload_bytes_per_sec, fallback_upload_burst_bytes_per_sec, fallback_download_after_bytes, fallback_download_bytes_per_sec, fallback_download_burst_bytes_per_sec FROM reality_settings WHERE revision_id = ? AND endpoint_kind = ? AND endpoint_id = ?")
        .bind(revision).bind(kind).bind(id).fetch_optional(pool).await? {
        let names = sqlx::query_scalar("SELECT server_name FROM reality_server_names WHERE revision_id = ? AND endpoint_kind = ? AND endpoint_id = ? ORDER BY position")
            .bind(revision).bind(kind).bind(id).fetch_all(pool).await?;
        let short_ids = sqlx::query_scalar("SELECT short_id FROM reality_short_ids WHERE revision_id = ? AND endpoint_kind = ? AND endpoint_id = ? ORDER BY position")
            .bind(revision).bind(kind).bind(id).fetch_all(pool).await?;
        let upload_after: Option<u64> = reality.try_get("fallback_upload_after_bytes")?;
        let upload_rate: Option<u64> = reality.try_get("fallback_upload_bytes_per_sec")?;
        let upload_burst: Option<u64> = reality.try_get("fallback_upload_burst_bytes_per_sec")?;
        let download_after: Option<u64> = reality.try_get("fallback_download_after_bytes")?;
        let download_rate: Option<u64> = reality.try_get("fallback_download_bytes_per_sec")?;
        let download_burst: Option<u64> = reality.try_get("fallback_download_burst_bytes_per_sec")?;
        stream.reality_settings = Some(RealityConfig {
            show: reality.try_get("show_details")?,
            dest: reality.try_get("destination")?,
            private_key: reality.try_get("private_key")?,
            short_ids,
            public_key: reality.try_get("public_key")?,
            short_id: reality.try_get("short_id")?,
            fingerprint: reality.try_get("fingerprint")?,
            server_name: reality.try_get("server_name")?,
            server_names: names,
            max_time_diff: 0,
            max_time_diff_seconds: reality.try_get("max_time_diff_seconds")?,
            limit_fallback_upload: upload_rate.map(|bytes_per_sec| RealityFallbackLimitConfig {
                after_bytes: upload_after.unwrap_or_default(),
                bytes_per_sec,
                burst_bytes_per_sec: upload_burst.unwrap_or_default(),
            }),
            limit_fallback_download: download_rate.map(|bytes_per_sec| RealityFallbackLimitConfig {
                after_bytes: download_after.unwrap_or_default(),
                bytes_per_sec,
                burst_bytes_per_sec: download_burst.unwrap_or_default(),
            }),
        });
    }
    if let Some(shadow) = sqlx::query("SELECT password_value,destination,version FROM shadowtls_settings WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=?")
        .bind(revision).bind(kind).bind(id).fetch_optional(pool).await? {
        stream.shadow_tls_settings = Some(ShadowTlsConfig {
            password: bytes_string(&shadow, "password_value")?.unwrap_or_default(),
            dest: shadow.try_get("destination")?, version: shadow.try_get("version")?,
        });
    }
    for transport_kind in ["ws", "httpupgrade"] {
        if let Some(transport) = sqlx::query("SELECT request_path FROM websocket_settings WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=? AND transport_kind=?")
            .bind(revision).bind(kind).bind(id).bind(transport_kind).fetch_optional(pool).await? {
            let header_rows = sqlx::query("SELECT header_name, header_value FROM transport_headers WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=? AND transport_kind=?")
                .bind(revision).bind(kind).bind(id).bind(transport_kind).fetch_all(pool).await?;
            let mut headers = std::collections::HashMap::new();
            for header in header_rows {
                headers.insert(header.try_get("header_name")?, header.try_get("header_value")?);
            }
            let config = WsConfig { path: transport.try_get("request_path")?, headers };
            if transport_kind == "ws" { stream.ws_settings = Some(config); } else { stream.httpupgrade_settings = Some(config); }
        }
    }
    if let Some(transport) = sqlx::query("SELECT w.request_path,s.* FROM websocket_settings w JOIN splithttp_settings s ON s.revision_id=w.revision_id AND s.endpoint_kind=w.endpoint_kind AND s.endpoint_id=w.endpoint_id WHERE w.revision_id=? AND w.endpoint_kind=? AND w.endpoint_id=? AND w.transport_kind='splithttp'")
        .bind(revision).bind(kind).bind(id).fetch_optional(pool).await? {
        let host = sqlx::query_scalar("SELECT host_value FROM splithttp_hosts WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=? ORDER BY position").bind(revision).bind(kind).bind(id).fetch_all(pool).await?;
        let header_rows = sqlx::query("SELECT header_name,header_value FROM transport_headers WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=? AND transport_kind='splithttp'").bind(revision).bind(kind).bind(id).fetch_all(pool).await?;
        let mut headers = std::collections::HashMap::new();
        for header in header_rows { headers.insert(header.try_get("header_name")?, header.try_get("header_value")?); }
        let x_padding_bytes = match transport.try_get::<Option<String>, _>("padding_kind")?.as_deref() {
            Some("fixed") => transport.try_get::<Option<u64>, _>("padding_fixed")?.map(|v| PaddingBytes::Fixed(v as usize)),
            Some("range") => transport.try_get::<Option<String>, _>("padding_range")?.map(PaddingBytes::Range),
            Some("bounds") => Some(PaddingBytes::Bounds(PaddingBounds { min: optional_usize(&transport, "padding_min")?, max: optional_usize(&transport, "padding_max")?, from: optional_usize(&transport, "padding_from")?, to: optional_usize(&transport, "padding_to")? })),
            _ => None,
        };
        let xmux = transport.try_get::<bool, _>("xmux_configured")?.then(|| XmuxConfig {
            max_concurrency: optional_usize(&transport, "xmux_max_concurrency").ok().flatten(), max_connections: optional_usize(&transport, "xmux_max_connections").ok().flatten(), c_max_reuse_times: optional_usize(&transport, "xmux_c_max_reuse_times").ok().flatten(), h_max_request_times: optional_usize(&transport, "xmux_h_max_request_times").ok().flatten(), h_max_reusable_secs: transport.try_get("xmux_h_max_reusable_secs").ok().flatten(), h_keep_alive_period: transport.try_get("xmux_h_keep_alive_period").ok().flatten(),
        });
        let download_settings = transport.try_get::<bool, _>("download_configured")?.then(|| -> StoreResult<DownloadSettings> { Ok(DownloadSettings {
            network: transport.try_get::<Option<String>, _>("download_network")?.map(|v| parse_network(&v)).transpose()?,
            security: transport.try_get::<Option<String>, _>("download_security")?.map(|v| parse_security(&v)).transpose()?,
        }) }).transpose()?;
        stream.splithttp_settings = Some(SplitHttpConfig {
            path: transport.try_get("request_path")?, host, method: transport.try_get("method_value")?, mode: transport.try_get("mode_value")?, uplink_http_method: transport.try_get("uplink_http_method")?, headers, x_padding_bytes,
            x_padding_method: transport.try_get("padding_method")?, x_padding_header: transport.try_get("padding_header")?, x_padding_key: transport.try_get("padding_key")?, x_padding_placement: transport.try_get("padding_placement")?,
            session_placement: transport.try_get("session_placement")?, session_key: transport.try_get("session_key")?, seq_placement: transport.try_get("seq_placement")?, seq_key: transport.try_get("seq_key")?, uplink_data_placement: transport.try_get("uplink_data_placement")?, uplink_data_key: transport.try_get("uplink_data_key")?,
            uplink_chunk_size: u32::try_from(transport.try_get::<u64, _>("uplink_chunk_size")?).map_err(decode_error)?, sc_max_buffered_posts: usize::try_from(transport.try_get::<u64, _>("sc_max_buffered_posts")?).map_err(decode_error)?, xmux, download_settings,
        });
    }
    if let Some(grpc) = sqlx::query("SELECT service_name, multi_mode FROM grpc_settings WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=?")
        .bind(revision).bind(kind).bind(id).fetch_optional(pool).await? {
        stream.grpc_settings = Some(GrpcConfig { service_name: grpc.try_get("service_name")?, multi_mode: grpc.try_get("multi_mode")? });
    }
    if let Some(kcp) = sqlx::query("SELECT header_type, mtu, tti_ms, uplink_capacity, downlink_capacity, congestion, read_buffer_size, write_buffer_size FROM kcp_settings WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=?")
        .bind(revision).bind(kind).bind(id).fetch_optional(pool).await? {
        stream.kcp_settings = Some(KcpConfig {
            header: kcp.try_get("header_type")?,
            mtu: u16::try_from(kcp.try_get::<u32, _>("mtu")?).map_err(decode_error)?,
            tti: kcp.try_get("tti_ms")?,
            uplink_capacity: kcp.try_get("uplink_capacity")?,
            downlink_capacity: kcp.try_get("downlink_capacity")?,
            congestion: kcp.try_get("congestion")?,
            read_buffer_size: kcp.try_get("read_buffer_size")?,
            write_buffer_size: kcp.try_get("write_buffer_size")?,
        });
    }
    Ok(Some(stream))
}

async fn load_endpoint_tuning(
    pool: &MySqlPool,
    revision: i64,
    kind: &str,
    id: i64,
    settings: &mut EndpointSettings,
) -> StoreResult<()> {
    let Some(row) = sqlx::query("SELECT congestion_mode,min_ack_rate,max_queue_delay_ms,pacing_gain,loss_compensation,quic_reuse_port,quic_endpoints,quic_recv_buffer_bytes,quic_send_buffer_bytes,datagram_enabled,udp_over_datagram,datagram_policy,fec_mode,fec_max_overhead_percent FROM endpoint_tuning WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=?")
        .bind(revision).bind(kind).bind(id).fetch_optional(pool).await? else { return Ok(()); };

    let congestion_mode: Option<String> = row.try_get("congestion_mode")?;
    let min_ack_rate: Option<f64> = row.try_get("min_ack_rate")?;
    let max_queue_delay_ms: Option<u64> = row.try_get("max_queue_delay_ms")?;
    let pacing_gain: Option<f64> = row.try_get("pacing_gain")?;
    let loss_compensation: Option<bool> = row.try_get("loss_compensation")?;
    if congestion_mode.is_some()
        || min_ack_rate.is_some()
        || max_queue_delay_ms.is_some()
        || pacing_gain.is_some()
        || loss_compensation.is_some()
    {
        settings.congestion = Some(CongestionSettings {
            mode: congestion_mode.unwrap_or_else(|| "standard".into()),
            min_ack_rate,
            max_queue_delay_ms,
            pacing_gain,
            loss_compensation,
        });
    }

    let endpoints: Option<String> = row.try_get("quic_endpoints")?;
    let reuse_port: Option<bool> = row.try_get("quic_reuse_port")?;
    let recv_buffer_bytes = optional_usize(&row, "quic_recv_buffer_bytes")?;
    let send_buffer_bytes = optional_usize(&row, "quic_send_buffer_bytes")?;
    if endpoints.is_some()
        || reuse_port.is_some()
        || recv_buffer_bytes.is_some()
        || send_buffer_bytes.is_some()
    {
        settings.quic = Some(QuicSocketOverrides {
            reuse_port,
            endpoints: endpoints.map(|value| {
                value
                    .parse::<usize>()
                    .map(EndpointCount::Fixed)
                    .unwrap_or(EndpointCount::Named(value))
            }),
            recv_buffer_bytes,
            send_buffer_bytes,
        });
    }

    let datagram_enabled: Option<bool> = row.try_get("datagram_enabled")?;
    let udp_over_datagram: Option<bool> = row.try_get("udp_over_datagram")?;
    let datagram_policy: Option<String> = row.try_get("datagram_policy")?;
    if datagram_enabled.is_some() || udp_over_datagram.is_some() || datagram_policy.is_some() {
        settings.datagram = Some(DatagramOverrides {
            enabled: datagram_enabled,
            udp_over_datagram,
            policy: datagram_policy,
            ..Default::default()
        });
    }

    let fec_mode: Option<String> = row.try_get("fec_mode")?;
    let max_overhead_percent: Option<u8> = row.try_get("fec_max_overhead_percent")?;
    if fec_mode.is_some() || max_overhead_percent.is_some() {
        settings.fec = Some(FecOverrides {
            mode: fec_mode,
            max_overhead_percent,
            ..Default::default()
        });
    }
    Ok(())
}

async fn load_inbound_limits(
    pool: &MySqlPool,
    revision: i64,
    inbound_id: i64,
) -> StoreResult<Option<InboundLimitsConfig>> {
    let row = sqlx::query("SELECT max_connections, max_handshake_seconds, max_idle_seconds FROM inbound_limits WHERE revision_id=? AND inbound_id=?")
        .bind(revision).bind(inbound_id).fetch_optional(pool).await?;
    row.map(|row| {
        Ok(InboundLimitsConfig {
            max_connections: optional_usize(&row, "max_connections")?,
            max_handshake_seconds: row.try_get("max_handshake_seconds")?,
            max_idle_seconds: row.try_get("max_idle_seconds")?,
        })
    })
    .transpose()
}

async fn load_sniffing(
    pool: &MySqlPool,
    revision: i64,
    inbound_id: i64,
) -> StoreResult<Option<SniffingConfig>> {
    let Some(row) = sqlx::query("SELECT enabled, metadata_only, route_only FROM sniffing_settings WHERE revision_id=? AND inbound_id=?")
        .bind(revision).bind(inbound_id).fetch_optional(pool).await? else { return Ok(None); };
    let dest_override = sqlx::query_scalar("SELECT protocol FROM sniffing_overrides WHERE revision_id=? AND inbound_id=? ORDER BY position")
        .bind(revision).bind(inbound_id).fetch_all(pool).await?;
    Ok(Some(SniffingConfig {
        enabled: row.try_get("enabled")?,
        dest_override,
        metadata_only: row.try_get("metadata_only")?,
        route_only: row.try_get("route_only")?,
    }))
}

async fn load_dns(pool: &MySqlPool, revision: i64) -> StoreResult<Option<DnsConfig>> {
    let Some(row) = sqlx::query(
        "SELECT enabled, fake_ip_enabled, fake_ip_pool FROM dns_config WHERE revision_id = ?",
    )
    .bind(revision)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    if !row.try_get::<bool, _>("enabled")? {
        return Ok(None);
    }
    let servers = sqlx::query_scalar(
        "SELECT address FROM dns_servers WHERE revision_id = ? ORDER BY position",
    )
    .bind(revision)
    .fetch_all(pool)
    .await?;
    let fake_ip = row
        .try_get::<bool, _>("fake_ip_enabled")?
        .then(|| FakeIpConfig {
            enabled: true,
            pool: row
                .try_get::<Option<String>, _>("fake_ip_pool")
                .ok()
                .flatten()
                .unwrap_or_else(|| "198.18.0.0/15".into()),
        });
    Ok(Some(DnsConfig { servers, fake_ip }))
}

async fn load_routing(pool: &MySqlPool, revision: i64) -> StoreResult<Option<RoutingConfig>> {
    let Some(config) = sqlx::query("SELECT enabled, domain_strategy, geoip_file, geosite_file FROM routing_config WHERE revision_id = ?").bind(revision).fetch_optional(pool).await? else { return Ok(None); };
    if !config.try_get::<bool, _>("enabled")? {
        return Ok(None);
    }
    let rows = sqlx::query("SELECT rule_id, rule_type, port_expression, outbound_id FROM routing_rules WHERE revision_id = ? ORDER BY position").bind(revision).fetch_all(pool).await?;
    let mut rules = Vec::with_capacity(rows.len());
    for row in rows {
        let rule_id: i64 = row.try_get("rule_id")?;
        let values = sqlx::query("SELECT value_kind, value_text FROM routing_rule_values WHERE revision_id = ? AND rule_id = ? ORDER BY value_kind, position").bind(revision).bind(rule_id).fetch_all(pool).await?;
        let mut rule = RoutingRule {
            rule_type: row.try_get("rule_type")?,
            port: row.try_get("port_expression")?,
            ..Default::default()
        };
        for value in values {
            let text: String = value.try_get("value_text")?;
            match value.try_get::<String, _>("value_kind")?.as_str() {
                "domain" => rule.domain.push(text),
                "ip" => rule.ip.push(text),
                "inbound_tag" => rule.inbound_tag.push(text),
                "protocol" => rule.protocol.push(text),
                "user" => rule.user.push(text),
                _ => {}
            }
        }
        rule.outbound_tag = sqlx::query_scalar(
            "SELECT tag FROM outbounds WHERE revision_id = ? AND outbound_id = ?",
        )
        .bind(revision)
        .bind(row.try_get::<i64, _>("outbound_id")?)
        .fetch_one(pool)
        .await?;
        rules.push(rule);
    }
    let balancer_rows = sqlx::query("SELECT balancer_id,tag,strategy,failure_threshold,cooldown_seconds,ewma_alpha,switch_margin,health_url,health_interval_seconds,health_timeout_seconds,health_max_failures FROM routing_balancers WHERE revision_id=? ORDER BY position")
        .bind(revision).fetch_all(pool).await?;
    let mut balancers = Vec::with_capacity(balancer_rows.len());
    for row in balancer_rows {
        let balancer_id: i64 = row.try_get("balancer_id")?;
        let member_rows = sqlx::query("SELECT o.tag outbound_tag,m.profile_name FROM routing_balancer_members m JOIN outbounds o ON o.revision_id=m.revision_id AND o.outbound_id=m.outbound_id WHERE m.revision_id=? AND m.balancer_id=? ORDER BY m.position")
            .bind(revision).bind(balancer_id).fetch_all(pool).await?;
        let mut selector = Vec::new();
        let mut profiles = Vec::new();
        for member in member_rows {
            let outbound_tag: String = member.try_get("outbound_tag")?;
            if let Some(name) = member.try_get::<Option<String>, _>("profile_name")? {
                profiles.push(BalancerProfileConfig { name, outbound_tag });
            } else {
                selector.push(outbound_tag);
            }
        }
        let adaptive =
            row.try_get::<Option<u32>, _>("failure_threshold")?
                .map(|failure_threshold| AdaptiveBalancerConfig {
                    failure_threshold,
                    cooldown_secs: row
                        .try_get::<Option<u64>, _>("cooldown_seconds")
                        .unwrap_or(None)
                        .unwrap_or(30),
                    ewma_alpha: row
                        .try_get::<Option<f64>, _>("ewma_alpha")
                        .unwrap_or(None)
                        .unwrap_or(0.2),
                    switch_margin: row
                        .try_get::<Option<f64>, _>("switch_margin")
                        .unwrap_or(None)
                        .unwrap_or(0.15),
                });
        let health_check =
            row.try_get::<Option<String>, _>("health_url")?
                .map(|url| HealthCheckConfig {
                    url,
                    interval_secs: row
                        .try_get::<Option<u64>, _>("health_interval_seconds")
                        .unwrap_or(None)
                        .unwrap_or(30),
                    timeout_secs: row
                        .try_get::<Option<u64>, _>("health_timeout_seconds")
                        .unwrap_or(None)
                        .unwrap_or(5),
                    max_failures: row
                        .try_get::<Option<u32>, _>("health_max_failures")
                        .unwrap_or(None)
                        .unwrap_or(3),
                });
        balancers.push(BalancerConfig {
            tag: row.try_get("tag")?,
            selector,
            strategy: row.try_get("strategy")?,
            profiles,
            adaptive,
            health_check,
        });
    }
    Ok(Some(RoutingConfig {
        domain_strategy: config.try_get("domain_strategy")?,
        geoip_file: config.try_get("geoip_file")?,
        geosite_file: config.try_get("geosite_file")?,
        rules,
        balancers,
    }))
}

fn optional_usize(row: &sqlx::mysql::MySqlRow, name: &str) -> StoreResult<Option<usize>> {
    row.try_get::<Option<u64>, _>(name)?
        .map(usize::try_from)
        .transpose()
        .map_err(decode_error)
}
fn bytes_string(row: &sqlx::mysql::MySqlRow, name: &str) -> StoreResult<Option<String>> {
    row.try_get::<Option<Vec<u8>>, _>(name)?
        .map(String::from_utf8)
        .transpose()
        .map_err(decode_error)
}
fn decode_error(error: impl std::error::Error + Send + Sync + 'static) -> StoreError {
    StoreError::Sql(sqlx::Error::Decode(Box::new(error)))
}

fn parse_string_enum<T: serde::de::DeserializeOwned>(value: &str) -> StoreResult<T> {
    serde_json::from_value(serde_json::Value::String(value.to_owned())).map_err(decode_error)
}

fn parse_number_or_string<T: serde::de::DeserializeOwned>(value: &str) -> StoreResult<T> {
    let value = value
        .parse::<u64>()
        .ok()
        .map(serde_json::Number::from)
        .map(serde_json::Value::Number)
        .unwrap_or_else(|| serde_json::Value::String(value.to_owned()));
    serde_json::from_value(value).map_err(decode_error)
}

fn parse_profile(value: &str) -> StoreResult<ProfileMode> {
    match value {
        "compat" => Ok(ProfileMode::Compat),
        "fast" => Ok(ProfileMode::Fast),
        "latency" => Ok(ProfileMode::Latency),
        "throughput" => Ok(ProfileMode::Throughput),
        "badnet" => Ok(ProfileMode::Badnet),
        "mobile" => Ok(ProfileMode::Mobile),
        "stealth" => Ok(ProfileMode::Stealth),
        other => Err(value_error("profile", other)),
    }
}
fn parse_protocol(value: &str) -> StoreResult<Protocol> {
    match value {
        "vless" => Ok(Protocol::Vless),
        "vmess" => Ok(Protocol::Vmess),
        "trojan" => Ok(Protocol::Trojan),
        "shadowsocks" => Ok(Protocol::Shadowsocks),
        "hysteria2" => Ok(Protocol::Hysteria2),
        "tuic" => Ok(Protocol::Tuic),
        "socks" => Ok(Protocol::Socks),
        "http" => Ok(Protocol::Http),
        "freedom" => Ok(Protocol::Freedom),
        other => Err(value_error("protocol", other)),
    }
}
fn parse_network(value: &str) -> StoreResult<NetworkType> {
    match value {
        "tcp" => Ok(NetworkType::Tcp),
        "ws" => Ok(NetworkType::Ws),
        "httpupgrade" => Ok(NetworkType::HttpUpgrade),
        "grpc" => Ok(NetworkType::Grpc),
        "quic" => Ok(NetworkType::Quic),
        "kcp" => Ok(NetworkType::Kcp),
        "splithttp" | "xhttp" => Ok(NetworkType::SplitHttp),
        other => Err(value_error("network", other)),
    }
}
fn parse_security(value: &str) -> StoreResult<SecurityType> {
    match value {
        "none" => Ok(SecurityType::None),
        "tls" => Ok(SecurityType::Tls),
        "reality" => Ok(SecurityType::Reality),
        "shadowtls" => Ok(SecurityType::ShadowTls),
        other => Err(value_error("security", other)),
    }
}
fn value_error(kind: &str, value: &str) -> StoreError {
    decode_error(ValueError(format!("invalid {kind} '{value}'")))
}

#[derive(Debug)]
struct ValueError(String);
impl std::fmt::Display for ValueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ValueError {}
