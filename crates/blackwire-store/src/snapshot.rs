use std::net::IpAddr;

use blackwire_config::schema::{
    ApiConfig, Config, DnsConfig, EndpointSettings, EndpointUser, FakeIpConfig, GrpcConfig,
    InboundConfig, InboundLimitsConfig, KcpConfig, LimitsConfig, LogConfig, NetworkType,
    OutboundConfig, ProfileMode, Protocol, RealityConfig, RoutingConfig, RoutingRule, SecurityType,
    SniffingConfig, StreamSettingsConfig, TlsConfig, WsConfig,
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
            "SELECT profile, metrics_enabled, metrics_address, api_enabled, api_listen_address, log_level, log_structured, log_file FROM global_config WHERE revision_id = ?",
        )
        .bind(revision)
        .fetch_one(self.pool())
        .await?;
        let profile: String = global.try_get("profile")?;
        let api_enabled: bool = global.try_get("api_enabled")?;
        let api_address: Option<String> = global.try_get("api_listen_address")?;
        let metrics_enabled: bool = global.try_get("metrics_enabled")?;
        Ok(StoredConfig {
            revision,
            config: Config {
                profile: parse_profile(&profile)?,
                fast: None,
                budget: None,
                vision: None,
                first_packet_boost: None,
                quic: None,
                datagram: None,
                fec: None,
                log: LogConfig {
                    level: global.try_get("log_level")?,
                    json: global.try_get("log_structured")?,
                    file: global.try_get("log_file")?,
                },
                dns: load_dns(self.pool(), revision).await?,
                routing: load_routing(self.pool(), revision).await?,
                tun: None,
                limits: load_limits(self.pool(), revision).await?,
                inbounds: load_inbounds(self.pool(), revision).await?,
                outbounds: load_outbounds(self.pool(), revision).await?,
                stats: None,
                api: api_enabled.then(|| ApiConfig {
                    listen: api_address.unwrap_or_else(|| "127.0.0.1:62789".into()),
                    token: None,
                    services: vec!["HandlerService".into(), "StatsService".into()],
                }),
                metrics_addr: if metrics_enabled {
                    global.try_get("metrics_address")?
                } else {
                    None
                },
            },
        })
    }
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
    if let Some(reality) = sqlx::query("SELECT show_details, destination, private_key, public_key, short_id, fingerprint, server_name, max_time_diff_seconds FROM reality_settings WHERE revision_id = ? AND endpoint_kind = ? AND endpoint_id = ?")
        .bind(revision).bind(kind).bind(id).fetch_optional(pool).await? {
        let names = sqlx::query_scalar("SELECT server_name FROM reality_server_names WHERE revision_id = ? AND endpoint_kind = ? AND endpoint_id = ? ORDER BY position")
            .bind(revision).bind(kind).bind(id).fetch_all(pool).await?;
        let short_ids = sqlx::query_scalar("SELECT short_id FROM reality_short_ids WHERE revision_id = ? AND endpoint_kind = ? AND endpoint_id = ? ORDER BY position")
            .bind(revision).bind(kind).bind(id).fetch_all(pool).await?;
        stream.reality_settings = Some(RealityConfig { show: reality.try_get("show_details")?, dest: reality.try_get("destination")?, private_key: reality.try_get("private_key")?, short_ids, public_key: reality.try_get("public_key")?, short_id: reality.try_get("short_id")?, fingerprint: reality.try_get("fingerprint")?, server_name: reality.try_get("server_name")?, server_names: names, max_time_diff: 0, max_time_diff_seconds: reality.try_get("max_time_diff_seconds")? });
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
    Ok(Some(RoutingConfig {
        domain_strategy: config.try_get("domain_strategy")?,
        geoip_file: config.try_get("geoip_file")?,
        geosite_file: config.try_get("geosite_file")?,
        rules,
        balancers: Vec::new(),
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
