use blackwire_store::{
    ActivationClass, ActivationState, InboundRecord, OutboundRecord, PanelSettings, UserRecord,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub firewall_auto_open: bool,
    pub public_base_url: String,
    pub subscription_host: String,
    pub enforcement_interval_seconds: u64,
    #[serde(default)]
    pub adaptive_routing_enabled: bool,
    #[serde(default = "default_adaptive_tuning_mode")]
    pub adaptive_tuning_mode: String,
    #[serde(default = "default_adaptive_tuning_interval_seconds")]
    pub adaptive_tuning_interval_seconds: u64,
    #[serde(default = "default_adaptive_tuning_cooldown_seconds")]
    pub adaptive_tuning_cooldown_seconds: u64,
    #[serde(default = "default_adaptive_tuning_max_hysteria2_mbps")]
    pub adaptive_tuning_max_hysteria2_mbps: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            firewall_auto_open: false,
            public_base_url: "http://127.0.0.1:18080".into(),
            subscription_host: "127.0.0.1".into(),
            enforcement_interval_seconds: 30,
            adaptive_routing_enabled: false,
            adaptive_tuning_mode: default_adaptive_tuning_mode(),
            adaptive_tuning_interval_seconds: default_adaptive_tuning_interval_seconds(),
            adaptive_tuning_cooldown_seconds: default_adaptive_tuning_cooldown_seconds(),
            adaptive_tuning_max_hysteria2_mbps: default_adaptive_tuning_max_hysteria2_mbps(),
        }
    }
}

fn default_adaptive_tuning_mode() -> String {
    "off".into()
}

fn default_adaptive_tuning_interval_seconds() -> u64 {
    600
}

fn default_adaptive_tuning_cooldown_seconds() -> u64 {
    600
}

fn default_adaptive_tuning_max_hysteria2_mbps() -> u64 {
    1000
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RealityClientValues {
    pub source: String,
    pub tag: Option<String>,
    pub address: Option<String>,
    pub port: Option<u16>,
    pub uuid: Option<String>,
    pub private_key: Option<String>,
    pub public_key: String,
    pub short_id: String,
    pub server_name: String,
    pub dest: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RealityGeneratedValues {
    pub private_key: String,
    pub public_key: String,
    pub short_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TlsServerValues {
    pub source: String,
    pub tag: Option<String>,
    pub port: Option<u16>,
    pub server_name: Option<String>,
    pub alpn: Vec<String>,
    pub certificate_file: Option<String>,
    pub key_file: Option<String>,
    pub allow_insecure: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TlsSelfSignedInput {
    pub server_name: String,
    #[serde(default = "default_tls_self_signed_days")]
    pub days: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TlsSelfSignedResult {
    pub server_name: String,
    pub certificate_file: String,
    pub key_file: String,
    pub days: u16,
}

fn default_tls_self_signed_days() -> i64 {
    365
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub setup_required: bool,
    pub database_connected: bool,
    pub schema_version: i64,
    pub desired_revision: i64,
    pub active_revision: Option<i64>,
    pub pending_maintenance_revision: Option<i64>,
    pub activation_state: ActivationState,
    pub last_activation_error: Option<String>,
    pub runtime_reachable: bool,
    pub last_reconciliation: String,
    pub inbounds: usize,
    pub outbounds: usize,
    pub users: usize,
    pub active_users: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Inbound {
    pub id: i64,
    pub tag: String,
    pub listen: String,
    pub port: u16,
    pub protocol: String,
    pub enabled: bool,
    pub transport: String,
    pub security: String,
    pub settings: String,
    pub stream_settings: String,
    pub sniffing: String,
    pub limits: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundInput {
    pub tag: String,
    pub listen: String,
    pub port: u16,
    pub protocol: String,
    pub enabled: bool,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default = "default_security")]
    pub security: String,
    #[serde(default)]
    pub settings: Option<String>,
    #[serde(default)]
    pub stream_settings: Option<String>,
    #[serde(default)]
    pub sniffing: Option<String>,
    #[serde(default)]
    pub limits: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Outbound {
    pub id: i64,
    pub tag: String,
    pub protocol: String,
    pub enabled: bool,
    pub address: Option<String>,
    pub port: Option<u16>,
    pub transport: String,
    pub security: String,
    pub settings: String,
    pub stream_settings: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboundInput {
    pub tag: String,
    pub protocol: String,
    pub enabled: bool,
    pub address: Option<String>,
    pub port: Option<u16>,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default = "default_security")]
    pub security: String,
    #[serde(default)]
    pub settings: Option<String>,
    #[serde(default)]
    pub stream_settings: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ManagedUser {
    pub id: i64,
    pub inbound_id: i64,
    pub email: String,
    pub uuid: String,
    pub flow: String,
    pub credential_kind: String,
    pub credential: serde_json::Map<String, serde_json::Value>,
    pub method: Option<String>,
    pub note: String,
    pub enabled: bool,
    pub traffic_limit_bytes: Option<i64>,
    pub expiry_at: Option<String>,
    pub subscription_token: String,
    pub sub_token: String,
    pub upload_bytes: i64,
    pub download_bytes: i64,
    pub enforcement_status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInput {
    pub inbound_id: i64,
    pub email: String,
    pub uuid: String,
    pub flow: Option<String>,
    pub credential_kind: Option<String>,
    #[serde(default)]
    pub credential: Option<serde_json::Map<String, serde_json::Value>>,
    pub password: Option<String>,
    pub method: Option<String>,
    pub auth: Option<String>,
    pub subscription_token: Option<String>,
    pub note: Option<String>,
    pub enabled: bool,
    pub traffic_limit_bytes: Option<i64>,
    pub expiry_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkUserInput {
    pub user_ids: Vec<i64>,
    pub action: String,
    pub traffic_limit_bytes: Option<i64>,
    pub expiry_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupInput {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginInput {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    pub token: String,
    pub username: String,
}

/// The only panel state needed before authentication. Keep operational status
/// private; the login screen only needs to know whether it should create the
/// first administrator or show the sign-in form.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthBootstrapStatus {
    pub setup_required: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentAdmin {
    pub username: String,
}

#[derive(Debug, Serialize)]
pub struct GeneratedUuid {
    pub uuid: String,
}

#[derive(Debug, Serialize)]
pub struct MaintenanceResult {
    pub revision: i64,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub revision: i64,
    pub parent_revision: i64,
    pub active_revision: Option<i64>,
    pub state: ActivationState,
    pub activation_class: ActivationClass,
    pub message: String,
}

fn default_transport() -> String {
    "tcp".into()
}
fn default_security() -> String {
    "none".into()
}

impl From<PanelSettings> for Settings {
    fn from(value: PanelSettings) -> Self {
        Self {
            firewall_auto_open: value.firewall_auto_open,
            public_base_url: value.public_base_url,
            subscription_host: value.subscription_host,
            enforcement_interval_seconds: value.enforcement_interval_seconds,
            adaptive_routing_enabled: value.adaptive_routing_enabled,
            adaptive_tuning_mode: value.adaptive_tuning_mode,
            adaptive_tuning_interval_seconds: value.adaptive_tuning_interval_seconds,
            adaptive_tuning_cooldown_seconds: value.adaptive_tuning_cooldown_seconds,
            adaptive_tuning_max_hysteria2_mbps: value.adaptive_tuning_max_hysteria2_mbps,
        }
    }
}

impl From<Settings> for PanelSettings {
    fn from(value: Settings) -> Self {
        Self {
            firewall_auto_open: value.firewall_auto_open,
            public_base_url: value.public_base_url,
            subscription_host: value.subscription_host,
            enforcement_interval_seconds: value.enforcement_interval_seconds,
            adaptive_routing_enabled: value.adaptive_routing_enabled,
            adaptive_tuning_mode: value.adaptive_tuning_mode,
            adaptive_tuning_interval_seconds: value.adaptive_tuning_interval_seconds,
            adaptive_tuning_cooldown_seconds: value.adaptive_tuning_cooldown_seconds,
            adaptive_tuning_max_hysteria2_mbps: value.adaptive_tuning_max_hysteria2_mbps,
        }
    }
}

impl From<InboundRecord> for Inbound {
    fn from(value: InboundRecord) -> Self {
        Self {
            id: value.id,
            tag: value.tag,
            listen: value.listen,
            port: value.port,
            protocol: value.protocol,
            enabled: value.enabled,
            transport: value.transport,
            security: value.security,
            settings: "{}".into(),
            stream_settings: "{}".into(),
            sniffing: "{}".into(),
            limits: "{}".into(),
        }
    }
}

impl From<OutboundRecord> for Outbound {
    fn from(value: OutboundRecord) -> Self {
        Self {
            id: value.id,
            tag: value.tag,
            protocol: value.protocol,
            enabled: value.enabled,
            address: value.address,
            port: value.port,
            transport: value.transport,
            security: value.security,
            settings: "{}".into(),
            stream_settings: "{}".into(),
        }
    }
}

impl From<UserRecord> for ManagedUser {
    fn from(value: UserRecord) -> Self {
        let subscription_token = value.subscription_token;
        let mut credential = serde_json::Map::new();
        if let Some(uuid) = value.uuid.as_ref() {
            credential.insert("id".into(), serde_json::Value::String(uuid.clone()));
        }
        if let Some(method) = value.method.as_ref() {
            credential.insert("method".into(), serde_json::Value::String(method.clone()));
        }
        Self {
            id: value.id,
            inbound_id: value.inbound_id,
            email: value.email,
            uuid: value.uuid.unwrap_or_default(),
            flow: value.flow,
            credential_kind: value.credential_kind,
            credential,
            method: value.method,
            note: value.note,
            enabled: value.enabled,
            traffic_limit_bytes: value.traffic_limit_bytes,
            expiry_at: value.expiry_at.map(|time| time.to_rfc3339()),
            sub_token: subscription_token.clone(),
            subscription_token,
            upload_bytes: value.upload_bytes.min(i64::MAX as u64) as i64,
            download_bytes: value.download_bytes.min(i64::MAX as u64) as i64,
            enforcement_status: value.enforcement_status,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficSnapshot {
    pub users: Vec<UserTraffic>,
    pub inbounds: Vec<InboundTraffic>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityMap {
    pub protocols: Vec<CapabilityItem>,
    pub transports: Vec<CapabilityItem>,
    pub security: Vec<CapabilityItem>,
    pub config: Vec<CapabilityItem>,
    pub runtime: Vec<CapabilityItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityItem {
    pub key: &'static str,
    pub label: &'static str,
    pub status: &'static str,
    pub notes: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    pub systemd_available: bool,
    pub active_state: String,
    pub sub_state: String,
    pub logs: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserTraffic {
    pub email: String,
    pub upload_bytes: i64,
    pub download_bytes: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundTraffic {
    pub tag: String,
    pub upload_bytes: i64,
    pub download_bytes: i64,
}
