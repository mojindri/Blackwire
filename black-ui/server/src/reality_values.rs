use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use rand::RngExt;
use serde_json::Value;
use x25519_dalek::{PublicKey, StaticSecret};

use crate::models::{RealityClientValues, RealityGeneratedValues, TlsServerValues};

pub fn load(config_path: &str) -> Result<Vec<RealityClientValues>> {
    let mut values = Vec::new();
    let mut seen = HashSet::new();

    if let Some(value) = load_client_info()? {
        push_unique(&mut values, &mut seen, value);
    }

    for path in config_candidates(config_path) {
        if let Ok(config_values) = load_config_values(&path) {
            for value in config_values {
                push_unique(&mut values, &mut seen, value);
            }
        }
    }

    Ok(values)
}

pub fn load_tls(config_path: &str) -> Vec<TlsServerValues> {
    let mut values = Vec::new();
    let mut seen = HashSet::new();

    for path in config_candidates(config_path) {
        if let Ok(config_values) = load_tls_config_values(&path) {
            for value in config_values {
                let key = (
                    value.tag.clone(),
                    value.server_name.clone(),
                    value.certificate_file.clone(),
                    value.key_file.clone(),
                );
                if seen.insert(key) {
                    values.push(value);
                }
            }
        }
    }

    values
}

pub fn generate() -> RealityGeneratedValues {
    let secret = StaticSecret::random();
    let public = PublicKey::from(&secret);
    let mut short_id = [0u8; 8];
    rand::rng().fill(&mut short_id);

    RealityGeneratedValues {
        private_key: hex::encode(secret.to_bytes()),
        public_key: hex::encode(public.as_bytes()),
        short_id: hex::encode(short_id),
    }
}

fn push_unique(
    values: &mut Vec<RealityClientValues>,
    seen: &mut HashSet<(String, String, String)>,
    value: RealityClientValues,
) {
    let key = (
        value.public_key.clone(),
        value.short_id.clone(),
        value.server_name.clone(),
    );
    if seen.insert(key) {
        values.push(value);
    }
}

fn load_client_info() -> Result<Option<RealityClientValues>> {
    let path = std::env::var("BLACKWIRE_CLIENT_INFO_PATH")
        .unwrap_or_else(|_| "/etc/blackwire/client-info.txt".into());
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(None);
    };
    Ok(parse_client_info(&text, path.as_str()))
}

fn config_candidates(config_path: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(path) = std::env::var("BLACKWIRE_ACTIVE_CONFIG") {
        paths.push(PathBuf::from(path));
    }
    paths.push(PathBuf::from("/etc/blackwire/config.json"));
    if !config_path.trim().is_empty() {
        paths.push(PathBuf::from(config_path));
    }
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|p| seen.insert(p.clone()))
        .collect()
}

fn load_config_values(path: &Path) -> Result<Vec<RealityClientValues>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let value: Value =
        serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(parse_config(&value, &path.display().to_string()))
}

fn load_tls_config_values(path: &Path) -> Result<Vec<TlsServerValues>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let value: Value =
        serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(parse_tls_config(&value, &path.display().to_string()))
}

fn parse_client_info(text: &str, source: &str) -> Option<RealityClientValues> {
    let address = labeled_value(text, "Address");
    let port = labeled_value(text, "Port").and_then(|v| v.parse().ok());
    let uuid = labeled_value(text, "UUID");
    let public_key = labeled_value(text, "REALITY public key")?;
    let short_id = labeled_value(text, "REALITY short ID")?;
    let server_name = labeled_value(text, "REALITY server name")?;

    Some(RealityClientValues {
        source: source.into(),
        tag: None,
        address,
        port,
        uuid,
        private_key: None,
        public_key,
        short_id,
        server_name,
    })
}

fn labeled_value(text: &str, label: &str) -> Option<String> {
    let prefix = format!("{label}:");
    text.lines()
        .find_map(|line| line.strip_prefix(&prefix).map(str::trim))
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn parse_config(value: &Value, source: &str) -> Vec<RealityClientValues> {
    value
        .get("inbounds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|inbound| parse_config_inbound(inbound, source))
        .collect()
}

fn parse_tls_config(value: &Value, source: &str) -> Vec<TlsServerValues> {
    value
        .get("inbounds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|inbound| parse_tls_config_inbound(inbound, source))
        .collect()
}

fn parse_config_inbound(inbound: &Value, source: &str) -> Option<RealityClientValues> {
    let stream = inbound
        .get("streamSettings")
        .or_else(|| inbound.get("stream_settings"))?;
    if stream.get("security").and_then(Value::as_str) != Some("reality") {
        return None;
    }
    let reality = stream.get("realitySettings")?;
    let private_key = reality
        .get("privateKey")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);
    let public_key = reality
        .get("publicKey")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| private_key.as_deref().and_then(public_key_from_private_hex))?;
    let short_id = reality
        .get("shortId")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| first_string(reality.get("shortIds")))?;
    let server_name = reality
        .get("serverName")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| first_string(reality.get("serverNames")))?;

    Some(RealityClientValues {
        source: source.into(),
        tag: inbound
            .get("tag")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        address: None,
        port: inbound
            .get("port")
            .and_then(Value::as_u64)
            .and_then(|port| u16::try_from(port).ok()),
        uuid: first_client_uuid(inbound),
        private_key,
        public_key,
        short_id,
        server_name,
    })
}

fn parse_tls_config_inbound(inbound: &Value, source: &str) -> Option<TlsServerValues> {
    let stream = inbound
        .get("streamSettings")
        .or_else(|| inbound.get("stream_settings"))?;
    if stream.get("security").and_then(Value::as_str) != Some("tls") {
        return None;
    }
    let tls = stream.get("tlsSettings")?;
    let certificate_file = tls
        .get("certificateFile")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);
    let key_file = tls
        .get("keyFile")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);
    let server_name = tls
        .get("serverName")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);
    if certificate_file.is_none() && key_file.is_none() && server_name.is_none() {
        return None;
    }

    Some(TlsServerValues {
        source: source.into(),
        tag: inbound
            .get("tag")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        port: inbound
            .get("port")
            .and_then(Value::as_u64)
            .and_then(|port| u16::try_from(port).ok()),
        server_name,
        alpn: string_list(tls.get("alpn")),
        certificate_file,
        key_file,
        allow_insecure: tls_bool(tls, "allowInsecure")
            || tls_bool(tls, "insecure")
            || tls_bool(tls, "skipCertVerify"),
    })
}

fn first_string(value: Option<&Value>) -> Option<String> {
    value?
        .as_array()?
        .iter()
        .find_map(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect(),
        Some(Value::String(value)) if !value.is_empty() => value
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn tls_bool(tls: &Value, key: &str) -> bool {
    tls.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn first_client_uuid(inbound: &Value) -> Option<String> {
    inbound
        .get("settings")?
        .get("clients")?
        .as_array()?
        .iter()
        .find_map(|client| client.get("id").and_then(Value::as_str))
        .filter(|id| !id.is_empty())
        .map(ToString::to_string)
}

fn public_key_from_private_hex(private_key: &str) -> Option<String> {
    let bytes = hex::decode(private_key.trim()).ok()?;
    let key: [u8; 32] = bytes.try_into().ok()?;
    let secret = StaticSecret::from(key);
    let public = PublicKey::from(&secret);
    Some(hex::encode(public.as_bytes()))
}

#[allow(dead_code)]
fn public_key_from_private_base64(private_key: &str) -> Option<String> {
    let bytes = general_purpose::URL_SAFE_NO_PAD
        .decode(private_key.trim())
        .ok()?;
    let key: [u8; 32] = bytes.try_into().ok()?;
    let secret = StaticSecret::from(key);
    let public = PublicKey::from(&secret);
    Some(hex::encode(public.as_bytes()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_installer_client_info() {
        let parsed = parse_client_info(
            r#"Generated VLESS REALITY server config

Address: 203.0.113.10
Port: 443
UUID: 11111111-1111-4111-8111-111111111111
Network: tcp
Security: reality
REALITY public key: 250d08c08b9f82143595ea1734015b612ef6bc314c18955b5517bd868ee40b10
REALITY short ID: 0ca1ce9df12b31e5
REALITY server name: www.microsoft.com
"#,
            "/tmp/client-info.txt",
        )
        .expect("client info should parse");

        assert_eq!(parsed.address.as_deref(), Some("203.0.113.10"));
        assert_eq!(parsed.port, Some(443));
        assert_eq!(
            parsed.uuid.as_deref(),
            Some("11111111-1111-4111-8111-111111111111")
        );
        assert_eq!(parsed.private_key, None);
        assert_eq!(
            parsed.public_key,
            "250d08c08b9f82143595ea1734015b612ef6bc314c18955b5517bd868ee40b10"
        );
        assert_eq!(parsed.short_id, "0ca1ce9df12b31e5");
        assert_eq!(parsed.server_name, "www.microsoft.com");
    }

    #[test]
    fn derives_public_key_from_server_private_key() {
        let values = parse_config(
            &json!({
                "inbounds": [{
                    "tag": "vless-reality-in",
                    "port": 443,
                    "settings": {
                        "clients": [{ "id": "f645bc3c-5e00-4122-8c94-9beaa54e2022" }]
                    },
                    "streamSettings": {
                        "security": "reality",
                        "realitySettings": {
                            "privateKey": "769aa4a053f2c8af7a27bb1d79fc0067f39b6c1ce6743543bb3f7584aa68223c",
                            "shortIds": ["0ca1ce9df12b31e5"],
                            "serverNames": ["www.microsoft.com"]
                        }
                    }
                }]
            }),
            "/etc/blackwire/config.json",
        );

        assert_eq!(values.len(), 1);
        assert_eq!(values[0].tag.as_deref(), Some("vless-reality-in"));
        assert_eq!(values[0].port, Some(443));
        assert_eq!(
            values[0].uuid.as_deref(),
            Some("f645bc3c-5e00-4122-8c94-9beaa54e2022")
        );
        assert_eq!(
            values[0].private_key.as_deref(),
            Some("769aa4a053f2c8af7a27bb1d79fc0067f39b6c1ce6743543bb3f7584aa68223c")
        );
        assert_eq!(
            values[0].public_key,
            "250d08c08b9f82143595ea1734015b612ef6bc314c18955b5517bd868ee40b10"
        );
        assert_eq!(values[0].short_id, "0ca1ce9df12b31e5");
        assert_eq!(values[0].server_name, "www.microsoft.com");
    }

    #[test]
    fn parses_tls_server_values_from_config() {
        let values = parse_tls_config(
            &json!({
                "inbounds": [{
                    "tag": "vless-tls-in",
                    "port": 443,
                    "streamSettings": {
                        "security": "tls",
                        "tlsSettings": {
                            "serverName": "proxy.example.com",
                            "alpn": ["h2", "http/1.1"],
                            "certificateFile": "/etc/letsencrypt/live/proxy/fullchain.pem",
                            "keyFile": "/etc/letsencrypt/live/proxy/privkey.pem",
                            "allowInsecure": false
                        }
                    }
                }]
            }),
            "/etc/blackwire/config.json",
        );

        assert_eq!(values.len(), 1);
        assert_eq!(values[0].tag.as_deref(), Some("vless-tls-in"));
        assert_eq!(values[0].port, Some(443));
        assert_eq!(values[0].server_name.as_deref(), Some("proxy.example.com"));
        assert_eq!(values[0].alpn, vec!["h2", "http/1.1"]);
        assert_eq!(
            values[0].certificate_file.as_deref(),
            Some("/etc/letsencrypt/live/proxy/fullchain.pem")
        );
        assert_eq!(
            values[0].key_file.as_deref(),
            Some("/etc/letsencrypt/live/proxy/privkey.pem")
        );
        assert!(!values[0].allow_insecure);
    }

    #[test]
    fn generates_matching_reality_values() {
        let generated = generate();

        assert_eq!(generated.private_key.len(), 64);
        assert_eq!(generated.public_key.len(), 64);
        assert_eq!(generated.short_id.len(), 16);
        assert!(hex::decode(&generated.private_key).is_ok());
        assert!(hex::decode(&generated.public_key).is_ok());
        assert!(hex::decode(&generated.short_id).is_ok());
        assert_eq!(
            public_key_from_private_hex(&generated.private_key).as_deref(),
            Some(generated.public_key.as_str())
        );
    }
}
