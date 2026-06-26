use std::path::Path;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use validator::Validate;

use crate::{
    db,
    models::{Inbound, ManagedUser, Outbound, Settings},
    state::AppState,
    util,
};

pub fn build_value(state: &AppState) -> Result<Value> {
    let conn = state.lock_db()?;
    let settings = db::load_settings(&conn)?;
    let inbounds = db::load_inbounds(&conn)?;
    let outbounds = db::load_outbounds(&conn)?;
    let sections = db::load_section_map(&conn)?;
    let users = db::load_users(&conn)?;
    let mut inbound_json = Vec::new();

    for inbound in inbounds.into_iter().filter(|i| i.enabled) {
        let clients: Vec<Value> = users
            .iter()
            .filter(|u| u.inbound_id == inbound.id && u.enabled && u.enforcement_status == "active")
            .map(|u| client_entry(&inbound.protocol, u))
            .collect();
        if clients.is_empty()
            && (inbound.protocol == "tuic" || protocol_uses_clients(&inbound.protocol))
        {
            continue;
        }
        let mut settings_json = object_or_empty(&inbound.settings)?;
        if inbound.protocol == "tuic" {
            settings_json["users"] = Value::Array(clients);
        } else if protocol_uses_clients(&inbound.protocol) {
            settings_json["clients"] = Value::Array(clients);
        }
        let mut entry = json!({
            "tag": inbound.tag,
            "protocol": inbound.protocol,
            "listen": inbound.listen,
            "port": inbound.port,
            "settings": settings_json
        });
        if let Some(stream) = stream_settings(&inbound)? {
            entry["streamSettings"] = stream;
        }
        if let Some(sniffing) = optional_json(&inbound.sniffing)? {
            entry["sniffing"] = sniffing;
        }
        if let Some(limits) = optional_json(&inbound.limits)? {
            entry["limits"] = limits;
        }
        inbound_json.push(entry);
    }

    let enabled_outbounds = outbounds
        .into_iter()
        .filter(|outbound| outbound.enabled)
        .collect::<Vec<_>>();
    let mut outbound_json = Vec::new();
    for outbound in &enabled_outbounds {
        let mut entry = json!({
            "tag": outbound.tag,
            "protocol": outbound.protocol,
            "settings": object_or_empty(&outbound.settings)?
        });
        if let Some(stream) = optional_json(&outbound.stream_settings)? {
            entry["streamSettings"] = stream;
        }
        outbound_json.push(entry);
    }
    if outbound_json.is_empty() {
        outbound_json.push(json!({ "tag": "freedom", "protocol": "freedom" }));
    }

    let mut root = json!({
        "log": section_or_default(&sections, "log", json!({ "level": "info", "json": false }))?,
        "api": section_or_default(
            &sections,
            "api",
            json!({
                "listen": settings.grpc_address,
                "tag": "api",
                "services": ["HandlerService", "StatsService"],
            }),
        )?,
        "inbounds": inbound_json,
        "outbounds": outbound_json,
    });

    for key in ["dns", "routing", "tun", "limits", "stats", "fast"] {
        if let Some(value) = enabled_section(&sections, key)? {
            root[key] = value;
        }
    }
    if settings.adaptive_routing_enabled {
        root["routing"] = adaptive_routing_section(&enabled_outbounds);
    }
    if let Some(value) = enabled_section(&sections, "metricsAddr")? {
        root["metricsAddr"] = value;
    }
    if let Some(value) = enabled_section(&sections, "profile")? {
        root["profile"] = value;
    }

    Ok(root)
}

fn adaptive_routing_section(outbounds: &[Outbound]) -> Value {
    let tags = outbounds
        .iter()
        .map(|outbound| outbound.tag.as_str())
        .collect::<Vec<_>>();
    if tags.len() < 2 {
        return json!({ "rules": [{ "outboundTag": tags.first().copied().unwrap_or("freedom") }] });
    }
    let profiles = tags
        .iter()
        .enumerate()
        .map(|(idx, tag)| {
            json!({
                "name": if idx == 0 { "stable".to_string() } else { format!("backup-{idx}") },
                "outboundTag": tag
            })
        })
        .collect::<Vec<_>>();
    json!({
        "balancers": [{
            "tag": "auto-proxy",
            "selector": tags,
            "strategy": "adaptive",
            "profiles": profiles,
            "adaptive": {
                "failureThreshold": 2,
                "cooldownSecs": 30,
                "ewmaAlpha": 0.2,
                "switchMargin": 0.15
            },
            "health_check": {
                "url": "http://www.gstatic.com/generate_204",
                "interval_secs": 30,
                "timeout_secs": 5,
                "max_failures": 2
            }
        }],
        "rules": [{ "outboundTag": "auto-proxy" }]
    })
}

pub fn validate_value(value: &Value) -> Result<()> {
    let cfg: blackwire_config::Config = serde_json::from_value(value.clone())?;
    cfg.validate().map_err(|e| anyhow!(e.to_string()))
}

pub fn write(state: &AppState) -> Result<()> {
    let settings = {
        let conn = state.lock_db()?;
        db::load_settings(&conn)?
    };
    let value = build_value(state)?;
    validate_value(&value)?;
    write_value(&settings, &value)
}

pub fn write_if_generated_inbounds(state: &AppState) -> Result<bool> {
    let settings = {
        let conn = state.lock_db()?;
        db::load_settings(&conn)?
    };
    let value = build_value(state)?;
    if value
        .get("inbounds")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty)
    {
        return Ok(false);
    }
    validate_value(&value)?;
    write_value(&settings, &value)?;
    Ok(true)
}

fn write_value(settings: &Settings, value: &Value) -> Result<()> {
    if let Some(parent) = Path::new(&settings.config_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&settings.config_path, serde_json::to_vec_pretty(&value)?)?;
    Ok(())
}

pub fn stream_settings(inbound: &Inbound) -> Result<Option<Value>> {
    if !inbound.stream_settings.trim().is_empty() {
        return Ok(Some(serde_json::from_str(&inbound.stream_settings)?));
    }
    match inbound.transport.as_str() {
        "tcp" => Ok(None),
        "ws" => Ok(Some(json!({
            "network": "ws",
            "security": "none",
            "wsSettings": { "path": format!("/{}", inbound.tag) }
        }))),
        "reality" => Err(anyhow!(
            "REALITY inbound '{}' requires streamSettings",
            inbound.tag
        )),
        "grpc" => Ok(Some(json!({
            "network": "grpc",
            "security": "none",
            "grpcSettings": { "serviceName": inbound.tag }
        }))),
        "httpupgrade" => Ok(Some(json!({
            "network": "httpupgrade",
            "security": "none",
            "httpupgradeSettings": { "path": format!("/{}", inbound.tag) }
        }))),
        "splithttp" => Ok(Some(json!({
            "network": "splithttp",
            "security": "none",
            "splithttpSettings": { "path": format!("/{}", inbound.tag), "mode": "stream-one" }
        }))),
        "kcp" => Ok(Some(json!({ "network": "kcp", "security": "none" }))),
        "quic" => Ok(Some(json!({ "network": "quic", "security": "none" }))),
        _ => Err(anyhow!("unsupported transport '{}'", inbound.transport)),
    }
}

pub fn subscription_link(
    settings: &Settings,
    inbound: &Inbound,
    user: &ManagedUser,
) -> Result<String> {
    match inbound.protocol.as_str() {
        "vless" => Ok(vless_link(settings, inbound, user)),
        "vmess" => Ok(vmess_link(settings, inbound, user)),
        "trojan" => trojan_link(settings, inbound, user),
        "shadowsocks" => shadowsocks_link(settings, inbound, user),
        "hysteria2" => hysteria2_link(settings, inbound, user),
        "tuic" => tuic_link(settings, inbound, user),
        other => Err(anyhow!(
            "subscription link for protocol '{other}' requires manual config export"
        )),
    }
}

pub fn vless_link(settings: &Settings, inbound: &Inbound, user: &ManagedUser) -> String {
    let mut params = vec![
        format!("type={}", share_network(inbound)),
        "encryption=none".into(),
    ];
    append_transport_params(inbound, &mut params);
    let security = stream_security(inbound).unwrap_or_else(|| {
        if inbound.transport == "reality" {
            "reality".into()
        } else {
            "none".into()
        }
    });
    if security == "reality" {
        params.push("security=reality".into());
        params.push("headerType=none".into());
        if let Some(value) = reality_value(inbound, "/realitySettings/publicKey") {
            params.push(format!(
                "pbk={}",
                util::url_escape(&reality_public_key_share_value(&value))
            ));
        }
        if let Some(value) = reality_value(inbound, "/realitySettings/shortId").or_else(|| {
            serde_json::from_str::<Value>(&inbound.stream_settings)
                .ok()
                .and_then(|v| {
                    v.pointer("/realitySettings/shortIds")
                        .and_then(Value::as_array)
                        .and_then(|ids| ids.first())
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
        }) {
            params.push(format!("sid={}", util::url_escape(&value)));
        }
        if let Some(value) = reality_value(inbound, "/realitySettings/serverName") {
            params.push(format!("sni={}", util::url_escape(&value)));
        }
        if let Some(value) = reality_value(inbound, "/realitySettings/fingerprint") {
            params.push(format!("fp={}", util::url_escape(&value)));
        } else {
            params.push("fp=chrome".into());
        }
        let spider_x =
            reality_value(inbound, "/realitySettings/spiderX").unwrap_or_else(|| "/".into());
        params.push(format!("spx={}", url_escape_query_value(&spider_x)));
    } else if security == "tls" {
        params.push("security=tls".into());
        if let Some(value) = stream_value(inbound, "/tlsSettings/serverName") {
            params.push(format!("sni={}", util::url_escape(&value)));
        }
        if tls_share_requires_insecure(inbound) {
            params.push("allowInsecure=1".into());
        }
        if let Some(value) = tls_share_alpn(inbound) {
            params.push(format!("alpn={}", util::url_escape(&value)));
        }
    } else {
        params.push("security=none".into());
    }
    if !user.flow.trim().is_empty() {
        params.push(format!("flow={}", util::url_escape(&user.flow)));
    }
    format!(
        "vless://{}@{}:{}?{}#{}",
        user.uuid,
        settings.subscription_host,
        inbound.port,
        params.join("&"),
        util::url_escape(&user.email)
    )
}

fn vmess_link(settings: &Settings, inbound: &Inbound, user: &ManagedUser) -> String {
    let network = share_network(inbound);
    let security = stream_security(inbound).unwrap_or_else(|| "none".into());
    let host = stream_value(inbound, "/wsSettings/headers/Host")
        .or_else(|| stream_value(inbound, "/httpupgradeSettings/host"))
        .unwrap_or_default();
    let path = stream_value(inbound, "/wsSettings/path")
        .or_else(|| stream_value(inbound, "/httpupgradeSettings/path"))
        .or_else(|| stream_value(inbound, "/splithttpSettings/path"))
        .or_else(|| stream_value(inbound, "/grpcSettings/serviceName"))
        .unwrap_or_default();
    let transport_type = if network == "grpc" { "gun" } else { "none" };
    let sni = stream_value(inbound, "/tlsSettings/serverName").unwrap_or_default();
    let alpn = tls_share_alpn(inbound).unwrap_or_default();
    let vmess_security = if network == "quic" {
        "aes-128-gcm"
    } else {
        "auto"
    };
    let mut payload = json!({
        "v": "2",
        "ps": user.email,
        "add": settings.subscription_host,
        "port": inbound.port.to_string(),
        "id": user.uuid,
        "aid": "0",
        "scy": vmess_security,
        "security": vmess_security,
        "net": network,
        "type": transport_type,
        "host": host,
        "path": path,
        "tls": if security == "tls" { "tls" } else { "" },
        "sni": sni,
        "alpn": alpn,
    });
    if security == "tls" && tls_share_requires_insecure(inbound) {
        payload["allowInsecure"] = json!(true);
    }
    if network == "xhttp" {
        payload["mode"] = json!(splithttp_share_mode(inbound));
    }
    let encoded = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        serde_json::to_string(&payload).unwrap_or_default(),
    );
    format!("vmess://{encoded}")
}

fn reality_value(inbound: &Inbound, pointer: &str) -> Option<String> {
    stream_value(inbound, pointer)
}

fn reality_public_key_share_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() == 64 && trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        if let Ok(bytes) = hex::decode(trimmed) {
            if bytes.len() == 32 {
                return base64::Engine::encode(
                    &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                    bytes,
                );
            }
        }
    }
    trimmed.to_string()
}

fn url_escape_query_value(value: &str) -> String {
    value
        .bytes()
        .flat_map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![b as char]
            }
            _ => format!("%{b:02X}").chars().collect(),
        })
        .collect()
}

fn trojan_link(settings: &Settings, inbound: &Inbound, user: &ManagedUser) -> Result<String> {
    let password = credential_string(user, "password").unwrap_or_else(|| user.uuid.clone());
    let mut params = vec![format!("type={}", share_network(inbound))];
    append_transport_params(inbound, &mut params);
    let security = stream_security(inbound).unwrap_or_else(|| "tls".into());
    params.push(format!("security={security}"));
    if security == "tls" {
        if let Some(value) = stream_value(inbound, "/tlsSettings/serverName") {
            params.push(format!("sni={}", util::url_escape(&value)));
        }
        if tls_share_requires_insecure(inbound) {
            params.push("allowInsecure=1".into());
        }
        if let Some(value) = tls_share_alpn(inbound) {
            params.push(format!("alpn={}", util::url_escape(&value)));
        }
    }
    Ok(format!(
        "trojan://{}@{}:{}?{}#{}",
        util::url_escape(&password),
        settings.subscription_host,
        inbound.port,
        params.join("&"),
        util::url_escape(&user.email)
    ))
}

fn shadowsocks_link(settings: &Settings, inbound: &Inbound, user: &ManagedUser) -> Result<String> {
    let method = credential_string(user, "method")
        .or_else(|| settings_value(&inbound.settings, "method"))
        .unwrap_or_else(|| "2022-blake3-aes-256-gcm".into());
    let password = credential_string(user, "password").unwrap_or_else(|| user.uuid.clone());
    let password = shadowsocks_share_password(&method, &password);
    let userinfo = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD_NO_PAD,
        format!("{method}:{}", password),
    );
    Ok(format!(
        "ss://{}@{}:{}#{}",
        userinfo,
        settings.subscription_host,
        inbound.port,
        util::url_escape(&user.email)
    ))
}

fn shadowsocks_share_password(method: &str, password: &str) -> String {
    if !method.starts_with("2022-blake3-") {
        return password.to_string();
    }
    let psk = shadowsocks_2022_psk(password);
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, psk)
}

fn shadowsocks_2022_psk(password: &str) -> [u8; 32] {
    let engines = [
        base64::engine::general_purpose::STANDARD,
        base64::engine::general_purpose::STANDARD_NO_PAD,
        base64::engine::general_purpose::URL_SAFE,
        base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ];
    for engine in &engines {
        if let Ok(bytes) = base64::Engine::decode(engine, password) {
            if bytes.len() == 32 {
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes);
                return key;
            }
        }
    }
    blake3::hash(password.as_bytes()).into()
}

fn hysteria2_link(settings: &Settings, inbound: &Inbound, user: &ManagedUser) -> Result<String> {
    let auth = credential_string(user, "auth")
        .or_else(|| credential_string(user, "password"))
        .unwrap_or_else(|| user.uuid.clone());
    let mut params = Vec::new();
    if tls_share_requires_insecure(inbound) {
        params.push("insecure=1".to_string());
    }
    if let Some(value) = stream_value(inbound, "/tlsSettings/serverName") {
        params.push(format!("sni={}", util::url_escape(&value)));
    }
    let query = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };
    Ok(format!(
        "hysteria2://{}@{}:{}{}#{}",
        util::url_escape(&auth),
        settings.subscription_host,
        inbound.port,
        query,
        util::url_escape(&user.email)
    ))
}

fn tls_share_explicitly_allows_insecure(inbound: &Inbound) -> bool {
    stream_bool(inbound, "/tlsSettings/allowInsecure")
        || stream_bool(inbound, "/tlsSettings/insecure")
        || stream_bool(inbound, "/tlsSettings/skipCertVerify")
}

fn tls_share_requires_insecure(inbound: &Inbound) -> bool {
    if tls_share_explicitly_allows_insecure(inbound) {
        return true;
    }

    let Some(cert_file) = stream_value(inbound, "/tlsSettings/certificateFile") else {
        return false;
    };
    let cert_file = cert_file.to_ascii_lowercase();
    if cert_file.contains("/letsencrypt/") || cert_file.ends_with("/fullchain.pem") {
        return false;
    }
    cert_file.starts_with("/etc/blackwire/certs/")
}

fn tls_share_alpn(inbound: &Inbound) -> Option<String> {
    stream_value(inbound, "/tlsSettings/alpn").or_else(|| {
        ((share_network(inbound) == "quic" || inbound.protocol == "tuic")
            && stream_security(inbound).as_deref() == Some("tls"))
        .then(|| "h3".into())
    })
}

fn tuic_link(settings: &Settings, inbound: &Inbound, user: &ManagedUser) -> Result<String> {
    let password = credential_string(user, "password").unwrap_or_else(|| user.uuid.clone());
    let mut params = vec![format!("uuid={}", util::url_escape(&user.uuid))];
    if tls_share_requires_insecure(inbound) {
        params.push("insecure=1".to_string());
    }
    if let Some(value) = stream_value(inbound, "/tlsSettings/serverName") {
        params.push(format!("sni={}", util::url_escape(&value)));
    }
    if let Some(value) = tls_share_alpn(inbound) {
        params.push(format!("alpn={}", util::url_escape(&value)));
    }
    Ok(format!(
        "tuic://{}:{}@{}:{}?{}#{}",
        util::url_escape(&user.uuid),
        util::url_escape(&password),
        settings.subscription_host,
        inbound.port,
        params.join("&"),
        util::url_escape(&user.email)
    ))
}

fn share_network(inbound: &Inbound) -> String {
    let security = stream_security(inbound);
    if inbound.transport == "reality" || security.as_deref() == Some("reality") {
        "tcp".into()
    } else {
        let network =
            stream_value(inbound, "/network").unwrap_or_else(|| inbound.transport.clone());
        if network == "splithttp" {
            "xhttp".into()
        } else {
            network
        }
    }
}

fn append_transport_params(inbound: &Inbound, params: &mut Vec<String>) {
    if let Some(path) = stream_value(inbound, "/wsSettings/path")
        .or_else(|| stream_value(inbound, "/httpupgradeSettings/path"))
        .or_else(|| stream_value(inbound, "/splithttpSettings/path"))
    {
        params.push(format!("path={}", util::url_escape(&path)));
    }
    if let Some(host) = stream_value(inbound, "/wsSettings/headers/Host")
        .or_else(|| stream_value(inbound, "/httpupgradeSettings/host"))
    {
        params.push(format!("host={}", util::url_escape(&host)));
    }
    if let Some(service_name) = stream_value(inbound, "/grpcSettings/serviceName") {
        params.push(format!("serviceName={}", util::url_escape(&service_name)));
        params.push("mode=gun".into());
    }
    if share_network(inbound) == "xhttp" {
        params.push(format!("mode={}", splithttp_share_mode(inbound)));
    }
}

fn splithttp_share_mode(inbound: &Inbound) -> String {
    stream_value(inbound, "/splithttpSettings/mode")
        .filter(|mode| !mode.trim().is_empty())
        .unwrap_or_else(|| "stream-one".into())
}

fn client_entry(protocol: &str, user: &ManagedUser) -> Value {
    let mut entry = user.credential.as_object().cloned().unwrap_or_default();
    entry.insert("email".into(), json!(user.email));
    match protocol {
        "vless" | "vmess" => {
            entry.entry("id").or_insert_with(|| json!(user.uuid));
            if protocol == "vless" && !user.flow.is_empty() {
                entry.entry("flow").or_insert_with(|| json!(user.flow));
            }
        }
        "trojan" | "shadowsocks" => {
            entry.entry("password").or_insert_with(|| json!(user.uuid));
        }
        "hysteria2" => {
            entry.entry("auth").or_insert_with(|| json!(user.uuid));
        }
        "tuic" => {
            entry.entry("uuid").or_insert_with(|| json!(user.uuid));
            entry.entry("password").or_insert_with(|| json!(user.uuid));
        }
        _ => {}
    }
    Value::Object(entry)
}

fn protocol_uses_clients(protocol: &str) -> bool {
    matches!(
        protocol,
        "vless" | "vmess" | "trojan" | "shadowsocks" | "hysteria2" | "tuic"
    )
}

fn optional_json(raw: &str) -> Result<Option<Value>> {
    if raw.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(raw)?))
}

fn object_or_empty(raw: &str) -> Result<Value> {
    Ok(optional_json(raw)?.unwrap_or_else(|| json!({})))
}

fn enabled_section(
    sections: &std::collections::HashMap<String, crate::models::ConfigSection>,
    key: &str,
) -> Result<Option<Value>> {
    let Some(section) = sections.get(key) else {
        return Ok(None);
    };
    if !section.enabled {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&section.value)?))
}

fn section_or_default(
    sections: &std::collections::HashMap<String, crate::models::ConfigSection>,
    key: &str,
    default: Value,
) -> Result<Value> {
    Ok(enabled_section(sections, key)?.unwrap_or(default))
}

fn credential_string(user: &ManagedUser, key: &str) -> Option<String> {
    user.credential
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

fn settings_value(raw: &str, key: &str) -> Option<String> {
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|v| v.get(key).and_then(Value::as_str).map(str::to_string))
}

fn stream_value(inbound: &Inbound, pointer: &str) -> Option<String> {
    serde_json::from_str::<Value>(&inbound.stream_settings)
        .ok()
        .and_then(|v| {
            let value = v.pointer(pointer)?;
            if let Some(raw) = value.as_str() {
                return Some(raw.to_string());
            }
            if let Some(items) = value.as_array() {
                return Some(
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(","),
                );
            }
            None
        })
        .filter(|v| !v.is_empty())
}

fn stream_bool(inbound: &Inbound, pointer: &str) -> bool {
    serde_json::from_str::<Value>(&inbound.stream_settings)
        .ok()
        .and_then(|v| v.pointer(pointer).and_then(Value::as_bool))
        .unwrap_or(false)
}

fn stream_security(inbound: &Inbound) -> Option<String> {
    serde_json::from_str::<Value>(&inbound.stream_settings)
        .ok()
        .and_then(|v| {
            v.get("security")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use rusqlite::{params, Connection};

    use super::*;

    fn test_state() -> AppState {
        let conn = Connection::open_in_memory().unwrap();
        let data_dir = std::env::temp_dir().join(format!("black-ui-test-{}", uuid::Uuid::new_v4()));
        db::init(&conn, &data_dir).unwrap();
        AppState {
            db: Arc::new(Mutex::new(conn)),
        }
    }

    fn assert_link_omits_mux_params(link: &str) {
        assert!(
            !link.contains("mux="),
            "share link unexpectedly enables mux: {link}"
        );
        assert!(
            !link.contains("xmux="),
            "share link unexpectedly enables xmux: {link}"
        );
    }

    fn assert_vmess_payload_omits_mux(payload: &serde_json::Value) {
        assert!(
            payload.get("mux").is_none(),
            "vmess share payload unexpectedly enables mux: {payload}"
        );
        assert!(
            payload.get("xmux").is_none(),
            "vmess share payload unexpectedly enables xmux: {payload}"
        );
    }

    fn tls_policy_inbound(cert_file: &str, extra_tls: serde_json::Value) -> Inbound {
        let mut tls = json!({
            "serverName": "www.microsoft.com",
            "certificateFile": cert_file,
            "keyFile": "/etc/blackwire/certs/test.key"
        });
        if let (Some(base), Some(extra)) = (tls.as_object_mut(), extra_tls.as_object()) {
            for (key, value) in extra {
                base.insert(key.clone(), value.clone());
            }
        }
        Inbound {
            id: 1,
            tag: "tls-policy".into(),
            listen: "::".into(),
            port: 443,
            protocol: "vless".into(),
            enabled: true,
            transport: "quic".into(),
            settings: "{}".into(),
            stream_settings: json!({
                "network": "quic",
                "security": "tls",
                "tlsSettings": tls
            })
            .to_string(),
            sniffing: String::new(),
            limits: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn tls_share_insecure_policy_matches_certificate_source() {
        assert!(tls_share_requires_insecure(&tls_policy_inbound(
            "/etc/blackwire/certs/generated.crt",
            json!({})
        )));
        assert!(tls_share_requires_insecure(&tls_policy_inbound(
            "/opt/custom/public.pem",
            json!({"allowInsecure": true})
        )));
        assert!(!tls_share_requires_insecure(&tls_policy_inbound(
            "/etc/letsencrypt/live/example.com/fullchain.pem",
            json!({})
        )));
        assert!(!tls_share_requires_insecure(&tls_policy_inbound(
            "/opt/custom/public.pem",
            json!({})
        )));
    }

    #[test]
    fn generated_minimal_config_validates() {
        let state = test_state();
        {
            let conn = state.lock_db().unwrap();
            let ts = util::now();
            conn.execute(
                "INSERT INTO inbounds (tag, listen, port, protocol, enabled, transport, settings, stream_settings, sniffing, limits, created_at, updated_at)
                 VALUES ('socks', '127.0.0.1', 18080, 'socks', 1, 'tcp', '{}', '', '', '', ?1, ?1)",
                params![ts],
            )
            .unwrap();
        }
        let value = build_value(&state).unwrap();
        validate_value(&value).unwrap();
    }

    #[test]
    fn generated_vless_ws_user_config_validates() {
        let state = test_state();
        {
            let conn = state.lock_db().unwrap();
            let ts = util::now();
            conn.execute(
                "INSERT INTO inbounds (tag, listen, port, protocol, enabled, transport, settings, stream_settings, sniffing, limits, created_at, updated_at)
                 VALUES ('vless-ws', '127.0.0.1', 18001, 'vless', 1, 'ws', '{}', '', '', '', ?1, ?1)",
                params![ts],
            )
            .unwrap();
            let inbound_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO users (inbound_id, email, uuid, flow, credential_json, note, enabled, traffic_limit_bytes, expiry_at, sub_token, enforcement_status, created_at, updated_at)
                 VALUES (?1, 'alice@example.test', '00000000-0000-4000-8000-000000000001', '', '{}', '', 1, NULL, NULL, 'token', 'active', ?2, ?2)",
                params![inbound_id, util::now()],
            )
            .unwrap();
        }
        let value = build_value(&state).unwrap();
        validate_value(&value).unwrap();
        assert_eq!(value["inbounds"][0]["protocol"], "vless");
        assert_eq!(
            value["inbounds"][0]["settings"]["clients"][0]["email"],
            "alice@example.test"
        );
    }

    #[test]
    fn generated_adaptive_balancer_routing_config_validates() {
        let state = test_state();
        {
            let conn = state.lock_db().unwrap();
            let ts = util::now();
            conn.execute(
                "INSERT INTO inbounds (tag, listen, port, protocol, enabled, transport, settings, stream_settings, sniffing, limits, created_at, updated_at)
                 VALUES ('socks', '127.0.0.1', 18080, 'socks', 1, 'tcp', '{}', '', '', '', ?1, ?1)",
                params![ts],
            )
            .unwrap();
            conn.execute(
                "UPDATE outbounds SET tag='primary-vless', protocol='freedom', enabled=1, settings='{}', stream_settings='', updated_at=?1 WHERE tag='freedom'",
                params![ts],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO outbounds (tag, protocol, enabled, settings, stream_settings, created_at, updated_at)
                 VALUES ('backup-ss2022', 'freedom', 1, '{}', '', ?1, ?1)",
                params![ts],
            )
            .unwrap();
            conn.execute(
                "UPDATE config_sections SET enabled=1, value=?1, updated_at=?2 WHERE name='routing'",
                params![
                    r#"{
                      "balancers": [{
                        "tag": "auto-proxy",
                        "selector": ["primary-vless", "backup-ss2022"],
                        "strategy": "adaptive",
                        "profiles": [
                          { "name": "stable", "outboundTag": "primary-vless" },
                          { "name": "backup", "outboundTag": "backup-ss2022" }
                        ],
                        "adaptive": {
                          "failureThreshold": 2,
                          "cooldownSecs": 30,
                          "ewmaAlpha": 0.2,
                          "switchMargin": 0.15
                        },
                        "health_check": {
                          "url": "http://www.gstatic.com/generate_204",
                          "interval_secs": 30,
                          "timeout_secs": 5,
                          "max_failures": 2
                        }
                      }],
                      "rules": [{ "outboundTag": "auto-proxy" }]
                    }"#,
                    ts
                ],
            )
            .unwrap();
        }

        let value = build_value(&state).unwrap();
        validate_value(&value).unwrap();
        assert_eq!(value["routing"]["balancers"][0]["strategy"], "adaptive");
        assert_eq!(
            value["routing"]["balancers"][0]["profiles"][0]["outboundTag"],
            "primary-vless"
        );
    }

    #[test]
    fn adaptive_routing_setting_generates_balancer_only_with_multiple_outbounds() {
        let state = test_state();
        {
            let conn = state.lock_db().unwrap();
            let ts = util::now();
            conn.execute(
                "INSERT INTO inbounds (tag, listen, port, protocol, enabled, transport, settings, stream_settings, sniffing, limits, created_at, updated_at)
                 VALUES ('socks', '127.0.0.1', 18080, 'socks', 1, 'tcp', '{}', '', '', '', ?1, ?1)",
                params![ts],
            )
            .unwrap();
            db::save_settings(
                &conn,
                &Settings {
                    config_path: "/tmp/config.json".into(),
                    grpc_enabled: false,
                    grpc_address: "127.0.0.1:62789".into(),
                    firewall_auto_open: false,
                    public_base_url: "http://127.0.0.1:18080".into(),
                    subscription_host: "127.0.0.1".into(),
                    enforcement_interval_seconds: 30,
                    adaptive_routing_enabled: true,
                    ..Settings::default()
                },
            )
            .unwrap();
        }

        let value = build_value(&state).unwrap();
        validate_value(&value).unwrap();
        assert!(value["routing"].get("balancers").is_none());
        assert_eq!(value["routing"]["rules"][0]["outboundTag"], "freedom");

        {
            let conn = state.lock_db().unwrap();
            let ts = util::now();
            conn.execute(
                "INSERT INTO outbounds (tag, protocol, enabled, settings, stream_settings, created_at, updated_at)
                 VALUES ('backup-freedom', 'freedom', 1, '{}', '', ?1, ?1)",
                params![ts],
            )
            .unwrap();
        }

        let value = build_value(&state).unwrap();
        validate_value(&value).unwrap();
        assert_eq!(value["routing"]["balancers"][0]["strategy"], "adaptive");
        assert_eq!(value["routing"]["rules"][0]["outboundTag"], "auto-proxy");
    }

    #[test]
    fn vless_reality_subscription_uses_common_xray_uri_params() {
        let settings = Settings {
            config_path: "/tmp/config.json".into(),
            grpc_enabled: false,
            grpc_address: "127.0.0.1:62789".into(),
            firewall_auto_open: false,
            public_base_url: "http://127.0.0.1:18080".into(),
            subscription_host: "203.0.113.10".into(),
            enforcement_interval_seconds: 30,
            adaptive_routing_enabled: false,
            ..Settings::default()
        };
        let inbound = Inbound {
            id: 1,
            tag: "vless-reality-in".into(),
            listen: "0.0.0.0".into(),
            port: 443,
            protocol: "vless".into(),
            enabled: true,
            transport: "reality".into(),
            settings: "{}".into(),
            stream_settings: r#"{
              "network": "tcp",
              "security": "reality",
              "realitySettings": {
                "publicKey": "e1df9c8812b5ce9b3bd36da542896be856ad0a6c6e6df9d910a4040c07268142",
                "shortId": "feedbeef",
                "serverName": "www.microsoft.com",
                "fingerprint": "chrome"
              }
            }"#
            .into(),
            sniffing: String::new(),
            limits: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let user = ManagedUser {
            id: 1,
            inbound_id: 1,
            email: "Mollah".into(),
            uuid: "459dc0c8-d891-4768-9234-faf11fd26b5d".into(),
            flow: String::new(),
            credential: json!({}),
            note: String::new(),
            enabled: true,
            traffic_limit_bytes: None,
            expiry_at: None,
            upload_bytes: 0,
            download_bytes: 0,
            sub_token: "token".into(),
            enforcement_status: "active".into(),
            created_at: String::new(),
            updated_at: String::new(),
        };

        let link = vless_link(&settings, &inbound, &user);
        assert!(link.starts_with("vless://459dc0c8-d891-4768-9234-faf11fd26b5d@203.0.113.10:443?"));
        assert!(link.contains("type=tcp"));
        assert!(link.contains("security=reality"));
        assert!(link.contains("headerType=none"));
        assert!(link.contains("pbk=4d-ciBK1zps7022lQolr6FatCmxubfnZEKQEDAcmgUI"));
        assert!(link.contains("sid=feedbeef"));
        assert!(link.contains("sni=www.microsoft.com"));
        assert!(link.contains("fp=chrome"));
        assert!(link.contains("spx=%2F"));
        assert!(link.ends_with("#Mollah"));
        assert_link_omits_mux_params(&link);
    }

    #[test]
    fn vless_grpc_subscription_includes_gun_mode() {
        let settings = Settings {
            config_path: "/tmp/config.json".into(),
            grpc_enabled: false,
            grpc_address: "127.0.0.1:62789".into(),
            firewall_auto_open: false,
            public_base_url: "http://127.0.0.1:18080".into(),
            subscription_host: "203.0.113.10".into(),
            enforcement_interval_seconds: 30,
            adaptive_routing_enabled: false,
            ..Settings::default()
        };
        let inbound = Inbound {
            id: 1,
            tag: "manual-vless-grpc".into(),
            listen: "0.0.0.0".into(),
            port: 10445,
            protocol: "vless".into(),
            enabled: true,
            transport: "grpc".into(),
            settings: "{}".into(),
            stream_settings: r#"{
              "network": "grpc",
              "security": "none",
              "grpcSettings": {
                "serviceName": "manual-vless-grpc"
              }
            }"#
            .into(),
            sniffing: String::new(),
            limits: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let user = ManagedUser {
            id: 1,
            inbound_id: 1,
            email: "manual-vless-grpc@example.local".into(),
            uuid: "57d975c8-d935-47ca-bb62-31c7efac938d".into(),
            flow: String::new(),
            credential: json!({}),
            note: String::new(),
            enabled: true,
            traffic_limit_bytes: None,
            expiry_at: None,
            upload_bytes: 0,
            download_bytes: 0,
            sub_token: "token".into(),
            enforcement_status: "active".into(),
            created_at: String::new(),
            updated_at: String::new(),
        };

        let link = vless_link(&settings, &inbound, &user);
        assert!(
            link.starts_with("vless://57d975c8-d935-47ca-bb62-31c7efac938d@203.0.113.10:10445?")
        );
        assert!(link.contains("type=grpc"));
        assert!(link.contains("serviceName=manual-vless-grpc"));
        assert!(link.contains("mode=gun"));
        assert!(link.contains("security=none"));
        assert!(link.ends_with("#manual-vless-grpc%40example.local"));
        assert_link_omits_mux_params(&link);
    }

    #[test]
    fn vmess_quic_tls_subscription_defaults_to_h3_alpn() {
        let settings = Settings {
            config_path: "/tmp/config.json".into(),
            grpc_enabled: false,
            grpc_address: "127.0.0.1:62789".into(),
            firewall_auto_open: false,
            public_base_url: "http://127.0.0.1:18080".into(),
            subscription_host: "203.0.113.10".into(),
            enforcement_interval_seconds: 30,
            adaptive_routing_enabled: false,
            ..Settings::default()
        };
        let inbound = Inbound {
            id: 1,
            tag: "manual-vmess-quic".into(),
            listen: "::".into(),
            port: 10549,
            protocol: "vmess".into(),
            enabled: true,
            transport: "quic".into(),
            settings: "{}".into(),
            stream_settings: r#"{
              "network": "quic",
              "security": "tls",
              "tlsSettings": {
                "serverName": "www.microsoft.com",
                "certificateFile": "/etc/blackwire/certs/hysteria2.crt",
                "keyFile": "/etc/blackwire/certs/hysteria2.key"
              }
            }"#
            .into(),
            sniffing: String::new(),
            limits: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let user = ManagedUser {
            id: 1,
            inbound_id: 1,
            email: "manual-vmess-quic@example.local".into(),
            uuid: "f613e9d6-fcc2-4030-a57f-9523f8d1721d".into(),
            flow: String::new(),
            credential: json!({}),
            note: String::new(),
            enabled: true,
            traffic_limit_bytes: None,
            expiry_at: None,
            upload_bytes: 0,
            download_bytes: 0,
            sub_token: "token".into(),
            enforcement_status: "active".into(),
            created_at: String::new(),
            updated_at: String::new(),
        };

        let link = vmess_link(&settings, &inbound, &user);
        let encoded = link.strip_prefix("vmess://").unwrap();
        let decoded =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded).unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(payload["net"], "quic");
        assert_eq!(payload["tls"], "tls");
        assert_eq!(payload["sni"], "www.microsoft.com");
        assert_eq!(payload["alpn"], "h3");
        assert_eq!(payload["scy"], "aes-128-gcm");
        assert_eq!(payload["security"], "aes-128-gcm");
        assert_eq!(payload["allowInsecure"], true);
        assert!(payload.get("allowinsecure").is_none());
        assert!(payload.get("insecure").is_none());
        assert_vmess_payload_omits_mux(&payload);
    }

    #[test]
    fn vless_quic_tls_subscription_includes_insecure_for_self_signed_cert() {
        let settings = Settings {
            config_path: "/tmp/config.json".into(),
            grpc_enabled: false,
            grpc_address: "127.0.0.1:62789".into(),
            firewall_auto_open: false,
            public_base_url: "http://127.0.0.1:18080".into(),
            subscription_host: "203.0.113.10".into(),
            enforcement_interval_seconds: 30,
            adaptive_routing_enabled: false,
            ..Settings::default()
        };
        let inbound = Inbound {
            id: 1,
            tag: "manual-vless-quic".into(),
            listen: "::".into(),
            port: 10449,
            protocol: "vless".into(),
            enabled: true,
            transport: "quic".into(),
            settings: r#"{"decryption":"none"}"#.into(),
            stream_settings: r#"{
              "network": "quic",
              "security": "tls",
              "tlsSettings": {
                "serverName": "www.microsoft.com",
                "certificateFile": "/etc/blackwire/certs/vless-quic.crt",
                "keyFile": "/etc/blackwire/certs/vless-quic.key"
              }
            }"#
            .into(),
            sniffing: String::new(),
            limits: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let user = ManagedUser {
            id: 1,
            inbound_id: 1,
            email: "manual-vless-quic@example.local".into(),
            uuid: "64e29104-b88e-4514-b6a1-3c77a7707eef".into(),
            flow: String::new(),
            credential: json!({}),
            note: String::new(),
            enabled: true,
            traffic_limit_bytes: None,
            expiry_at: None,
            upload_bytes: 0,
            download_bytes: 0,
            sub_token: "token".into(),
            enforcement_status: "active".into(),
            created_at: String::new(),
            updated_at: String::new(),
        };

        let link = vless_link(&settings, &inbound, &user);
        assert!(link.contains("type=quic"));
        assert!(link.contains("security=tls"));
        assert!(link.contains("sni=www.microsoft.com"));
        assert!(link.contains("allowInsecure=1"));
        assert!(link.contains("alpn=h3"));
        assert_link_omits_mux_params(&link);
    }

    #[test]
    fn trojan_quic_tls_subscription_includes_insecure_for_self_signed_cert() {
        let settings = Settings {
            config_path: "/tmp/config.json".into(),
            grpc_enabled: false,
            grpc_address: "127.0.0.1:62789".into(),
            firewall_auto_open: false,
            public_base_url: "http://127.0.0.1:18080".into(),
            subscription_host: "203.0.113.10".into(),
            enforcement_interval_seconds: 30,
            adaptive_routing_enabled: false,
            ..Settings::default()
        };
        let inbound = Inbound {
            id: 1,
            tag: "manual-trojan-quic".into(),
            listen: "::".into(),
            port: 10649,
            protocol: "trojan".into(),
            enabled: true,
            transport: "quic".into(),
            settings: "{}".into(),
            stream_settings: r#"{
              "network": "quic",
              "security": "tls",
              "tlsSettings": {
                "serverName": "www.microsoft.com",
                "certificateFile": "/etc/blackwire/certs/trojan-quic.crt",
                "keyFile": "/etc/blackwire/certs/trojan-quic.key"
              }
            }"#
            .into(),
            sniffing: String::new(),
            limits: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let user = ManagedUser {
            id: 1,
            inbound_id: 1,
            email: "manual-trojan-quic@example.local".into(),
            uuid: "94b109af-4729-446a-a2c4-c2f25421ab5f".into(),
            flow: String::new(),
            credential: json!({"password": "secret"}),
            note: String::new(),
            enabled: true,
            traffic_limit_bytes: None,
            expiry_at: None,
            upload_bytes: 0,
            download_bytes: 0,
            sub_token: "token".into(),
            enforcement_status: "active".into(),
            created_at: String::new(),
            updated_at: String::new(),
        };

        let link = trojan_link(&settings, &inbound, &user).unwrap();
        assert!(link.contains("type=quic"));
        assert!(link.contains("security=tls"));
        assert!(link.contains("sni=www.microsoft.com"));
        assert!(link.contains("allowInsecure=1"));
        assert!(link.contains("alpn=h3"));
        assert_link_omits_mux_params(&link);
    }

    #[test]
    fn vmess_grpc_subscription_uses_service_name_path_and_gun_type() {
        let settings = Settings {
            config_path: "/tmp/config.json".into(),
            grpc_enabled: false,
            grpc_address: "127.0.0.1:62789".into(),
            firewall_auto_open: false,
            public_base_url: "http://127.0.0.1:18080".into(),
            subscription_host: "203.0.113.10".into(),
            enforcement_interval_seconds: 30,
            adaptive_routing_enabled: false,
            ..Settings::default()
        };
        let inbound = Inbound {
            id: 1,
            tag: "manual-vmess-grpc".into(),
            listen: "::".into(),
            port: 10545,
            protocol: "vmess".into(),
            enabled: true,
            transport: "grpc".into(),
            settings: "{}".into(),
            stream_settings: r#"{
              "network": "grpc",
              "security": "none",
              "grpcSettings": {
                "serviceName": "manual-vmess-grpc"
              }
            }"#
            .into(),
            sniffing: String::new(),
            limits: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let user = ManagedUser {
            id: 1,
            inbound_id: 1,
            email: "manual-vmess-grpc@example.local".into(),
            uuid: "bb621137-204c-49c6-a1e6-3774af2888d6".into(),
            flow: String::new(),
            credential: json!({}),
            note: String::new(),
            enabled: true,
            traffic_limit_bytes: None,
            expiry_at: None,
            upload_bytes: 0,
            download_bytes: 0,
            sub_token: "token".into(),
            enforcement_status: "active".into(),
            created_at: String::new(),
            updated_at: String::new(),
        };

        let link = vmess_link(&settings, &inbound, &user);
        let encoded = link.strip_prefix("vmess://").unwrap();
        let decoded =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded).unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(payload["net"], "grpc");
        assert_eq!(payload["type"], "gun");
        assert_eq!(payload["path"], "manual-vmess-grpc");
        assert_eq!(payload["tls"], "");
        assert_vmess_payload_omits_mux(&payload);
    }

    #[test]
    fn vless_splithttp_subscription_uses_xhttp_share_name() {
        let settings = Settings {
            config_path: "/tmp/config.json".into(),
            grpc_enabled: false,
            grpc_address: "127.0.0.1:62789".into(),
            firewall_auto_open: false,
            public_base_url: "http://127.0.0.1:18080".into(),
            subscription_host: "203.0.113.10".into(),
            enforcement_interval_seconds: 30,
            adaptive_routing_enabled: false,
            ..Settings::default()
        };
        let inbound = Inbound {
            id: 1,
            tag: "manual-vless-splithttp".into(),
            listen: "::".into(),
            port: 10546,
            protocol: "vless".into(),
            enabled: true,
            transport: "splithttp".into(),
            settings: "{}".into(),
            stream_settings: r#"{
              "network": "splithttp",
              "security": "none",
              "splithttpSettings": {
                "mode": "stream-one",
                "path": "/manual/vless/splithttp"
              }
            }"#
            .into(),
            sniffing: String::new(),
            limits: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let user = ManagedUser {
            id: 1,
            inbound_id: 1,
            email: "manual-vless-splithttp@example.local".into(),
            uuid: "57d975c8-d935-47ca-bb62-31c7efac938d".into(),
            flow: String::new(),
            credential: json!({}),
            note: String::new(),
            enabled: true,
            traffic_limit_bytes: None,
            expiry_at: None,
            upload_bytes: 0,
            download_bytes: 0,
            sub_token: "token".into(),
            enforcement_status: "active".into(),
            created_at: String::new(),
            updated_at: String::new(),
        };

        let link = vless_link(&settings, &inbound, &user);
        assert!(link.contains("type=xhttp"));
        assert!(link.contains("path=/manual/vless/splithttp"));
        assert!(link.contains("mode=stream-one"));
        assert!(link.contains("security=none"));
        assert_link_omits_mux_params(&link);
    }

    #[test]
    fn vmess_splithttp_subscription_uses_xhttp_share_name() {
        let settings = Settings {
            config_path: "/tmp/config.json".into(),
            grpc_enabled: false,
            grpc_address: "127.0.0.1:62789".into(),
            firewall_auto_open: false,
            public_base_url: "http://127.0.0.1:18080".into(),
            subscription_host: "203.0.113.10".into(),
            enforcement_interval_seconds: 30,
            adaptive_routing_enabled: false,
            ..Settings::default()
        };
        let inbound = Inbound {
            id: 1,
            tag: "manual-vmess-splithttp".into(),
            listen: "::".into(),
            port: 10547,
            protocol: "vmess".into(),
            enabled: true,
            transport: "splithttp".into(),
            settings: "{}".into(),
            stream_settings: r#"{
              "network": "splithttp",
              "security": "none",
              "splithttpSettings": {
                "mode": "stream-one",
                "path": "/manual/vmess/splithttp"
              }
            }"#
            .into(),
            sniffing: String::new(),
            limits: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let user = ManagedUser {
            id: 1,
            inbound_id: 1,
            email: "manual-vmess-splithttp@example.local".into(),
            uuid: "efcbf11a-8fcc-4d05-a494-2d173a177e32".into(),
            flow: String::new(),
            credential: json!({}),
            note: String::new(),
            enabled: true,
            traffic_limit_bytes: None,
            expiry_at: None,
            upload_bytes: 0,
            download_bytes: 0,
            sub_token: "token".into(),
            enforcement_status: "active".into(),
            created_at: String::new(),
            updated_at: String::new(),
        };

        let link = vmess_link(&settings, &inbound, &user);
        let encoded = link.strip_prefix("vmess://").unwrap();
        let decoded =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded).unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(payload["net"], "xhttp");
        assert_eq!(payload["path"], "/manual/vmess/splithttp");
        assert_eq!(payload["mode"], "stream-one");
        assert_eq!(payload["tls"], "");
        assert_vmess_payload_omits_mux(&payload);
    }

    #[test]
    fn hysteria2_subscription_includes_insecure_for_self_signed_cert() {
        let settings = Settings {
            config_path: "/tmp/config.json".into(),
            grpc_enabled: false,
            grpc_address: "127.0.0.1:62789".into(),
            firewall_auto_open: false,
            public_base_url: "http://127.0.0.1:18080".into(),
            subscription_host: "203.0.113.10".into(),
            enforcement_interval_seconds: 30,
            adaptive_routing_enabled: false,
            ..Settings::default()
        };
        let inbound = Inbound {
            id: 1,
            tag: "hysteria2-in".into(),
            listen: "::".into(),
            port: 443,
            protocol: "hysteria2".into(),
            enabled: true,
            transport: "quic".into(),
            settings: r#"{"auth":"secret"}"#.into(),
            stream_settings: r#"{
              "network": "tcp",
              "security": "tls",
              "tlsSettings": {
                "serverName": "www.microsoft.com",
                "certificateFile": "/etc/blackwire/certs/hysteria2.crt"
              }
            }"#
            .into(),
            sniffing: String::new(),
            limits: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let user = ManagedUser {
            id: 1,
            inbound_id: 1,
            email: "hysteria2@example.local".into(),
            uuid: "fallback-auth".into(),
            flow: String::new(),
            credential: json!({"auth": "secret"}),
            note: String::new(),
            enabled: true,
            traffic_limit_bytes: None,
            expiry_at: None,
            upload_bytes: 0,
            download_bytes: 0,
            sub_token: "token".into(),
            enforcement_status: "active".into(),
            created_at: String::new(),
            updated_at: String::new(),
        };

        let link = hysteria2_link(&settings, &inbound, &user).unwrap();
        assert_eq!(
            link,
            "hysteria2://secret@203.0.113.10:443?insecure=1&sni=www.microsoft.com#hysteria2%40example.local"
        );
        assert_link_omits_mux_params(&link);
    }

    #[test]
    fn hysteria2_subscription_omits_insecure_for_public_cert() {
        let settings = Settings {
            config_path: "/tmp/config.json".into(),
            grpc_enabled: false,
            grpc_address: "127.0.0.1:62789".into(),
            firewall_auto_open: false,
            public_base_url: "http://127.0.0.1:18080".into(),
            subscription_host: "203.0.113.10".into(),
            enforcement_interval_seconds: 30,
            adaptive_routing_enabled: false,
            ..Settings::default()
        };
        let inbound = Inbound {
            id: 1,
            tag: "hysteria2-in".into(),
            listen: "::".into(),
            port: 443,
            protocol: "hysteria2".into(),
            enabled: true,
            transport: "quic".into(),
            settings: r#"{"auth":"secret"}"#.into(),
            stream_settings: r#"{
              "network": "tcp",
              "security": "tls",
              "tlsSettings": {
                "serverName": "www.microsoft.com",
                "certificateFile": "/etc/letsencrypt/live/example.com/fullchain.pem"
              }
            }"#
            .into(),
            sniffing: String::new(),
            limits: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let user = ManagedUser {
            id: 1,
            inbound_id: 1,
            email: "hysteria2@example.local".into(),
            uuid: "fallback-auth".into(),
            flow: String::new(),
            credential: json!({"auth": "secret"}),
            note: String::new(),
            enabled: true,
            traffic_limit_bytes: None,
            expiry_at: None,
            upload_bytes: 0,
            download_bytes: 0,
            sub_token: "token".into(),
            enforcement_status: "active".into(),
            created_at: String::new(),
            updated_at: String::new(),
        };

        let link = hysteria2_link(&settings, &inbound, &user).unwrap();
        assert_eq!(
            link,
            "hysteria2://secret@203.0.113.10:443?sni=www.microsoft.com#hysteria2%40example.local"
        );
        assert_link_omits_mux_params(&link);
    }

    #[test]
    fn shadowsocks_2022_subscription_exports_padded_standard_base64_key() {
        let settings = Settings {
            config_path: "/tmp/config.json".into(),
            grpc_enabled: false,
            grpc_address: "127.0.0.1:62789".into(),
            firewall_auto_open: false,
            public_base_url: "http://127.0.0.1:18080".into(),
            subscription_host: "203.0.113.10".into(),
            enforcement_interval_seconds: 30,
            adaptive_routing_enabled: false,
            ..Settings::default()
        };
        let inbound = Inbound {
            id: 1,
            tag: "ss2022-in".into(),
            listen: "::".into(),
            port: 8388,
            protocol: "shadowsocks".into(),
            enabled: true,
            transport: "tcp".into(),
            settings: r#"{"method":"2022-blake3-aes-256-gcm"}"#.into(),
            stream_settings: String::new(),
            sniffing: String::new(),
            limits: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let user = ManagedUser {
            id: 1,
            inbound_id: 1,
            email: "manual-shadowsocks-2022@example.local".into(),
            uuid: "fallback-password".into(),
            flow: String::new(),
            credential: json!({
                "password": "PtySNW17x+SKO1j3kMEQRV0j6/vbYH67zqCuEkkb3MA"
            }),
            note: String::new(),
            enabled: true,
            traffic_limit_bytes: None,
            expiry_at: None,
            upload_bytes: 0,
            download_bytes: 0,
            sub_token: "token".into(),
            enforcement_status: "active".into(),
            created_at: String::new(),
            updated_at: String::new(),
        };

        let link = shadowsocks_link(&settings, &inbound, &user).unwrap();
        let userinfo = link
            .strip_prefix("ss://")
            .and_then(|value| value.split('@').next())
            .unwrap();
        let decoded =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD_NO_PAD, userinfo)
                .unwrap();
        let decoded = String::from_utf8(decoded).unwrap();
        let (method, password) = decoded.split_once(':').unwrap();
        assert_eq!(method, "2022-blake3-aes-256-gcm");
        assert_eq!(password, "PtySNW17x+SKO1j3kMEQRV0j6/vbYH67zqCuEkkb3MA=");
        assert_eq!(
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, password)
                .unwrap()
                .len(),
            32
        );
    }

    #[test]
    fn tuic_subscription_includes_insecure_for_self_signed_cert() {
        let settings = Settings {
            config_path: "/tmp/config.json".into(),
            grpc_enabled: false,
            grpc_address: "127.0.0.1:62789".into(),
            firewall_auto_open: false,
            public_base_url: "http://127.0.0.1:18080".into(),
            subscription_host: "203.0.113.10".into(),
            enforcement_interval_seconds: 30,
            adaptive_routing_enabled: false,
            ..Settings::default()
        };
        let mut inbound = Inbound {
            id: 1,
            tag: "tuic-in".into(),
            listen: "::".into(),
            port: 443,
            protocol: "tuic".into(),
            enabled: true,
            transport: "quic".into(),
            settings: "{}".into(),
            stream_settings: r#"{
              "network": "tcp",
              "security": "tls",
              "tlsSettings": {
                "serverName": "www.microsoft.com",
                "certificateFile": "/etc/blackwire/certs/tuic.crt"
              }
            }"#
            .into(),
            sniffing: String::new(),
            limits: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let user = ManagedUser {
            id: 1,
            inbound_id: 1,
            email: "tuic@example.local".into(),
            uuid: "ccdc43c9-5fd4-4d60-a363-17071e7a3f20".into(),
            flow: String::new(),
            credential: json!({"password": "secret"}),
            note: String::new(),
            enabled: true,
            traffic_limit_bytes: None,
            expiry_at: None,
            upload_bytes: 0,
            download_bytes: 0,
            sub_token: "token".into(),
            enforcement_status: "active".into(),
            created_at: String::new(),
            updated_at: String::new(),
        };

        let link = tuic_link(&settings, &inbound, &user).unwrap();
        assert_eq!(
            link,
            "tuic://ccdc43c9-5fd4-4d60-a363-17071e7a3f20:secret@203.0.113.10:443?uuid=ccdc43c9-5fd4-4d60-a363-17071e7a3f20&insecure=1&sni=www.microsoft.com&alpn=h3#tuic%40example.local"
        );
        assert_link_omits_mux_params(&link);

        inbound.stream_settings = r#"{
          "network": "tcp",
          "security": "tls",
          "tlsSettings": {
            "serverName": "www.microsoft.com",
            "certificateFile": "/etc/blackwire/certs/tuic.crt",
            "allowInsecure": true
          }
        }"#
        .into();
        let link = tuic_link(&settings, &inbound, &user).unwrap();
        assert_eq!(
            link,
            "tuic://ccdc43c9-5fd4-4d60-a363-17071e7a3f20:secret@203.0.113.10:443?uuid=ccdc43c9-5fd4-4d60-a363-17071e7a3f20&insecure=1&sni=www.microsoft.com&alpn=h3#tuic%40example.local"
        );
        assert_link_omits_mux_params(&link);
    }
}
