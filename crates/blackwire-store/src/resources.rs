use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::{ActivationClass, ActivationState, Database, MutationResult, StoreError, StoreResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundRecord {
    pub id: i64,
    pub tag: String,
    pub listen: String,
    pub port: u16,
    pub protocol: String,
    pub enabled: bool,
    pub transport: String,
    pub security: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboundRecord {
    pub id: i64,
    pub tag: String,
    pub protocol: String,
    pub enabled: bool,
    pub address: Option<String>,
    pub port: Option<u16>,
    pub transport: String,
    pub security: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserRecord {
    pub id: i64,
    pub inbound_id: i64,
    pub email: String,
    pub enabled: bool,
    pub flow: String,
    pub note: String,
    pub traffic_limit_bytes: Option<i64>,
    pub expiry_at: Option<DateTime<Utc>>,
    pub subscription_token: String,
    pub credential_kind: String,
    pub uuid: Option<String>,
    pub method: Option<String>,
    pub upload_bytes: u64,
    pub download_bytes: u64,
    pub enforcement_status: String,
}

#[derive(Debug, Clone)]
pub struct SubscriptionRecord {
    pub email: String,
    pub enabled: bool,
    pub expiry_at: Option<DateTime<Utc>>,
    pub uuid: Option<String>,
    pub password: Option<String>,
    pub method: Option<String>,
    pub auth: Option<String>,
    pub flow: String,
    pub protocol: String,
    pub port: u16,
    pub transport: String,
    pub security: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundWrite {
    pub id: Option<i64>,
    pub tag: String,
    pub listen: String,
    pub port: u16,
    pub protocol: String,
    pub enabled: bool,
    pub transport: String,
    pub security: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboundWrite {
    pub id: Option<i64>,
    pub tag: String,
    pub protocol: String,
    pub enabled: bool,
    pub address: Option<String>,
    pub port: Option<u16>,
    pub transport: String,
    pub security: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserWrite {
    pub id: Option<i64>,
    pub inbound_id: i64,
    pub email: String,
    pub enabled: bool,
    pub flow: String,
    pub note: String,
    pub traffic_limit_bytes: Option<i64>,
    pub expiry_at: Option<DateTime<Utc>>,
    pub subscription_token: String,
    pub credential_kind: String,
    pub uuid: Option<String>,
    pub password: Option<String>,
    pub method: Option<String>,
    pub auth: Option<String>,
}

impl Database {
    pub async fn subscription_by_token(
        &self,
        token: &str,
    ) -> StoreResult<Option<SubscriptionRecord>> {
        let revision = self.state().await?.desired_revision;
        let row = sqlx::query(
            "SELECT u.email,u.enabled,u.expiry_at,u.flow,c.uuid_value,c.password_value,c.method,c.auth_value,i.protocol,i.listen_port,COALESCE(s.network,'tcp') network,COALESCE(s.security,'none') security FROM users u JOIN user_credentials c ON c.revision_id=u.revision_id AND c.user_id=u.user_id JOIN inbounds i ON i.revision_id=u.revision_id AND i.inbound_id=u.inbound_id LEFT JOIN stream_settings s ON s.revision_id=i.revision_id AND s.endpoint_kind='inbound' AND s.endpoint_id=i.inbound_id LEFT JOIN user_traffic t ON t.user_id=u.user_id WHERE u.revision_id=? AND u.subscription_token=? AND u.enabled=TRUE AND i.enabled=TRUE AND (u.traffic_limit_bytes IS NULL OR COALESCE(t.upload_bytes,0)+COALESCE(t.download_bytes,0)<u.traffic_limit_bytes) LIMIT 1",
        )
        .bind(revision)
        .bind(token)
        .fetch_optional(self.pool())
        .await?;
        row.map(|row| {
            let password = row
                .try_get::<Option<Vec<u8>>, _>("password_value")?
                .map(String::from_utf8)
                .transpose()
                .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
            let auth = row
                .try_get::<Option<Vec<u8>>, _>("auth_value")?
                .map(String::from_utf8)
                .transpose()
                .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
            Ok::<SubscriptionRecord, sqlx::Error>(SubscriptionRecord {
                email: row.try_get("email")?,
                enabled: row.try_get("enabled")?,
                expiry_at: row.try_get("expiry_at")?,
                uuid: row.try_get("uuid_value")?,
                password,
                method: row.try_get("method")?,
                auth,
                flow: row.try_get("flow")?,
                protocol: row.try_get("protocol")?,
                port: u16::try_from(row.try_get::<u32, _>("listen_port")?)
                    .map_err(|error| sqlx::Error::Decode(Box::new(error)))?,
                transport: row.try_get("network")?,
                security: row.try_get("security")?,
            })
        })
        .transpose()
        .map_err(Into::into)
    }

    pub async fn list_inbounds(&self, revision: i64) -> StoreResult<Vec<InboundRecord>> {
        let rows = sqlx::query(
            "SELECT i.inbound_id, i.tag, i.listen_address, i.listen_port, i.protocol, i.enabled, COALESCE(s.network, 'tcp') network, COALESCE(s.security, 'none') security FROM inbounds i LEFT JOIN stream_settings s ON s.revision_id=i.revision_id AND s.endpoint_kind='inbound' AND s.endpoint_id=i.inbound_id WHERE i.revision_id=? ORDER BY i.position",
        )
        .bind(revision)
        .fetch_all(self.pool())
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(InboundRecord {
                    id: row.try_get("inbound_id")?,
                    tag: row.try_get("tag")?,
                    listen: row.try_get("listen_address")?,
                    port: u16::try_from(row.try_get::<u32, _>("listen_port")?)
                        .map_err(|error| sqlx::Error::Decode(Box::new(error)))?,
                    protocol: row.try_get("protocol")?,
                    enabled: row.try_get("enabled")?,
                    transport: row.try_get("network")?,
                    security: row.try_get("security")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    pub async fn list_outbounds(&self, revision: i64) -> StoreResult<Vec<OutboundRecord>> {
        let rows = sqlx::query(
            "SELECT o.outbound_id, o.tag, o.protocol, o.enabled, o.server_address, o.server_port, COALESCE(s.network, 'tcp') network, COALESCE(s.security, 'none') security FROM outbounds o LEFT JOIN stream_settings s ON s.revision_id=o.revision_id AND s.endpoint_kind='outbound' AND s.endpoint_id=o.outbound_id WHERE o.revision_id=? ORDER BY o.position",
        )
        .bind(revision)
        .fetch_all(self.pool())
        .await?;
        rows.into_iter()
            .map(|row| {
                let port = row.try_get::<Option<u32>, _>("server_port")?;
                Ok(OutboundRecord {
                    id: row.try_get("outbound_id")?,
                    tag: row.try_get("tag")?,
                    protocol: row.try_get("protocol")?,
                    enabled: row.try_get("enabled")?,
                    address: row.try_get("server_address")?,
                    port: port
                        .map(u16::try_from)
                        .transpose()
                        .map_err(|error| sqlx::Error::Decode(Box::new(error)))?,
                    transport: row.try_get("network")?,
                    security: row.try_get("security")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    pub async fn list_users(&self, revision: i64) -> StoreResult<Vec<UserRecord>> {
        let rows = sqlx::query(
            "SELECT u.user_id, u.inbound_id, u.email, u.enabled, u.flow, u.note, u.traffic_limit_bytes, u.expiry_at, u.subscription_token, c.credential_kind, c.uuid_value, c.method, CAST(COALESCE(t.upload_bytes,0) AS UNSIGNED) upload_bytes, CAST(COALESCE(t.download_bytes,0) AS UNSIGNED) download_bytes, COALESCE(e.status,CASE WHEN NOT u.enabled THEN 'disabled' WHEN u.expiry_at IS NOT NULL AND u.expiry_at<=UTC_TIMESTAMP(6) THEN 'expired' ELSE 'current' END) enforcement_status FROM users u JOIN user_credentials c ON c.revision_id=u.revision_id AND c.user_id=u.user_id LEFT JOIN user_traffic t ON t.user_id=u.user_id LEFT JOIN enforcement_state e ON e.user_id=u.user_id WHERE u.revision_id=? ORDER BY u.user_id",
        )
        .bind(revision)
        .fetch_all(self.pool())
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(UserRecord {
                    id: row.try_get("user_id")?,
                    inbound_id: row.try_get("inbound_id")?,
                    email: row.try_get("email")?,
                    enabled: row.try_get("enabled")?,
                    flow: row.try_get("flow")?,
                    note: row.try_get("note")?,
                    traffic_limit_bytes: row.try_get("traffic_limit_bytes")?,
                    expiry_at: row.try_get("expiry_at")?,
                    subscription_token: row.try_get("subscription_token")?,
                    credential_kind: row.try_get("credential_kind")?,
                    uuid: row.try_get("uuid_value")?,
                    method: row.try_get("method")?,
                    upload_bytes: row.try_get("upload_bytes")?,
                    download_bytes: row.try_get("download_bytes")?,
                    enforcement_status: row.try_get("enforcement_status")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    pub async fn save_inbound(
        &self,
        actor: &str,
        expected_revision: i64,
        input: InboundWrite,
    ) -> StoreResult<MutationResult> {
        validate_inbound(&input)?;
        let state = self.state().await?;
        let (mut tx, revision) = self
            .fork_revision(
                expected_revision,
                actor,
                "Save inbound",
                ActivationClass::ListenerHandover,
            )
            .await?;
        let id = input.id.unwrap_or(
            sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(inbound_id) FROM inbounds")
                .fetch_one(&mut *tx)
                .await?
                .unwrap_or(0)
                + 1,
        );
        let position = if input.id.is_some() {
            0
        } else {
            sqlx::query_scalar::<_, Option<u32>>(
                "SELECT MAX(position) FROM inbounds WHERE revision_id=?",
            )
            .bind(revision)
            .fetch_one(&mut *tx)
            .await?
            .map_or(0, |value| value + 1)
        };
        sqlx::query("INSERT INTO inbounds (revision_id,inbound_id,tag,listen_address,listen_port,protocol,enabled,position) VALUES (?,?,?,?,?,?,?,?) ON DUPLICATE KEY UPDATE tag=VALUES(tag),listen_address=VALUES(listen_address),listen_port=VALUES(listen_port),protocol=VALUES(protocol),enabled=VALUES(enabled)")
            .bind(revision).bind(id).bind(input.tag).bind(input.listen).bind(input.port).bind(input.protocol).bind(input.enabled).bind(position)
            .execute(&mut *tx).await?;
        sqlx::query("DELETE FROM stream_settings WHERE revision_id=? AND endpoint_kind='inbound' AND endpoint_id=?").bind(revision).bind(id).execute(&mut *tx).await?;
        if input.transport != "tcp" || input.security != "none" {
            sqlx::query("INSERT INTO stream_settings (revision_id,endpoint_kind,endpoint_id,network,security) VALUES (?,'inbound',?,?,?)")
                .bind(revision).bind(id).bind(input.transport).bind(input.security).execute(&mut *tx).await?;
        }
        Database::publish_revision(&mut tx, revision, ActivationClass::ListenerHandover).await?;
        tx.commit().await?;
        Ok(mutation_result(
            &state,
            revision,
            ActivationClass::ListenerHandover,
            "Inbound saved",
        ))
    }

    pub async fn delete_inbound(
        &self,
        actor: &str,
        expected_revision: i64,
        id: i64,
    ) -> StoreResult<MutationResult> {
        let state = self.state().await?;
        let (mut tx, revision) = self
            .fork_revision(
                expected_revision,
                actor,
                "Delete inbound",
                ActivationClass::ListenerHandover,
            )
            .await?;
        sqlx::query("DELETE FROM inbounds WHERE revision_id=? AND inbound_id=?")
            .bind(revision)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        Database::publish_revision(&mut tx, revision, ActivationClass::ListenerHandover).await?;
        tx.commit().await?;
        Ok(mutation_result(
            &state,
            revision,
            ActivationClass::ListenerHandover,
            "Inbound deleted",
        ))
    }

    pub async fn save_outbound(
        &self,
        actor: &str,
        expected_revision: i64,
        input: OutboundWrite,
    ) -> StoreResult<MutationResult> {
        validate_outbound(&input)?;
        let state = self.state().await?;
        let class = ActivationClass::MaintenanceRequired;
        let (mut tx, revision) = self
            .fork_revision(expected_revision, actor, "Save outbound", class)
            .await?;
        let id = input.id.unwrap_or(
            sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(outbound_id) FROM outbounds")
                .fetch_one(&mut *tx)
                .await?
                .unwrap_or(0)
                + 1,
        );
        let position = if input.id.is_some() {
            0
        } else {
            sqlx::query_scalar::<_, Option<u32>>(
                "SELECT MAX(position) FROM outbounds WHERE revision_id=?",
            )
            .bind(revision)
            .fetch_one(&mut *tx)
            .await?
            .map_or(0, |value| value + 1)
        };
        sqlx::query("INSERT INTO outbounds (revision_id,outbound_id,tag,protocol,enabled,position,server_address,server_port,domain_strategy,deny_loopback,reject_ipv6_literal) VALUES (?,?,?,?,?,?,?,?,NULL,NULL,NULL) ON DUPLICATE KEY UPDATE tag=VALUES(tag),protocol=VALUES(protocol),enabled=VALUES(enabled),server_address=VALUES(server_address),server_port=VALUES(server_port)")
            .bind(revision).bind(id).bind(input.tag).bind(input.protocol).bind(input.enabled).bind(position).bind(input.address).bind(input.port)
            .execute(&mut *tx).await?;
        sqlx::query("DELETE FROM stream_settings WHERE revision_id=? AND endpoint_kind='outbound' AND endpoint_id=?").bind(revision).bind(id).execute(&mut *tx).await?;
        if input.transport != "tcp" || input.security != "none" {
            sqlx::query("INSERT INTO stream_settings (revision_id,endpoint_kind,endpoint_id,network,security) VALUES (?,'outbound',?,?,?)")
                .bind(revision).bind(id).bind(input.transport).bind(input.security).execute(&mut *tx).await?;
        }
        Database::publish_revision(&mut tx, revision, class).await?;
        tx.commit().await?;
        Ok(mutation_result(
            &state,
            revision,
            class,
            "Outbound saved; maintenance activation required",
        ))
    }

    pub async fn delete_outbound(
        &self,
        actor: &str,
        expected_revision: i64,
        id: i64,
    ) -> StoreResult<MutationResult> {
        let state = self.state().await?;
        let class = ActivationClass::MaintenanceRequired;
        let (mut tx, revision) = self
            .fork_revision(expected_revision, actor, "Delete outbound", class)
            .await?;
        sqlx::query("DELETE FROM outbounds WHERE revision_id=? AND outbound_id=?")
            .bind(revision)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        Database::publish_revision(&mut tx, revision, class).await?;
        tx.commit().await?;
        Ok(mutation_result(
            &state,
            revision,
            class,
            "Outbound deleted; maintenance activation required",
        ))
    }

    pub async fn save_user(
        &self,
        actor: &str,
        expected_revision: i64,
        input: UserWrite,
    ) -> StoreResult<MutationResult> {
        if input.email.trim().is_empty() || input.subscription_token.trim().is_empty() {
            return Err(StoreError::InvalidConfiguration(
                "user email and subscription token are required".into(),
            ));
        }
        let state = self.state().await?;
        let class = ActivationClass::HotSwap;
        let (mut tx, revision) = self
            .fork_revision(expected_revision, actor, "Save user", class)
            .await?;
        let id = input.id.unwrap_or(
            sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(user_id) FROM users")
                .fetch_one(&mut *tx)
                .await?
                .unwrap_or(0)
                + 1,
        );
        sqlx::query("INSERT INTO users (revision_id,user_id,inbound_id,email,enabled,flow,note,traffic_limit_bytes,expiry_at,subscription_token) VALUES (?,?,?,?,?,?,?,?,?,?) ON DUPLICATE KEY UPDATE inbound_id=VALUES(inbound_id),email=VALUES(email),enabled=VALUES(enabled),flow=VALUES(flow),note=VALUES(note),traffic_limit_bytes=VALUES(traffic_limit_bytes),expiry_at=VALUES(expiry_at),subscription_token=VALUES(subscription_token)")
            .bind(revision).bind(id).bind(input.inbound_id).bind(input.email).bind(input.enabled).bind(input.flow).bind(input.note).bind(input.traffic_limit_bytes).bind(input.expiry_at).bind(input.subscription_token)
            .execute(&mut *tx).await?;
        sqlx::query("INSERT INTO user_credentials (revision_id,user_id,credential_kind,uuid_value,password_value,method,auth_value) VALUES (?,?,?,?,?,?,?) ON DUPLICATE KEY UPDATE credential_kind=VALUES(credential_kind),uuid_value=COALESCE(VALUES(uuid_value),uuid_value),password_value=COALESCE(VALUES(password_value),password_value),method=COALESCE(VALUES(method),method),auth_value=COALESCE(VALUES(auth_value),auth_value)")
            .bind(revision).bind(id).bind(input.credential_kind).bind(input.uuid).bind(input.password.map(String::into_bytes)).bind(input.method).bind(input.auth.map(String::into_bytes))
            .execute(&mut *tx).await?;
        Database::publish_revision(&mut tx, revision, class).await?;
        tx.commit().await?;
        Ok(mutation_result(&state, revision, class, "User saved"))
    }

    pub async fn delete_user(
        &self,
        actor: &str,
        expected_revision: i64,
        id: i64,
    ) -> StoreResult<MutationResult> {
        let state = self.state().await?;
        let class = ActivationClass::HotSwap;
        let (mut tx, revision) = self
            .fork_revision(expected_revision, actor, "Delete user", class)
            .await?;
        sqlx::query("DELETE FROM users WHERE revision_id=? AND user_id=?")
            .bind(revision)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        Database::publish_revision(&mut tx, revision, class).await?;
        tx.commit().await?;
        Ok(mutation_result(&state, revision, class, "User deleted"))
    }
}

fn validate_inbound(input: &InboundWrite) -> StoreResult<()> {
    if input.tag.trim().is_empty()
        || input.listen.parse::<std::net::IpAddr>().is_err()
        || input.port == 0
    {
        return Err(StoreError::InvalidConfiguration(
            "inbound tag, IP listen address, and non-zero port are required".into(),
        ));
    }
    if !matches!(
        input.protocol.as_str(),
        "vless" | "vmess" | "trojan" | "shadowsocks" | "hysteria2" | "tuic" | "socks" | "http"
    ) {
        return Err(StoreError::InvalidConfiguration(format!(
            "unsupported inbound protocol '{}'",
            input.protocol
        )));
    }
    validate_transport_security(&input.transport, &input.security)
}

fn validate_outbound(input: &OutboundWrite) -> StoreResult<()> {
    if input.tag.trim().is_empty() {
        return Err(StoreError::InvalidConfiguration(
            "outbound tag is required".into(),
        ));
    }
    if !matches!(
        input.protocol.as_str(),
        "freedom" | "vless" | "vmess" | "trojan" | "shadowsocks" | "hysteria2" | "tuic"
    ) {
        return Err(StoreError::InvalidConfiguration(format!(
            "unsupported outbound protocol '{}'",
            input.protocol
        )));
    }
    if input.protocol != "freedom"
        && (input.address.as_deref().is_none_or(str::is_empty)
            || input.port.is_none_or(|port| port == 0))
    {
        return Err(StoreError::InvalidConfiguration(
            "non-freedom outbounds require a server address and port".into(),
        ));
    }
    validate_transport_security(&input.transport, &input.security)
}

fn validate_transport_security(transport: &str, security: &str) -> StoreResult<()> {
    if !matches!(
        transport,
        "tcp" | "ws" | "grpc" | "httpupgrade" | "splithttp" | "quic" | "kcp"
    ) {
        return Err(StoreError::InvalidConfiguration(format!(
            "unsupported transport '{transport}'"
        )));
    }
    if !matches!(security, "none" | "tls" | "reality" | "shadowtls") {
        return Err(StoreError::InvalidConfiguration(format!(
            "unsupported security mode '{security}'"
        )));
    }
    Ok(())
}

fn mutation_result(
    state: &crate::ConfigurationState,
    revision: i64,
    class: ActivationClass,
    message: &str,
) -> MutationResult {
    MutationResult {
        revision,
        parent_revision: state.desired_revision,
        active_revision: state.active_revision,
        state: if class == ActivationClass::MaintenanceRequired {
            ActivationState::PendingMaintenance
        } else {
            ActivationState::Activating
        },
        activation_class: class,
        message: message.into(),
    }
}
