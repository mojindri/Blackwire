use crate::sqlx;
use blackwire_config::schema::{
    EndpointSettings, InboundLimitsConfig, NetworkType, PaddingBytes, SecurityType, SniffingConfig,
    SplitHttpConfig, StreamSettingsConfig,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{MySql, Row, Transaction};

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
    pub server_name: Option<String>,
    pub reality_public_key: Option<String>,
    pub reality_short_id: Option<String>,
    pub reality_fingerprint: Option<String>,
    pub transport_path: Option<String>,
    pub transport_host: Option<String>,
    pub grpc_service_name: Option<String>,
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
    pub settings: Option<EndpointSettings>,
    pub stream_settings: Option<StreamSettingsConfig>,
    pub sniffing: Option<SniffingConfig>,
    pub limits: Option<InboundLimitsConfig>,
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
    pub settings: Option<EndpointSettings>,
    pub stream_settings: Option<StreamSettingsConfig>,
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
            "SELECT u.email,u.enabled,u.expiry_at,u.flow,c.uuid_value,c.password_value,c.method,c.auth_value,i.protocol,i.listen_port,COALESCE(s.network,'tcp') network,COALESCE(s.security,'none') security,COALESCE(NULLIF(tls.server_name,''),NULLIF(r.server_name,'')) server_name,NULLIF(r.public_key,'') reality_public_key,COALESCE(NULLIF(r.short_id,''),(SELECT NULLIF(rs.short_id,'') FROM reality_short_ids rs WHERE rs.revision_id=i.revision_id AND rs.endpoint_kind='inbound' AND rs.endpoint_id=i.inbound_id ORDER BY rs.position LIMIT 1)) reality_short_id,NULLIF(r.fingerprint,'') reality_fingerprint,NULLIF(w.request_path,'') transport_path,(SELECT NULLIF(h.header_value,'') FROM transport_headers h WHERE h.revision_id=i.revision_id AND h.endpoint_kind='inbound' AND h.endpoint_id=i.inbound_id AND h.transport_kind=COALESCE(s.network,'tcp') AND LOWER(h.header_name)='host' LIMIT 1) transport_host,NULLIF(g.service_name,'') grpc_service_name FROM users u JOIN user_credentials c ON c.revision_id=u.revision_id AND c.user_id=u.user_id JOIN inbounds i ON i.revision_id=u.revision_id AND i.inbound_id=u.inbound_id LEFT JOIN stream_settings s ON s.revision_id=i.revision_id AND s.endpoint_kind='inbound' AND s.endpoint_id=i.inbound_id LEFT JOIN tls_settings tls ON tls.revision_id=i.revision_id AND tls.endpoint_kind='inbound' AND tls.endpoint_id=i.inbound_id LEFT JOIN reality_settings r ON r.revision_id=i.revision_id AND r.endpoint_kind='inbound' AND r.endpoint_id=i.inbound_id LEFT JOIN websocket_settings w ON w.revision_id=i.revision_id AND w.endpoint_kind='inbound' AND w.endpoint_id=i.inbound_id AND w.transport_kind=COALESCE(s.network,'tcp') LEFT JOIN grpc_settings g ON g.revision_id=i.revision_id AND g.endpoint_kind='inbound' AND g.endpoint_id=i.inbound_id LEFT JOIN user_traffic t ON t.user_id=u.user_id WHERE u.revision_id=? AND u.subscription_token=? AND u.enabled=TRUE AND i.enabled=TRUE AND (u.traffic_limit_bytes IS NULL OR COALESCE(t.upload_bytes,0)+COALESCE(t.download_bytes,0)<u.traffic_limit_bytes) LIMIT 1",
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
                server_name: row.try_get("server_name")?,
                reality_public_key: row.try_get("reality_public_key")?,
                reality_short_id: row.try_get("reality_short_id")?,
                reality_fingerprint: row.try_get("reality_fingerprint")?,
                transport_path: row.try_get("transport_path")?,
                transport_host: row.try_get("transport_host")?,
                grpc_service_name: row.try_get("grpc_service_name")?,
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
        let class = self
            .inbound_activation_class(expected_revision, input.id, &input.transport)
            .await?;
        let (mut tx, revision) = self
            .fork_revision(expected_revision, actor, "Save inbound", class)
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
            .bind(revision).bind(id).bind(&input.tag).bind(&input.listen).bind(input.port).bind(&input.protocol).bind(input.enabled).bind(position)
            .execute(&mut *tx).await?;
        // Update the stream envelope in place. Deleting this row cascades into
        // TLS, REALITY, WebSocket, gRPC, and mKCP detail tables, so a basic
        // panel edit must never replace it destructively.
        sqlx::query("INSERT INTO stream_settings (revision_id,endpoint_kind,endpoint_id,network,security) VALUES (?,'inbound',?,?,?) ON DUPLICATE KEY UPDATE network=VALUES(network),security=VALUES(security)")
            .bind(revision).bind(id).bind(&input.transport).bind(&input.security).execute(&mut *tx).await?;
        write_inbound_details(&mut tx, revision, id, &input).await?;
        Database::publish_revision(&mut tx, revision, class).await?;
        tx.commit().await?;
        Ok(mutation_result(
            &state,
            revision,
            class,
            if class == ActivationClass::MaintenanceRequired {
                "Inbound saved; mKCP activation requires maintenance confirmation"
            } else {
                "Inbound saved"
            },
        ))
    }

    pub async fn delete_inbound(
        &self,
        actor: &str,
        expected_revision: i64,
        id: i64,
    ) -> StoreResult<MutationResult> {
        let state = self.state().await?;
        let class = self
            .inbound_activation_class(expected_revision, Some(id), "tcp")
            .await?;
        let (mut tx, revision) = self
            .fork_revision(expected_revision, actor, "Delete inbound", class)
            .await?;
        sqlx::query("DELETE FROM inbounds WHERE revision_id=? AND inbound_id=?")
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
            if class == ActivationClass::MaintenanceRequired {
                "Inbound deleted; mKCP shutdown requires maintenance confirmation"
            } else {
                "Inbound deleted"
            },
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
        let class = ActivationClass::HotSwap;
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
            .bind(revision).bind(id).bind(&input.tag).bind(&input.protocol).bind(input.enabled).bind(position).bind(&input.address).bind(input.port)
            .execute(&mut *tx).await?;
        // Preserve protocol/transport child rows when the panel changes only
        // the endpoint envelope. See the inbound path above for why this must
        // be an upsert rather than delete-and-recreate.
        sqlx::query("INSERT INTO stream_settings (revision_id,endpoint_kind,endpoint_id,network,security) VALUES (?,'outbound',?,?,?) ON DUPLICATE KEY UPDATE network=VALUES(network),security=VALUES(security)")
            .bind(revision).bind(id).bind(&input.transport).bind(&input.security).execute(&mut *tx).await?;
        write_outbound_details(&mut tx, revision, id, &input).await?;
        Database::publish_revision(&mut tx, revision, class).await?;
        tx.commit().await?;
        Ok(mutation_result(&state, revision, class, "Outbound saved"))
    }

    pub async fn delete_outbound(
        &self,
        actor: &str,
        expected_revision: i64,
        id: i64,
    ) -> StoreResult<MutationResult> {
        let state = self.state().await?;
        let class = ActivationClass::HotSwap;
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
        Ok(mutation_result(&state, revision, class, "Outbound deleted"))
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

    async fn inbound_activation_class(
        &self,
        revision: i64,
        inbound_id: Option<i64>,
        next_transport: &str,
    ) -> StoreResult<ActivationClass> {
        if next_transport == "kcp" {
            return Ok(ActivationClass::MaintenanceRequired);
        }
        let Some(inbound_id) = inbound_id else {
            return Ok(ActivationClass::ListenerHandover);
        };
        let current_transport: Option<String> = sqlx::query_scalar(
            "SELECT COALESCE(s.network,'tcp') FROM inbounds i LEFT JOIN stream_settings s ON s.revision_id=i.revision_id AND s.endpoint_kind='inbound' AND s.endpoint_id=i.inbound_id WHERE i.revision_id=? AND i.inbound_id=?",
        )
        .bind(revision)
        .bind(inbound_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(if current_transport.as_deref() == Some("kcp") {
            ActivationClass::MaintenanceRequired
        } else {
            ActivationClass::ListenerHandover
        })
    }
}

async fn write_inbound_details(
    tx: &mut Transaction<'_, MySql>,
    revision: i64,
    id: i64,
    input: &InboundWrite,
) -> StoreResult<()> {
    if let Some(settings) = &input.settings {
        sqlx::query("INSERT INTO inbound_protocol_settings (revision_id,inbound_id,decryption,method,auth_value,up_mbps,down_mbps,endpoint_shards,network,auth_timeout_ms) VALUES (?,?,?,?,?,?,?,?,?,?) ON DUPLICATE KEY UPDATE decryption=VALUES(decryption),method=VALUES(method),auth_value=VALUES(auth_value),up_mbps=VALUES(up_mbps),down_mbps=VALUES(down_mbps),endpoint_shards=VALUES(endpoint_shards),network=VALUES(network),auth_timeout_ms=VALUES(auth_timeout_ms)")
            .bind(revision).bind(id).bind(&settings.decryption).bind(&settings.method)
            .bind(settings.auth.as_ref().map(|value| value.as_bytes()))
            .bind(settings.up_mbps).bind(settings.down_mbps)
            .bind(settings.endpoint_shards.map(|value| value as u64))
            .bind(&settings.network).bind(settings.auth_timeout_ms)
            .execute(&mut **tx).await?;
        write_endpoint_tuning(tx, revision, "inbound", id, settings).await?;
    }
    if let Some(stream) = &input.stream_settings {
        write_stream_details(tx, revision, "inbound", id, stream).await?;
    }
    if let Some(sniffing) = &input.sniffing {
        sqlx::query("INSERT INTO sniffing_settings (revision_id,inbound_id,enabled,metadata_only,route_only) VALUES (?,?,?,?,?) ON DUPLICATE KEY UPDATE enabled=VALUES(enabled),metadata_only=VALUES(metadata_only),route_only=VALUES(route_only)")
            .bind(revision).bind(id).bind(sniffing.enabled).bind(sniffing.metadata_only).bind(sniffing.route_only)
            .execute(&mut **tx).await?;
        sqlx::query("DELETE FROM sniffing_overrides WHERE revision_id=? AND inbound_id=?")
            .bind(revision)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        for (position, protocol) in sniffing.dest_override.iter().enumerate() {
            sqlx::query("INSERT INTO sniffing_overrides (revision_id,inbound_id,position,protocol) VALUES (?,?,?,?)")
                .bind(revision).bind(id).bind(position as u32).bind(protocol)
                .execute(&mut **tx).await?;
        }
    }
    if let Some(limits) = &input.limits {
        sqlx::query("INSERT INTO inbound_limits (revision_id,inbound_id,max_connections,max_handshake_seconds,max_idle_seconds) VALUES (?,?,?,?,?) ON DUPLICATE KEY UPDATE max_connections=VALUES(max_connections),max_handshake_seconds=VALUES(max_handshake_seconds),max_idle_seconds=VALUES(max_idle_seconds)")
            .bind(revision).bind(id).bind(limits.max_connections.map(|value| value as u64))
            .bind(limits.max_handshake_seconds).bind(limits.max_idle_seconds)
            .execute(&mut **tx).await?;
    }
    Ok(())
}

async fn write_outbound_details(
    tx: &mut Transaction<'_, MySql>,
    revision: i64,
    id: i64,
    input: &OutboundWrite,
) -> StoreResult<()> {
    if let Some(settings) = &input.settings {
        let primary_user = settings.users.first().or_else(|| settings.clients.first());
        let password = settings
            .password
            .as_ref()
            .or_else(|| primary_user.and_then(|user| user.password.as_ref()));
        let auth = settings
            .auth
            .as_ref()
            .or_else(|| primary_user.and_then(|user| user.auth.as_ref()));
        let uuid = settings
            .uuid
            .as_ref()
            .or_else(|| primary_user.and_then(|user| user.id.as_ref()));
        let flow = (!settings.flow.is_empty())
            .then_some(&settings.flow)
            .or_else(|| {
                primary_user
                    .map(|user| &user.flow)
                    .filter(|flow| !flow.is_empty())
            });
        sqlx::query("UPDATE outbounds SET server_address=?,server_port=?,domain_strategy=?,deny_loopback=?,reject_ipv6_literal=? WHERE revision_id=? AND outbound_id=?")
            .bind(settings.address.as_ref().or(settings.server.as_ref()).or(input.address.as_ref())).bind(settings.port.or(input.port))
            .bind(&settings.domain_strategy).bind(settings.deny_loopback).bind(settings.reject_ipv6_literal)
            .bind(revision).bind(id).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO outbound_protocol_settings (revision_id,outbound_id,password_value,auth_value,method,uuid_value,flow,server_name,skip_certificate_verify,endpoint_shards) VALUES (?,?,?,?,?,?,?,?,?,?) ON DUPLICATE KEY UPDATE password_value=VALUES(password_value),auth_value=VALUES(auth_value),method=VALUES(method),uuid_value=VALUES(uuid_value),flow=VALUES(flow),server_name=VALUES(server_name),skip_certificate_verify=VALUES(skip_certificate_verify),endpoint_shards=VALUES(endpoint_shards)")
            .bind(revision).bind(id)
            .bind(password.map(|value| value.as_bytes()))
            .bind(auth.map(|value| value.as_bytes()))
            .bind(&settings.method).bind(uuid).bind(flow).bind(&settings.server_name)
            .bind(settings.skip_cert_verify).bind(settings.endpoint_shards.map(|value| value as u64))
            .execute(&mut **tx).await?;
        write_endpoint_tuning(tx, revision, "outbound", id, settings).await?;
    }
    if let Some(stream) = &input.stream_settings {
        write_stream_details(tx, revision, "outbound", id, stream).await?;
    }
    Ok(())
}

async fn write_stream_details(
    tx: &mut Transaction<'_, MySql>,
    revision: i64,
    kind: &str,
    id: i64,
    stream: &StreamSettingsConfig,
) -> StoreResult<()> {
    if let Some(tls) = &stream.tls_settings {
        sqlx::query("INSERT INTO tls_settings (revision_id,endpoint_kind,endpoint_id,server_name,allow_insecure,certificate_file,key_file) VALUES (?,?,?,?,?,?,?) ON DUPLICATE KEY UPDATE server_name=VALUES(server_name),allow_insecure=VALUES(allow_insecure),certificate_file=VALUES(certificate_file),key_file=VALUES(key_file)")
            .bind(revision).bind(kind).bind(id).bind(&tls.server_name).bind(tls.allow_insecure)
            .bind(&tls.certificate_file).bind(&tls.key_file).execute(&mut **tx).await?;
        sqlx::query(
            "DELETE FROM tls_alpn WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=?",
        )
        .bind(revision)
        .bind(kind)
        .bind(id)
        .execute(&mut **tx)
        .await?;
        for (position, protocol) in tls.alpn.iter().enumerate() {
            sqlx::query("INSERT INTO tls_alpn (revision_id,endpoint_kind,endpoint_id,position,protocol) VALUES (?,?,?,?,?)")
                .bind(revision).bind(kind).bind(id).bind(position as u32).bind(protocol)
                .execute(&mut **tx).await?;
        }
    } else {
        sqlx::query(
            "DELETE FROM tls_settings WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=?",
        )
        .bind(revision)
        .bind(kind)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    }
    if let Some(shadow) = &stream.shadow_tls_settings {
        sqlx::query("INSERT INTO shadowtls_settings (revision_id,endpoint_kind,endpoint_id,password_value,destination,version) VALUES (?,?,?,?,?,?) ON DUPLICATE KEY UPDATE password_value=VALUES(password_value),destination=VALUES(destination),version=VALUES(version)")
            .bind(revision).bind(kind).bind(id).bind(shadow.password.as_bytes()).bind(shadow.dest.trim()).bind(shadow.version)
            .execute(&mut **tx).await?;
    } else {
        sqlx::query("DELETE FROM shadowtls_settings WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=?")
            .bind(revision).bind(kind).bind(id).execute(&mut **tx).await?;
    }
    if let Some(reality) = &stream.reality_settings {
        sqlx::query("INSERT INTO reality_settings (revision_id,endpoint_kind,endpoint_id,show_details,destination,private_key,public_key,short_id,fingerprint,server_name,max_time_diff_seconds,fallback_upload_after_bytes,fallback_upload_bytes_per_sec,fallback_upload_burst_bytes_per_sec,fallback_download_after_bytes,fallback_download_bytes_per_sec,fallback_download_burst_bytes_per_sec) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON DUPLICATE KEY UPDATE show_details=VALUES(show_details),destination=VALUES(destination),private_key=VALUES(private_key),public_key=VALUES(public_key),short_id=VALUES(short_id),fingerprint=VALUES(fingerprint),server_name=VALUES(server_name),max_time_diff_seconds=VALUES(max_time_diff_seconds),fallback_upload_after_bytes=VALUES(fallback_upload_after_bytes),fallback_upload_bytes_per_sec=VALUES(fallback_upload_bytes_per_sec),fallback_upload_burst_bytes_per_sec=VALUES(fallback_upload_burst_bytes_per_sec),fallback_download_after_bytes=VALUES(fallback_download_after_bytes),fallback_download_bytes_per_sec=VALUES(fallback_download_bytes_per_sec),fallback_download_burst_bytes_per_sec=VALUES(fallback_download_burst_bytes_per_sec)")
            .bind(revision).bind(kind).bind(id).bind(reality.show).bind(&reality.dest).bind(&reality.private_key)
            .bind(&reality.public_key).bind(&reality.short_id).bind(&reality.fingerprint).bind(&reality.server_name)
            .bind(reality.max_time_diff_seconds.or((reality.max_time_diff > 0).then_some(reality.max_time_diff)))
            .bind(reality.limit_fallback_upload.map(|limit| limit.after_bytes))
            .bind(reality.limit_fallback_upload.map(|limit| limit.bytes_per_sec))
            .bind(reality.limit_fallback_upload.map(|limit| limit.burst_bytes_per_sec))
            .bind(reality.limit_fallback_download.map(|limit| limit.after_bytes))
            .bind(reality.limit_fallback_download.map(|limit| limit.bytes_per_sec))
            .bind(reality.limit_fallback_download.map(|limit| limit.burst_bytes_per_sec))
            .execute(&mut **tx).await?;
        sqlx::query("DELETE FROM reality_server_names WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=?")
            .bind(revision).bind(kind).bind(id).execute(&mut **tx).await?;
        for (position, name) in reality.server_names.iter().enumerate() {
            sqlx::query("INSERT INTO reality_server_names (revision_id,endpoint_kind,endpoint_id,position,server_name) VALUES (?,?,?,?,?)")
                .bind(revision).bind(kind).bind(id).bind(position as u32).bind(name).execute(&mut **tx).await?;
        }
        sqlx::query("DELETE FROM reality_short_ids WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=?")
            .bind(revision).bind(kind).bind(id).execute(&mut **tx).await?;
        for (position, short_id) in reality.short_ids.iter().enumerate() {
            sqlx::query("INSERT INTO reality_short_ids (revision_id,endpoint_kind,endpoint_id,position,short_id) VALUES (?,?,?,?,?)")
                .bind(revision).bind(kind).bind(id).bind(position as u32).bind(short_id).execute(&mut **tx).await?;
        }
    } else {
        sqlx::query("DELETE FROM reality_settings WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=?")
            .bind(revision).bind(kind).bind(id).execute(&mut **tx).await?;
    }
    write_web_transport(tx, revision, kind, id, "ws", stream.ws_settings.as_ref()).await?;
    write_web_transport(
        tx,
        revision,
        kind,
        id,
        "httpupgrade",
        stream.httpupgrade_settings.as_ref(),
    )
    .await?;
    write_transport_path(
        tx,
        revision,
        kind,
        id,
        "splithttp",
        stream
            .splithttp_settings
            .as_ref()
            .map(|config| config.path.as_str()),
    )
    .await?;
    write_splithttp(tx, revision, kind, id, stream.splithttp_settings.as_ref()).await?;
    if let Some(grpc) = &stream.grpc_settings {
        sqlx::query("INSERT INTO grpc_settings (revision_id,endpoint_kind,endpoint_id,service_name,multi_mode) VALUES (?,?,?,?,?) ON DUPLICATE KEY UPDATE service_name=VALUES(service_name),multi_mode=VALUES(multi_mode)")
            .bind(revision).bind(kind).bind(id).bind(&grpc.service_name).bind(grpc.multi_mode).execute(&mut **tx).await?;
    } else {
        sqlx::query(
            "DELETE FROM grpc_settings WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=?",
        )
        .bind(revision)
        .bind(kind)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    }
    if let Some(kcp) = &stream.kcp_settings {
        sqlx::query("INSERT INTO kcp_settings (revision_id,endpoint_kind,endpoint_id,header_type,mtu,tti_ms,uplink_capacity,downlink_capacity,congestion,read_buffer_size,write_buffer_size) VALUES (?,?,?,?,?,?,?,?,?,?,?) ON DUPLICATE KEY UPDATE header_type=VALUES(header_type),mtu=VALUES(mtu),tti_ms=VALUES(tti_ms),uplink_capacity=VALUES(uplink_capacity),downlink_capacity=VALUES(downlink_capacity),congestion=VALUES(congestion),read_buffer_size=VALUES(read_buffer_size),write_buffer_size=VALUES(write_buffer_size)")
            .bind(revision).bind(kind).bind(id).bind(&kcp.header).bind(kcp.mtu).bind(kcp.tti)
            .bind(kcp.uplink_capacity).bind(kcp.downlink_capacity).bind(kcp.congestion)
            .bind(kcp.read_buffer_size).bind(kcp.write_buffer_size).execute(&mut **tx).await?;
    } else {
        sqlx::query(
            "DELETE FROM kcp_settings WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=?",
        )
        .bind(revision)
        .bind(kind)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn write_splithttp(
    tx: &mut Transaction<'_, MySql>,
    revision: i64,
    kind: &str,
    id: i64,
    config: Option<&SplitHttpConfig>,
) -> StoreResult<()> {
    let Some(config) = config else {
        sqlx::query("DELETE FROM splithttp_settings WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=?")
            .bind(revision).bind(kind).bind(id).execute(&mut **tx).await?;
        sqlx::query("DELETE FROM transport_headers WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=? AND transport_kind='splithttp'")
            .bind(revision).bind(kind).bind(id).execute(&mut **tx).await?;
        return Ok(());
    };
    let (
        padding_kind,
        padding_fixed,
        padding_range,
        padding_min,
        padding_max,
        padding_from,
        padding_to,
    ) = match config.x_padding_bytes.as_ref() {
        Some(PaddingBytes::Fixed(value)) => (
            Some("fixed"),
            Some(*value as u64),
            None,
            None,
            None,
            None,
            None,
        ),
        Some(PaddingBytes::Range(value)) => (
            Some("range"),
            None,
            Some(value.as_str()),
            None,
            None,
            None,
            None,
        ),
        Some(PaddingBytes::Bounds(value)) => (
            Some("bounds"),
            None,
            None,
            value.min.map(|v| v as u64),
            value.max.map(|v| v as u64),
            value.from.map(|v| v as u64),
            value.to.map(|v| v as u64),
        ),
        None => (None, None, None, None, None, None, None),
    };
    let xmux = config.xmux.as_ref();
    let download = config.download_settings.as_ref();
    sqlx::query("INSERT INTO splithttp_settings (revision_id,endpoint_kind,endpoint_id,method_value,mode_value,uplink_http_method,padding_kind,padding_fixed,padding_range,padding_min,padding_max,padding_from,padding_to,padding_method,padding_header,padding_key,padding_placement,session_placement,session_key,seq_placement,seq_key,uplink_data_placement,uplink_data_key,uplink_chunk_size,sc_max_buffered_posts,xmux_configured,xmux_max_concurrency,xmux_max_connections,xmux_c_max_reuse_times,xmux_h_max_request_times,xmux_h_max_reusable_secs,xmux_h_keep_alive_period,download_configured,download_network,download_security) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON DUPLICATE KEY UPDATE method_value=VALUES(method_value),mode_value=VALUES(mode_value),uplink_http_method=VALUES(uplink_http_method),padding_kind=VALUES(padding_kind),padding_fixed=VALUES(padding_fixed),padding_range=VALUES(padding_range),padding_min=VALUES(padding_min),padding_max=VALUES(padding_max),padding_from=VALUES(padding_from),padding_to=VALUES(padding_to),padding_method=VALUES(padding_method),padding_header=VALUES(padding_header),padding_key=VALUES(padding_key),padding_placement=VALUES(padding_placement),session_placement=VALUES(session_placement),session_key=VALUES(session_key),seq_placement=VALUES(seq_placement),seq_key=VALUES(seq_key),uplink_data_placement=VALUES(uplink_data_placement),uplink_data_key=VALUES(uplink_data_key),uplink_chunk_size=VALUES(uplink_chunk_size),sc_max_buffered_posts=VALUES(sc_max_buffered_posts),xmux_configured=VALUES(xmux_configured),xmux_max_concurrency=VALUES(xmux_max_concurrency),xmux_max_connections=VALUES(xmux_max_connections),xmux_c_max_reuse_times=VALUES(xmux_c_max_reuse_times),xmux_h_max_request_times=VALUES(xmux_h_max_request_times),xmux_h_max_reusable_secs=VALUES(xmux_h_max_reusable_secs),xmux_h_keep_alive_period=VALUES(xmux_h_keep_alive_period),download_configured=VALUES(download_configured),download_network=VALUES(download_network),download_security=VALUES(download_security)")
        .bind(revision).bind(kind).bind(id).bind(&config.method).bind(&config.mode).bind(&config.uplink_http_method)
        .bind(padding_kind).bind(padding_fixed).bind(padding_range).bind(padding_min).bind(padding_max).bind(padding_from).bind(padding_to)
        .bind(&config.x_padding_method).bind(&config.x_padding_header).bind(&config.x_padding_key).bind(&config.x_padding_placement)
        .bind(&config.session_placement).bind(&config.session_key).bind(&config.seq_placement).bind(&config.seq_key).bind(&config.uplink_data_placement).bind(&config.uplink_data_key)
        .bind(u64::from(config.uplink_chunk_size)).bind(config.sc_max_buffered_posts as u64)
        .bind(xmux.is_some()).bind(xmux.and_then(|v| v.max_concurrency).map(|v| v as u64)).bind(xmux.and_then(|v| v.max_connections).map(|v| v as u64)).bind(xmux.and_then(|v| v.c_max_reuse_times).map(|v| v as u64)).bind(xmux.and_then(|v| v.h_max_request_times).map(|v| v as u64)).bind(xmux.and_then(|v| v.h_max_reusable_secs)).bind(xmux.and_then(|v| v.h_keep_alive_period))
        .bind(download.is_some()).bind(download.and_then(|v| v.network.as_ref()).map(network_name)).bind(download.and_then(|v| v.security.as_ref()).map(security_name))
        .execute(&mut **tx).await?;
    sqlx::query(
        "DELETE FROM splithttp_hosts WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=?",
    )
    .bind(revision)
    .bind(kind)
    .bind(id)
    .execute(&mut **tx)
    .await?;
    for (position, host) in config
        .host
        .iter()
        .filter(|value| !value.trim().is_empty())
        .enumerate()
    {
        sqlx::query("INSERT INTO splithttp_hosts (revision_id,endpoint_kind,endpoint_id,position,host_value) VALUES (?,?,?,?,?)").bind(revision).bind(kind).bind(id).bind(position as u32).bind(host.trim()).execute(&mut **tx).await?;
    }
    sqlx::query("DELETE FROM transport_headers WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=? AND transport_kind='splithttp'").bind(revision).bind(kind).bind(id).execute(&mut **tx).await?;
    for (name, value) in &config.headers {
        sqlx::query("INSERT INTO transport_headers (revision_id,endpoint_kind,endpoint_id,transport_kind,header_name,header_value) VALUES (?,?,?,'splithttp',?,?)").bind(revision).bind(kind).bind(id).bind(name).bind(value).execute(&mut **tx).await?;
    }
    Ok(())
}

fn network_name(value: &NetworkType) -> &'static str {
    match value {
        NetworkType::Tcp => "tcp",
        NetworkType::Ws => "ws",
        NetworkType::HttpUpgrade => "httpupgrade",
        NetworkType::Grpc => "grpc",
        NetworkType::Quic => "quic",
        NetworkType::Kcp => "kcp",
        NetworkType::SplitHttp => "splithttp",
    }
}

fn security_name(value: &SecurityType) -> &'static str {
    match value {
        SecurityType::None => "none",
        SecurityType::Tls => "tls",
        SecurityType::Reality => "reality",
        SecurityType::ShadowTls => "shadowtls",
    }
}

async fn write_endpoint_tuning(
    tx: &mut Transaction<'_, MySql>,
    revision: i64,
    kind: &str,
    id: i64,
    settings: &EndpointSettings,
) -> StoreResult<()> {
    if settings.congestion.is_none()
        && settings.quic.is_none()
        && settings.datagram.is_none()
        && settings.fec.is_none()
    {
        sqlx::query(
            "DELETE FROM endpoint_tuning WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=?",
        )
        .bind(revision)
        .bind(kind)
        .bind(id)
        .execute(&mut **tx)
        .await?;
        return Ok(());
    }
    let congestion = settings.congestion.as_ref();
    let quic = settings.quic.as_ref();
    let datagram = settings.datagram.as_ref();
    let fec = settings.fec.as_ref();
    let endpoints = quic
        .and_then(|value| value.endpoints.as_ref())
        .map(|value| match value {
            blackwire_config::schema::EndpointCount::Fixed(count) => count.to_string(),
            blackwire_config::schema::EndpointCount::Named(name) => name.clone(),
        });
    sqlx::query("INSERT INTO endpoint_tuning (revision_id,endpoint_kind,endpoint_id,congestion_mode,min_ack_rate,max_queue_delay_ms,pacing_gain,loss_compensation,quic_reuse_port,quic_endpoints,quic_recv_buffer_bytes,quic_send_buffer_bytes,datagram_enabled,udp_over_datagram,datagram_policy,fec_mode,fec_max_overhead_percent) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON DUPLICATE KEY UPDATE congestion_mode=VALUES(congestion_mode),min_ack_rate=VALUES(min_ack_rate),max_queue_delay_ms=VALUES(max_queue_delay_ms),pacing_gain=VALUES(pacing_gain),loss_compensation=VALUES(loss_compensation),quic_reuse_port=VALUES(quic_reuse_port),quic_endpoints=VALUES(quic_endpoints),quic_recv_buffer_bytes=VALUES(quic_recv_buffer_bytes),quic_send_buffer_bytes=VALUES(quic_send_buffer_bytes),datagram_enabled=VALUES(datagram_enabled),udp_over_datagram=VALUES(udp_over_datagram),datagram_policy=VALUES(datagram_policy),fec_mode=VALUES(fec_mode),fec_max_overhead_percent=VALUES(fec_max_overhead_percent)")
        .bind(revision).bind(kind).bind(id)
        .bind(congestion.map(|value| value.mode.as_str()))
        .bind(congestion.and_then(|value| value.min_ack_rate))
        .bind(congestion.and_then(|value| value.max_queue_delay_ms))
        .bind(congestion.and_then(|value| value.pacing_gain))
        .bind(congestion.and_then(|value| value.loss_compensation))
        .bind(quic.and_then(|value| value.reuse_port)).bind(endpoints)
        .bind(quic.and_then(|value| value.recv_buffer_bytes).map(|value| value as u64))
        .bind(quic.and_then(|value| value.send_buffer_bytes).map(|value| value as u64))
        .bind(datagram.and_then(|value| value.enabled))
        .bind(datagram.and_then(|value| value.udp_over_datagram))
        .bind(datagram.and_then(|value| value.policy.as_deref()))
        .bind(fec.and_then(|value| value.mode.as_deref()))
        .bind(fec.and_then(|value| value.max_overhead_percent))
        .execute(&mut **tx).await?;
    Ok(())
}

async fn write_transport_path(
    tx: &mut Transaction<'_, MySql>,
    revision: i64,
    kind: &str,
    id: i64,
    transport_kind: &str,
    path: Option<&str>,
) -> StoreResult<()> {
    if let Some(path) = path {
        sqlx::query("INSERT INTO websocket_settings (revision_id,endpoint_kind,endpoint_id,transport_kind,request_path) VALUES (?,?,?,?,?) ON DUPLICATE KEY UPDATE request_path=VALUES(request_path)")
            .bind(revision).bind(kind).bind(id).bind(transport_kind).bind(path).execute(&mut **tx).await?;
    } else {
        sqlx::query("DELETE FROM websocket_settings WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=? AND transport_kind=?")
            .bind(revision).bind(kind).bind(id).bind(transport_kind).execute(&mut **tx).await?;
    }
    Ok(())
}

async fn write_web_transport(
    tx: &mut Transaction<'_, MySql>,
    revision: i64,
    kind: &str,
    id: i64,
    transport_kind: &str,
    config: Option<&blackwire_config::schema::WsConfig>,
) -> StoreResult<()> {
    if let Some(config) = config {
        sqlx::query("INSERT INTO websocket_settings (revision_id,endpoint_kind,endpoint_id,transport_kind,request_path) VALUES (?,?,?,?,?) ON DUPLICATE KEY UPDATE request_path=VALUES(request_path)")
            .bind(revision).bind(kind).bind(id).bind(transport_kind).bind(&config.path).execute(&mut **tx).await?;
        sqlx::query("DELETE FROM transport_headers WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=? AND transport_kind=?")
            .bind(revision).bind(kind).bind(id).bind(transport_kind).execute(&mut **tx).await?;
        for (name, value) in &config.headers {
            sqlx::query("INSERT INTO transport_headers (revision_id,endpoint_kind,endpoint_id,transport_kind,header_name,header_value) VALUES (?,?,?,?,?,?)")
                .bind(revision).bind(kind).bind(id).bind(transport_kind).bind(name).bind(value).execute(&mut **tx).await?;
        }
    } else {
        sqlx::query("DELETE FROM websocket_settings WHERE revision_id=? AND endpoint_kind=? AND endpoint_id=? AND transport_kind=?")
            .bind(revision).bind(kind).bind(id).bind(transport_kind).execute(&mut **tx).await?;
    }
    Ok(())
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
