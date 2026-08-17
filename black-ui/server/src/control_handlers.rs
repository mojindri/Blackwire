use axum::{
    extract::{Path, State},
    http::{header, HeaderMap},
    response::IntoResponse,
    Json,
};
use base64::Engine as _;
use blackwire_store::{InboundWrite, OutboundWrite, PanelSettings, UserWrite};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::{
    capabilities,
    error::{ApiResult, AppError},
    models::{
        ApplyResult, BulkUserInput, CurrentAdmin, GeneratedUuid, Inbound, InboundInput, LoginInput,
        LoginResponse, MaintenanceResult, ManagedUser, Outbound, OutboundInput, ServiceStatus,
        Settings, SetupInput, Status, TrafficSnapshot, UserInput,
    },
    mysql_auth as auth,
    mysql_state::AppState,
    service, util,
};

pub async fn setup(
    State(state): State<AppState>,
    Json(input): Json<SetupInput>,
) -> impl IntoResponse {
    if let Err(error) = auth::create_first_admin(&state, &input.username, &input.password).await {
        return error.into_response();
    }
    match auth::create_admin_session(&state, &input.username, &input.password).await {
        Ok((token, username)) => (
            [(header::SET_COOKIE, auth::session_cookie(&token))],
            Json(LoginResponse { token, username }),
        )
            .into_response(),
        Err(error) => error.into_response(),
    }
}

pub async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginInput>,
) -> impl IntoResponse {
    match auth::create_admin_session(&state, &input.username, &input.password).await {
        Ok((token, username)) => (
            [(header::SET_COOKIE, auth::session_cookie(&token))],
            Json(LoginResponse { token, username }),
        )
            .into_response(),
        Err(error) => error.into_response(),
    }
}

pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    match auth::delete_session(&headers, &state).await {
        Ok(()) => (
            [(header::SET_COOKIE, auth::expired_session_cookie())],
            Json(json!({ "ok": true })),
        )
            .into_response(),
        Err(error) => error.into_response(),
    }
}

pub async fn me(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<CurrentAdmin> {
    let admin_id = auth::require(&headers, &state).await?;
    let username = state
        .store
        .admin_username(admin_id)
        .await
        .map_err(store_error)?
        .ok_or_else(AppError::unauthorized)?;
    Ok(Json(CurrentAdmin { username }))
}

pub async fn capabilities(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<crate::models::CapabilityMap> {
    auth::require(&headers, &state).await?;
    Ok(Json(capabilities::blackwire_capabilities()))
}

pub async fn status(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Status> {
    let setup_required = state.store.setup_required().await.map_err(store_error)?;
    if !setup_required {
        auth::require(&headers, &state).await?;
    }
    let config = state.store.state().await.map_err(store_error)?;
    let inbounds = state
        .store
        .list_inbounds(config.desired_revision)
        .await
        .map_err(store_error)?;
    let outbounds = state
        .store
        .list_outbounds(config.desired_revision)
        .await
        .map_err(store_error)?;
    let users = state
        .store
        .list_users(config.desired_revision)
        .await
        .map_err(store_error)?;
    let runtime_reachable = state.store.runtime_healthy().await.map_err(store_error)?;
    let last_reconciliation = config.updated_at.to_rfc3339();
    Ok(Json(Status {
        setup_required,
        database_connected: true,
        schema_version: blackwire_store::EXPECTED_SCHEMA_VERSION,
        desired_revision: config.desired_revision,
        active_revision: config.active_revision,
        pending_maintenance_revision: config.pending_maintenance_revision,
        activation_state: config.activation_state,
        last_activation_error: config.last_error,
        runtime_reachable,
        last_reconciliation,
        inbounds: inbounds.len(),
        outbounds: outbounds.len(),
        users: users.len(),
        active_users: users.iter().filter(|user| user.enabled).count(),
    }))
}

pub async fn get_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Settings> {
    auth::require(&headers, &state).await?;
    Ok(Json(
        state
            .store
            .panel_settings()
            .await
            .map_err(store_error)?
            .into(),
    ))
}

pub async fn update_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(settings): Json<Settings>,
) -> ApiResult<Settings> {
    auth::require(&headers, &state).await?;
    let stored: PanelSettings = settings.into();
    state
        .store
        .save_panel_settings(&stored)
        .await
        .map_err(store_error)?;
    Ok(Json(stored.into()))
}

pub async fn list_inbounds(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Vec<Inbound>> {
    auth::require(&headers, &state).await?;
    let revision = state
        .store
        .state()
        .await
        .map_err(store_error)?
        .desired_revision;
    let records = state
        .store
        .list_inbounds(revision)
        .await
        .map_err(store_error)?;
    Ok(Json(records.into_iter().map(Inbound::from).collect()))
}

pub async fn create_inbound(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<InboundInput>,
) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    validate_inbound(&input)?;
    let expected = expected_revision(&headers)?;
    Ok(Json(
        state
            .store
            .save_inbound("black-ui", expected, inbound_write(None, input))
            .await
            .map_err(store_error)?
            .into(),
    ))
}

pub async fn update_inbound(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(input): Json<InboundInput>,
) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    validate_inbound(&input)?;
    let expected = expected_revision(&headers)?;
    Ok(Json(
        state
            .store
            .save_inbound("black-ui", expected, inbound_write(Some(id), input))
            .await
            .map_err(store_error)?
            .into(),
    ))
}

pub async fn delete_inbound(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    let expected = expected_revision(&headers)?;
    Ok(Json(
        state
            .store
            .delete_inbound("black-ui", expected, id)
            .await
            .map_err(store_error)?
            .into(),
    ))
}

pub async fn list_outbounds(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Vec<Outbound>> {
    auth::require(&headers, &state).await?;
    let revision = state
        .store
        .state()
        .await
        .map_err(store_error)?
        .desired_revision;
    let records = state
        .store
        .list_outbounds(revision)
        .await
        .map_err(store_error)?;
    Ok(Json(records.into_iter().map(Outbound::from).collect()))
}

pub async fn create_outbound(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<OutboundInput>,
) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    validate_tag(&input.tag)?;
    let expected = expected_revision(&headers)?;
    Ok(Json(
        state
            .store
            .save_outbound("black-ui", expected, outbound_write(None, input))
            .await
            .map_err(store_error)?
            .into(),
    ))
}

pub async fn update_outbound(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(input): Json<OutboundInput>,
) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    validate_tag(&input.tag)?;
    let expected = expected_revision(&headers)?;
    Ok(Json(
        state
            .store
            .save_outbound("black-ui", expected, outbound_write(Some(id), input))
            .await
            .map_err(store_error)?
            .into(),
    ))
}

pub async fn delete_outbound(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    let expected = expected_revision(&headers)?;
    Ok(Json(
        state
            .store
            .delete_outbound("black-ui", expected, id)
            .await
            .map_err(store_error)?
            .into(),
    ))
}

pub async fn list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Vec<ManagedUser>> {
    auth::require(&headers, &state).await?;
    let revision = state
        .store
        .state()
        .await
        .map_err(store_error)?
        .desired_revision;
    let records = state
        .store
        .list_users(revision)
        .await
        .map_err(store_error)?;
    Ok(Json(records.into_iter().map(ManagedUser::from).collect()))
}

pub async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<UserInput>,
) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    let expected = expected_revision(&headers)?;
    Ok(Json(
        state
            .store
            .save_user("black-ui", expected, user_write(None, input)?)
            .await
            .map_err(store_error)?
            .into(),
    ))
}

pub async fn update_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(input): Json<UserInput>,
) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    let expected = expected_revision(&headers)?;
    Ok(Json(
        state
            .store
            .save_user("black-ui", expected, user_write(Some(id), input)?)
            .await
            .map_err(store_error)?
            .into(),
    ))
}

pub async fn delete_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    let expected = expected_revision(&headers)?;
    Ok(Json(
        state
            .store
            .delete_user("black-ui", expected, id)
            .await
            .map_err(store_error)?
            .into(),
    ))
}

pub async fn enable_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<ApplyResult> {
    set_user_enabled(&state, &headers, id, true).await
}

pub async fn disable_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<ApplyResult> {
    set_user_enabled(&state, &headers, id, false).await
}

pub async fn reset_user_usage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<ManagedUser> {
    auth::require(&headers, &state).await?;
    state
        .store
        .reset_user_traffic(id)
        .await
        .map_err(store_error)?;
    Ok(Json(load_user(&state, id).await?.into()))
}

pub async fn rotate_user_uuid(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    let expected = expected_revision(&headers)?;
    let mut write = record_to_write(load_user(&state, id).await?);
    write.uuid = Some(uuid::Uuid::new_v4().to_string());
    Ok(Json(
        state
            .store
            .save_user("black-ui", expected, write)
            .await
            .map_err(store_error)?
            .into(),
    ))
}

pub async fn rotate_user_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<ManagedUser> {
    auth::require(&headers, &state).await?;
    let expected = expected_revision(&headers)?;
    let mut write = record_to_write(load_user(&state, id).await?);
    write.subscription_token = util::random_token(24);
    state
        .store
        .save_user("black-ui", expected, write)
        .await
        .map_err(store_error)?;
    Ok(Json(load_user(&state, id).await?.into()))
}

pub async fn bulk_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<BulkUserInput>,
) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    let mut expected = expected_revision(&headers)?;
    if input.user_ids.is_empty() {
        return Err(AppError::bad_request("select at least one user"));
    }
    if input.action == "resetUsage" {
        for id in input.user_ids {
            state
                .store
                .reset_user_traffic(id)
                .await
                .map_err(store_error)?;
        }
        let current = state.store.state().await.map_err(store_error)?;
        return Ok(Json(ApplyResult {
            revision: current.desired_revision,
            parent_revision: current.desired_revision,
            active_revision: current.active_revision,
            state: current.activation_state,
            activation_class: blackwire_store::ActivationClass::HotSwap,
            message: "Usage reset".into(),
        }));
    }
    let expiry = input
        .expiry_at
        .as_deref()
        .map(DateTime::parse_from_rfc3339)
        .transpose()
        .map_err(|_| AppError::bad_request("expiry must be RFC 3339"))?
        .map(|value| value.with_timezone(&Utc));
    let mut last = None;
    for id in input.user_ids {
        let mut write = record_to_write(load_user(&state, id).await?);
        match input.action.as_str() {
            "enable" => write.enabled = true,
            "disable" => write.enabled = false,
            "setTrafficLimit" => write.traffic_limit_bytes = input.traffic_limit_bytes,
            "setExpiry" => write.expiry_at = expiry,
            _ => return Err(AppError::bad_request("unsupported bulk action")),
        }
        last = Some(
            state
                .store
                .save_user("black-ui", expected, write)
                .await
                .map_err(store_error)?,
        );
        expected = last.as_ref().expect("bulk result").revision;
    }
    Ok(Json(last.expect("non-empty bulk operation").into()))
}

async fn load_user(state: &AppState, id: i64) -> Result<blackwire_store::UserRecord, AppError> {
    let revision = state
        .store
        .state()
        .await
        .map_err(store_error)?
        .desired_revision;
    state
        .store
        .list_users(revision)
        .await
        .map_err(store_error)?
        .into_iter()
        .find(|user| user.id == id)
        .ok_or_else(|| AppError::not_found("user not found"))
}

fn record_to_write(user: blackwire_store::UserRecord) -> UserWrite {
    UserWrite {
        id: Some(user.id),
        inbound_id: user.inbound_id,
        email: user.email,
        enabled: user.enabled,
        flow: user.flow,
        note: user.note,
        traffic_limit_bytes: user.traffic_limit_bytes,
        expiry_at: user.expiry_at,
        subscription_token: user.subscription_token,
        credential_kind: user.credential_kind,
        uuid: user.uuid,
        password: None,
        method: user.method,
        auth: None,
    }
}

async fn set_user_enabled(
    state: &AppState,
    headers: &HeaderMap,
    id: i64,
    enabled: bool,
) -> ApiResult<ApplyResult> {
    auth::require(headers, state).await?;
    let revision = expected_revision(headers)?;
    let user = state
        .store
        .list_users(revision)
        .await
        .map_err(store_error)?
        .into_iter()
        .find(|user| user.id == id)
        .ok_or_else(|| AppError::not_found("user not found"))?;
    let input = UserWrite {
        id: Some(id),
        inbound_id: user.inbound_id,
        email: user.email,
        enabled,
        flow: user.flow,
        note: user.note,
        traffic_limit_bytes: user.traffic_limit_bytes,
        expiry_at: user.expiry_at,
        subscription_token: user.subscription_token,
        credential_kind: user.credential_kind,
        uuid: user.uuid,
        password: None,
        method: user.method,
        auth: None,
    };
    Ok(Json(
        state
            .store
            .save_user("black-ui", revision, input)
            .await
            .map_err(store_error)?
            .into(),
    ))
}

pub async fn generate_uuid(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<GeneratedUuid> {
    auth::require(&headers, &state).await?;
    Ok(Json(GeneratedUuid {
        uuid: uuid::Uuid::new_v4().to_string(),
    }))
}

pub async fn runtime_traffic(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<TrafficSnapshot> {
    auth::require(&headers, &state).await?;
    let (users, inbounds) = state.store.traffic_snapshot().await.map_err(store_error)?;
    Ok(Json(TrafficSnapshot {
        users: users
            .into_iter()
            .map(|row| crate::models::UserTraffic {
                email: row.email,
                upload_bytes: row.upload_bytes.min(i64::MAX as u64) as i64,
                download_bytes: row.download_bytes.min(i64::MAX as u64) as i64,
            })
            .collect(),
        inbounds: inbounds
            .into_iter()
            .map(|row| crate::models::InboundTraffic {
                tag: row.tag,
                upload_bytes: row.upload_bytes.min(i64::MAX as u64) as i64,
                download_bytes: row.download_bytes.min(i64::MAX as u64) as i64,
            })
            .collect(),
    }))
}

pub async fn get_routing_dns(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<blackwire_store::RoutingDnsRecord> {
    auth::require(&headers, &state).await?;
    let revision = state
        .store
        .state()
        .await
        .map_err(store_error)?
        .desired_revision;
    Ok(Json(
        state
            .store
            .routing_dns(revision)
            .await
            .map_err(store_error)?,
    ))
}

pub async fn update_routing_dns(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<blackwire_store::RoutingDnsWrite>,
) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    let expected = expected_revision(&headers)?;
    Ok(Json(
        state
            .store
            .save_routing_dns("black-ui", expected, input)
            .await
            .map_err(store_error)?
            .into(),
    ))
}

pub async fn revision_history(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Vec<blackwire_store::RevisionSummary>> {
    auth::require(&headers, &state).await?;
    Ok(Json(state.store.history(20).await.map_err(store_error)?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionInput {
    revision: i64,
}

pub async fn rollback_revision(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<RevisionInput>,
) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    Ok(Json(
        state
            .store
            .rollback(input.revision, "black-ui")
            .await
            .map_err(store_error)?
            .into(),
    ))
}

pub async fn activate_maintenance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<RevisionInput>,
) -> ApiResult<MaintenanceResult> {
    auth::require(&headers, &state).await?;
    state
        .store
        .confirm_maintenance(input.revision)
        .await
        .map_err(store_error)?;
    Ok(Json(MaintenanceResult {
        revision: input.revision,
        message: "Maintenance activation confirmed".into(),
    }))
}

pub async fn service_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<ServiceStatus> {
    auth::require(&headers, &state).await?;
    Ok(Json(service::blackwire_status()))
}

pub async fn service_restart_blackwire(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<ServiceStatus> {
    auth::require(&headers, &state).await?;
    Ok(Json(
        service::restart_blackwire().map_err(AppError::internal)?,
    ))
}

pub async fn service_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Vec<String>> {
    auth::require(&headers, &state).await?;
    Ok(Json(service::recent_logs()))
}

pub async fn subscription_base64(
    State(state): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Result<String, AppError> {
    let link = subscription_link(&state, &token, &headers).await?;
    Ok(base64::engine::general_purpose::STANDARD.encode(link))
}

pub async fn subscription_raw(
    State(state): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Result<String, AppError> {
    subscription_link(&state, &token, &headers).await
}

async fn subscription_link(
    state: &AppState,
    token: &str,
    headers: &HeaderMap,
) -> Result<String, AppError> {
    let user = state
        .store
        .subscription_by_token(token)
        .await
        .map_err(store_error)?
        .ok_or_else(|| AppError::not_found("subscription not found"))?;
    if !user.enabled || user.expiry_at.is_some_and(|expiry| expiry <= Utc::now()) {
        return Err(AppError::not_found("subscription not found"));
    }
    let panel = state.store.panel_settings().await.map_err(store_error)?;
    let host = public_subscription_host(&panel.subscription_host, headers);
    let label = url_escape(&user.email);
    let network = if user.transport == "splithttp" {
        "xhttp"
    } else {
        &user.transport
    };
    let mut params = vec![format!("type={}", url_escape(network))];
    if user.security != "none" {
        params.push(format!("security={}", url_escape(&user.security)));
    } else {
        params.push("security=none".into());
    }
    if !user.flow.is_empty() {
        params.push(format!("flow={}", url_escape(&user.flow)));
    }
    if let Some(server_name) = user.server_name.as_deref() {
        params.push(format!("sni={}", url_escape(server_name)));
    }
    if user.security == "reality" {
        if let Some(public_key) = user.reality_public_key.as_deref() {
            params.push(format!("pbk={}", url_escape(public_key)));
        }
        if let Some(short_id) = user.reality_short_id.as_deref() {
            params.push(format!("sid={}", url_escape(short_id)));
        }
        if let Some(fingerprint) = user.reality_fingerprint.as_deref() {
            params.push(format!("fp={}", url_escape(fingerprint)));
        }
    }
    if let Some(path) = user.transport_path.as_deref() {
        params.push(format!("path={}", url_escape(path)));
    }
    if let Some(host) = user.transport_host.as_deref() {
        params.push(format!("host={}", url_escape(host)));
    }
    if let Some(service_name) = user.grpc_service_name.as_deref() {
        params.push(format!("serviceName={}", url_escape(service_name)));
    }
    let uuid = user.uuid.as_deref().unwrap_or_default();
    match user.protocol.as_str() {
        "vless" => Ok(format!(
            "vless://{}@{}:{}?encryption=none&{}#{}",
            url_escape(uuid),
            host,
            user.port,
            params.join("&"),
            label
        )),
        "vmess" => {
            let payload = json!({
                "v": "2", "ps": user.email, "add": host, "port": user.port.to_string(),
                "id": uuid, "aid": "0", "scy": "auto", "security": "auto",
                "net": network, "type": if network == "grpc" { "gun" } else { "none" },
                "host": user.transport_host.as_deref().unwrap_or_default(),
                "path": if network == "grpc" { user.grpc_service_name.as_deref().unwrap_or_default() } else { user.transport_path.as_deref().unwrap_or_default() },
                "tls": if user.security == "none" { "" } else { &user.security },
                "sni": user.server_name.as_deref().unwrap_or_default(), "alpn": ""
            });
            Ok(format!(
                "vmess://{}",
                base64::engine::general_purpose::STANDARD
                    .encode(serde_json::to_vec(&payload).unwrap_or_default())
            ))
        }
        "trojan" => {
            let password = user.password.as_deref().unwrap_or(uuid);
            Ok(format!(
                "trojan://{}@{}:{}?{}#{}",
                url_escape(password),
                host,
                user.port,
                params.join("&"),
                label
            ))
        }
        "shadowsocks" => {
            let method = user.method.as_deref().unwrap_or("2022-blake3-aes-256-gcm");
            let password = user.password.as_deref().unwrap_or(uuid);
            let credentials = base64::engine::general_purpose::STANDARD_NO_PAD
                .encode(format!("{method}:{password}"));
            Ok(format!(
                "ss://{}@{}:{}#{}",
                credentials, host, user.port, label
            ))
        }
        "hysteria2" => {
            let auth = user
                .auth
                .as_deref()
                .or(user.password.as_deref())
                .unwrap_or(uuid);
            let security = if user.security == "tls" {
                ""
            } else {
                "?insecure=1"
            };
            Ok(format!(
                "hysteria2://{}@{}:{}{}#{}",
                url_escape(auth),
                host,
                user.port,
                security,
                label
            ))
        }
        "tuic" => {
            let password = user.password.as_deref().unwrap_or(uuid);
            Ok(format!(
                "tuic://{}:{}@{}:{}?uuid={}#{}",
                url_escape(uuid),
                url_escape(password),
                host,
                user.port,
                url_escape(uuid),
                label
            ))
        }
        _ => Err(AppError::not_found(
            "subscription not available for this protocol",
        )),
    }
}

fn public_subscription_host(configured: &str, headers: &HeaderMap) -> String {
    if !matches!(configured.trim(), "" | "localhost" | "127.0.0.1" | "::1") {
        return configured.trim().to_string();
    }
    headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .strip_prefix('[')
                .and_then(|rest| rest.split_once(']').map(|(host, _)| host.to_string()))
                .or_else(|| value.split(':').next().map(str::to_string))
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| configured.trim().to_string())
}

fn url_escape(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

fn inbound_write(id: Option<i64>, input: InboundInput) -> InboundWrite {
    InboundWrite {
        id,
        tag: input.tag,
        listen: input.listen,
        port: input.port,
        protocol: input.protocol,
        enabled: input.enabled,
        transport: input.transport,
        security: input.security,
    }
}

fn outbound_write(id: Option<i64>, input: OutboundInput) -> OutboundWrite {
    OutboundWrite {
        id,
        tag: input.tag,
        protocol: input.protocol,
        enabled: input.enabled,
        address: input.address,
        port: input.port,
        transport: input.transport,
        security: input.security,
    }
}

fn user_write(id: Option<i64>, input: UserInput) -> Result<UserWrite, AppError> {
    if input.email.trim().is_empty() {
        return Err(AppError::bad_request("email is required"));
    }
    let expiry_at = input
        .expiry_at
        .as_deref()
        .map(DateTime::parse_from_rfc3339)
        .transpose()
        .map_err(|_| AppError::bad_request("expiry must be RFC 3339"))?
        .map(|value| value.with_timezone(&Utc));
    Ok(UserWrite {
        id,
        inbound_id: input.inbound_id,
        email: input.email,
        enabled: input.enabled,
        flow: input.flow.unwrap_or_default(),
        note: input.note.unwrap_or_default(),
        traffic_limit_bytes: input.traffic_limit_bytes,
        expiry_at,
        subscription_token: input
            .subscription_token
            .unwrap_or_else(|| util::random_token(24)),
        credential_kind: input.credential_kind.unwrap_or_else(|| "uuid".into()),
        uuid: Some(input.uuid),
        password: input.password,
        method: input.method,
        auth: input.auth,
    })
}

fn validate_inbound(input: &InboundInput) -> Result<(), AppError> {
    validate_tag(&input.tag)?;
    if input.port == 0 {
        return Err(AppError::bad_request("port must be between 1 and 65535"));
    }
    Ok(())
}

fn validate_tag(tag: &str) -> Result<(), AppError> {
    if tag.trim().is_empty() {
        Err(AppError::bad_request("tag is required"))
    } else {
        Ok(())
    }
}

fn expected_revision(headers: &HeaderMap) -> Result<i64, AppError> {
    headers
        .get(header::IF_MATCH)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().trim_matches('"'))
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| AppError::bad_request("configuration writes require the current revision"))
}

fn store_error(error: blackwire_store::StoreError) -> AppError {
    match error {
        blackwire_store::StoreError::RevisionConflict { expected, actual } => AppError::conflict(
            format!("configuration revision conflict: expected {expected}, current desired revision is {actual}; refresh before saving this stale edit"),
        ),
        blackwire_store::StoreError::Sql(_) => AppError::service_unavailable(
            "MySQL is unavailable; configuration remains read-only until it reconnects",
        ),
        other => AppError::internal(other.into()),
    }
}

impl From<blackwire_store::MutationResult> for ApplyResult {
    fn from(value: blackwire_store::MutationResult) -> Self {
        Self {
            revision: value.revision,
            parent_revision: value.parent_revision,
            active_revision: value.active_revision,
            state: value.state,
            activation_class: value.activation_class,
            message: value.message,
        }
    }
}
