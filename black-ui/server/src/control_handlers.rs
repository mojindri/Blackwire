use axum::{
    extract::{Path, State},
    http::{header, HeaderMap},
    response::IntoResponse,
    Json,
};
use base64::Engine as _;
use blackwire_store::{InboundWrite, OutboundWrite, PanelSettings, UserWrite};
use chrono::{DateTime, Utc};
use rand::RngExt;
use serde::Deserialize;
use serde_json::json;
use x25519_dalek::{PublicKey, StaticSecret};

use crate::{
    capabilities,
    error::{ApiResult, AppError},
    models::{
        ApplyResult, BulkUserInput, CurrentAdmin, GeneratedUuid, Inbound, InboundInput, LoginInput,
        LoginResponse, MaintenanceResult, ManagedUser, Outbound, OutboundInput,
        RealityClientValues, RealityGeneratedValues, ServiceStatus, Settings, SetupInput, Status,
        TlsSelfSignedInput, TlsSelfSignedResult, TlsServerValues, TrafficSnapshot, UserInput,
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

/// Returns only the setup gate used by the unauthenticated login page.
/// Operational status remains behind `GET /api/status` authentication.
pub async fn auth_bootstrap_status(
    State(state): State<AppState>,
) -> ApiResult<crate::models::AuthBootstrapStatus> {
    Ok(Json(crate::models::AuthBootstrapStatus {
        setup_required: state.store.setup_required().await.map_err(store_error)?,
    }))
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
    let snapshot = state
        .store
        .load_config(revision)
        .await
        .map_err(store_error)?;
    let mut result = Vec::with_capacity(records.len());
    for record in records {
        let mut model = Inbound::from(record);
        if let Some(config) = snapshot
            .config
            .inbounds
            .iter()
            .find(|item| item.tag == model.tag)
        {
            let mut settings = config.settings.clone();
            // Managed users have their own authenticated API. Do not duplicate
            // their credentials inside the endpoint editor payload.
            settings.clients.clear();
            settings.users.clear();
            model.settings = json_text(&settings)?;
            model.stream_settings = config
                .stream_settings
                .as_ref()
                .map(json_text)
                .transpose()?
                .unwrap_or_else(|| "{}".into());
            model.sniffing = config
                .sniffing
                .as_ref()
                .map(json_text)
                .transpose()?
                .unwrap_or_else(|| "{}".into());
            model.limits = config
                .limits
                .as_ref()
                .map(json_text)
                .transpose()?
                .unwrap_or_else(|| "{}".into());
        }
        result.push(model);
    }
    Ok(Json(result))
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
            .save_inbound("black-ui", expected, inbound_write(None, input)?)
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
            .save_inbound("black-ui", expected, inbound_write(Some(id), input)?)
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
    let snapshot = state
        .store
        .load_config(revision)
        .await
        .map_err(store_error)?;
    let mut result = Vec::with_capacity(records.len());
    for record in records {
        let mut model = Outbound::from(record);
        if let Some(config) = snapshot
            .config
            .outbounds
            .iter()
            .find(|item| item.tag == model.tag)
        {
            model.settings = json_text(&config.settings)?;
            model.stream_settings = config
                .stream_settings
                .as_ref()
                .map(json_text)
                .transpose()?
                .unwrap_or_else(|| "{}".into());
        }
        result.push(model);
    }
    Ok(Json(result))
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
            .save_outbound("black-ui", expected, outbound_write(None, input)?)
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
            .save_outbound("black-ui", expected, outbound_write(Some(id), input)?)
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

pub async fn reality_client_values(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Vec<RealityClientValues>> {
    auth::require(&headers, &state).await?;
    let revision = state
        .store
        .state()
        .await
        .map_err(store_error)?
        .desired_revision;
    let snapshot = state
        .store
        .load_config(revision)
        .await
        .map_err(store_error)?;
    let source = format!("MySQL revision {revision}");
    let values = snapshot
        .config
        .inbounds
        .iter()
        .filter_map(|inbound| {
            let reality = inbound
                .stream_settings
                .as_ref()?
                .reality_settings
                .as_ref()?;
            Some(RealityClientValues {
                source: source.clone(),
                tag: Some(inbound.tag.clone()),
                address: None,
                port: Some(inbound.port),
                uuid: inbound
                    .settings
                    .clients
                    .first()
                    .and_then(|user| user.identifier())
                    .map(str::to_owned),
                private_key: (!reality.private_key.is_empty()).then(|| reality.private_key.clone()),
                public_key: reality.public_key.clone(),
                short_id: reality
                    .short_ids
                    .first()
                    .cloned()
                    .unwrap_or_else(|| reality.short_id.clone()),
                server_name: reality
                    .server_names
                    .first()
                    .cloned()
                    .unwrap_or_else(|| reality.server_name.clone()),
                dest: (!reality.dest.is_empty()).then(|| reality.dest.clone()),
            })
        })
        .collect();
    Ok(Json(values))
}

pub async fn reality_generate_values(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<RealityGeneratedValues> {
    auth::require(&headers, &state).await?;
    let secret = StaticSecret::random();
    let public = PublicKey::from(&secret);
    let mut short_id = [0u8; 8];
    rand::rng().fill(&mut short_id);
    Ok(Json(RealityGeneratedValues {
        private_key: hex::encode(secret.to_bytes()),
        public_key: hex::encode(public.as_bytes()),
        short_id: hex::encode(short_id),
    }))
}

pub async fn tls_server_values(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Vec<TlsServerValues>> {
    auth::require(&headers, &state).await?;
    let revision = state
        .store
        .state()
        .await
        .map_err(store_error)?
        .desired_revision;
    let snapshot = state
        .store
        .load_config(revision)
        .await
        .map_err(store_error)?;
    let source = format!("MySQL revision {revision}");
    let values = snapshot
        .config
        .inbounds
        .iter()
        .filter_map(|inbound| {
            let tls = inbound.stream_settings.as_ref()?.tls_settings.as_ref()?;
            Some(TlsServerValues {
                source: source.clone(),
                tag: Some(inbound.tag.clone()),
                port: Some(inbound.port),
                server_name: (!tls.server_name.is_empty()).then(|| tls.server_name.clone()),
                alpn: tls.alpn.clone(),
                certificate_file: (!tls.certificate_file.is_empty())
                    .then(|| tls.certificate_file.clone()),
                key_file: (!tls.key_file.is_empty()).then(|| tls.key_file.clone()),
                allow_insecure: tls.allow_insecure,
            })
        })
        .collect();
    Ok(Json(values))
}

pub async fn tls_generate_self_signed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<TlsSelfSignedInput>,
) -> ApiResult<TlsSelfSignedResult> {
    auth::require(&headers, &state).await?;
    crate::tls_cert::generate_self_signed(input)
        .map(Json)
        .map_err(AppError::bad_request)
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

pub async fn get_core_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<blackwire_store::CoreSettings> {
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
            .core_settings(revision)
            .await
            .map_err(store_error)?,
    ))
}

pub async fn update_core_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<blackwire_store::CoreSettings>,
) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    let expected = expected_revision(&headers)?;
    Ok(Json(
        state
            .store
            .save_core_settings("black-ui", expected, input)
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

pub async fn service_start_blackwire(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<ServiceStatus> {
    auth::require(&headers, &state).await?;
    Ok(Json(
        service::start_blackwire().map_err(AppError::internal)?,
    ))
}

pub async fn service_stop_blackwire(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<ServiceStatus> {
    auth::require(&headers, &state).await?;
    Ok(Json(service::stop_blackwire().map_err(AppError::internal)?))
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
    Ok(encode_subscription_content(&link))
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
    build_subscription_link(&user, &host)
        .ok_or_else(|| AppError::not_found("subscription not available for this protocol"))
}

fn encode_subscription_content(link: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(link)
}

fn build_subscription_link(
    user: &blackwire_store::SubscriptionRecord,
    public_host: &str,
) -> Option<String> {
    let authority_host = subscription_authority_host(public_host);
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
            params.push(format!(
                "pbk={}",
                url_escape(&reality_public_key_base64url(public_key))
            ));
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
        "vless" => Some(format!(
            "vless://{}@{}:{}?encryption=none&{}#{}",
            url_escape(uuid),
            authority_host,
            user.port,
            params.join("&"),
            label
        )),
        "vmess" => {
            let payload = json!({
                "v": "2", "ps": user.email, "add": public_host.trim().trim_matches(['[', ']']), "port": user.port.to_string(),
                "id": uuid, "aid": "0", "scy": "auto", "security": "auto",
                "net": network, "type": if network == "grpc" { "gun" } else { "none" },
                "host": user.transport_host.as_deref().unwrap_or_default(),
                "path": if network == "grpc" { user.grpc_service_name.as_deref().unwrap_or_default() } else { user.transport_path.as_deref().unwrap_or_default() },
                "tls": if user.security == "none" { "" } else { &user.security },
                "sni": user.server_name.as_deref().unwrap_or_default(), "alpn": ""
            });
            Some(format!(
                "vmess://{}",
                base64::engine::general_purpose::STANDARD
                    .encode(serde_json::to_vec(&payload).unwrap_or_default())
            ))
        }
        "trojan" => {
            let password = user.password.as_deref().unwrap_or(uuid);
            Some(format!(
                "trojan://{}@{}:{}?{}#{}",
                url_escape(password),
                authority_host,
                user.port,
                params.join("&"),
                label
            ))
        }
        "shadowsocks" => {
            let method = user.method.as_deref().unwrap_or("2022-blake3-aes-256-gcm");
            let password = user.password.as_deref().unwrap_or(uuid);
            let credentials = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(format!("{method}:{password}"));
            Some(format!(
                "ss://{}@{}:{}#{}",
                credentials, authority_host, user.port, label
            ))
        }
        "hysteria2" => {
            let auth = user
                .auth
                .as_deref()
                .or(user.password.as_deref())
                .unwrap_or(uuid);
            let mut query = Vec::new();
            if let Some(server_name) = user.server_name.as_deref() {
                query.push(format!("sni={}", url_escape(server_name)));
            }
            if user.security != "tls" {
                query.push("insecure=1".into());
            }
            let query = if query.is_empty() {
                String::new()
            } else {
                format!("?{}", query.join("&"))
            };
            Some(format!(
                "hysteria2://{}@{}:{}/{}#{}",
                url_escape(auth),
                authority_host,
                user.port,
                query,
                label
            ))
        }
        "tuic" => {
            let password = user.password.as_deref().unwrap_or(uuid);
            let mut query = vec![
                "alpn=h3".to_string(),
                "congestion_control=bbr".to_string(),
                "udp_relay_mode=native".to_string(),
            ];
            if let Some(server_name) = user.server_name.as_deref() {
                query.push(format!("sni={}", url_escape(server_name)));
            }
            if user.security != "tls" {
                query.push("allow_insecure=1".into());
            }
            Some(format!(
                "tuic://{}:{}@{}:{}?{}#{}",
                url_escape(uuid),
                url_escape(password),
                authority_host,
                user.port,
                query.join("&"),
                label
            ))
        }
        _ => None,
    }
}

fn subscription_authority_host(host: &str) -> String {
    let host = host.trim();
    if host.contains(':') && !(host.starts_with('[') && host.ends_with(']')) {
        format!("[{host}]")
    } else {
        host.to_owned()
    }
}

fn reality_public_key_base64url(value: &str) -> String {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        if let Ok(bytes) = hex::decode(value) {
            return base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        }
    }

    let unpadded = value.trim_end_matches('=');
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(unpadded)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(unpadded));
    if let Ok(bytes) = decoded {
        if bytes.len() == 32 {
            return base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        }
    }
    value.to_owned()
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

fn json_text<T: serde::Serialize>(value: &T) -> Result<String, AppError> {
    serde_json::to_string(value).map_err(|error| AppError::internal(error.into()))
}

fn inbound_write(id: Option<i64>, input: InboundInput) -> Result<InboundWrite, AppError> {
    Ok(InboundWrite {
        id,
        tag: input.tag,
        listen: input.listen,
        port: input.port,
        protocol: input.protocol,
        enabled: input.enabled,
        transport: input.transport,
        security: input.security,
        settings: parse_typed_slice(input.settings, "protocol settings")?,
        stream_settings: parse_typed_slice(input.stream_settings, "stream settings")?,
        sniffing: parse_typed_slice(input.sniffing, "sniffing settings")?,
        limits: parse_typed_slice(input.limits, "inbound limits")?,
    })
}

fn outbound_write(id: Option<i64>, input: OutboundInput) -> Result<OutboundWrite, AppError> {
    let settings = parse_typed_slice(input.settings, "protocol settings")?;
    let address = input.address.or_else(|| {
        settings
            .as_ref()
            .and_then(|settings: &blackwire_config::schema::EndpointSettings| {
                settings.address.clone().or_else(|| settings.server.clone())
            })
    });
    let port = input
        .port
        .or_else(|| settings.as_ref().and_then(|settings| settings.port));
    Ok(OutboundWrite {
        id,
        tag: input.tag,
        protocol: input.protocol,
        enabled: input.enabled,
        address,
        port,
        transport: input.transport,
        security: input.security,
        settings,
        stream_settings: parse_typed_slice(input.stream_settings, "stream settings")?,
    })
}

fn parse_typed_slice<T: serde::de::DeserializeOwned>(
    value: Option<String>,
    label: &str,
) -> Result<Option<T>, AppError> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            serde_json::from_str(&value)
                .map_err(|error| AppError::bad_request(format!("invalid {label}: {error}")))
        })
        .transpose()
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
    let credential = input.credential.unwrap_or_default();
    let credential_string = |key: &str| {
        credential
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let password = input.password.or_else(|| credential_string("password"));
    let method = input.method.or_else(|| credential_string("method"));
    let auth = input.auth.or_else(|| credential_string("auth"));
    let uuid = if input.uuid.trim().is_empty() {
        credential_string("id").unwrap_or_default()
    } else {
        input.uuid
    };
    let credential_kind = input.credential_kind.unwrap_or_else(|| {
        if auth.is_some() {
            "auth"
        } else if password.is_some() {
            "password"
        } else {
            "uuid"
        }
        .into()
    });
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
        credential_kind,
        uuid: (!uuid.is_empty()).then_some(uuid),
        password,
        method,
        auth,
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use base64::Engine as _;
    use blackwire_store::SubscriptionRecord;
    use url::Url;

    use super::{
        build_subscription_link, encode_subscription_content, reality_public_key_base64url,
    };

    fn record(protocol: &str) -> SubscriptionRecord {
        SubscriptionRecord {
            email: "Alice + mobile@example.com".into(),
            enabled: true,
            expiry_at: None,
            uuid: Some("1791a4cd-09e3-4a29-a36d-fea98300c845".into()),
            password: Some("p@ss:word/+".into()),
            method: Some("2022-blake3-aes-256-gcm".into()),
            auth: Some("auth:user/value".into()),
            flow: String::new(),
            protocol: protocol.into(),
            port: 443,
            transport: "tcp".into(),
            security: "tls".into(),
            server_name: Some("cover.example.com".into()),
            reality_public_key: None,
            reality_short_id: None,
            reality_fingerprint: None,
            transport_path: None,
            transport_host: None,
            grpc_service_name: None,
        }
    }

    fn link(record: &SubscriptionRecord) -> String {
        build_subscription_link(record, "203.0.113.7").expect("supported subscription")
    }

    fn query(url: &Url) -> HashMap<String, String> {
        url.query_pairs().into_owned().collect()
    }

    #[test]
    fn subscription_reality_key_is_canonical_base64url() {
        let key = [0xa5; 32];
        let expected = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key);
        assert_eq!(reality_public_key_base64url(&hex::encode(key)), expected);
        assert_eq!(
            reality_public_key_base64url(&format!("{expected}=")),
            expected
        );

        let key_with_standard_alphabet = [0xff; 32];
        let standard = base64::engine::general_purpose::STANDARD.encode(key_with_standard_alphabet);
        let canonical =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key_with_standard_alphabet);
        assert_eq!(reality_public_key_base64url(&standard), canonical);
    }

    #[test]
    fn hiddify_vless_reality_exports_every_supported_parameter() {
        let key = [0xff; 32];
        let mut value = record("vless");
        value.security = "reality".into();
        value.flow = "xtls-rprx-vision".into();
        value.reality_public_key = Some(hex::encode(key));
        value.reality_short_id = Some("aabbccdd00000001".into());
        value.reality_fingerprint = Some("chrome".into());

        let parsed = Url::parse(&link(&value)).expect("valid VLESS URI");
        let params = query(&parsed);
        assert_eq!(parsed.scheme(), "vless");
        assert_eq!(parsed.username(), value.uuid.as_deref().unwrap());
        assert_eq!(parsed.host_str(), Some("203.0.113.7"));
        assert_eq!(parsed.port(), Some(443));
        assert_eq!(params.get("encryption").map(String::as_str), Some("none"));
        assert_eq!(params.get("type").map(String::as_str), Some("tcp"));
        assert_eq!(params.get("security").map(String::as_str), Some("reality"));
        assert_eq!(
            params.get("flow").map(String::as_str),
            Some("xtls-rprx-vision")
        );
        assert_eq!(
            params.get("sni").map(String::as_str),
            Some("cover.example.com")
        );
        assert_eq!(
            params.get("sid").map(String::as_str),
            Some("aabbccdd00000001")
        );
        assert_eq!(params.get("fp").map(String::as_str), Some("chrome"));
        assert_eq!(
            params.get("pbk").map(String::as_str),
            Some(
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(key)
                    .as_str()
            )
        );
    }

    #[test]
    fn hiddify_vless_transports_preserve_transport_parameters() {
        let cases = [
            (
                "ws",
                "ws",
                Some("/socket path"),
                Some("cdn.example.com"),
                None,
            ),
            (
                "httpupgrade",
                "httpupgrade",
                Some("/upgrade"),
                Some("cdn.example.com"),
                None,
            ),
            (
                "splithttp",
                "xhttp",
                Some("/split"),
                Some("cdn.example.com"),
                None,
            ),
            ("grpc", "grpc", None, None, Some("blackwire grpc")),
        ];
        for (transport, exported, path, host, service) in cases {
            let mut value = record("vless");
            value.transport = transport.into();
            value.transport_path = path.map(str::to_owned);
            value.transport_host = host.map(str::to_owned);
            value.grpc_service_name = service.map(str::to_owned);
            let parsed = Url::parse(&link(&value)).expect("valid VLESS transport URI");
            let params = query(&parsed);
            assert_eq!(params.get("type").map(String::as_str), Some(exported));
            assert_eq!(params.get("path").map(String::as_str), path);
            assert_eq!(params.get("host").map(String::as_str), host);
            assert_eq!(params.get("serviceName").map(String::as_str), service);
        }
    }

    #[test]
    fn hiddify_vmess_payload_contains_all_supported_fields() {
        let mut value = record("vmess");
        value.transport = "grpc".into();
        value.grpc_service_name = Some("blackwire-grpc".into());
        value.transport_host = Some("edge.example.com".into());
        let exported = link(&value);
        let encoded = exported.strip_prefix("vmess://").unwrap();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(payload["v"], "2");
        assert_eq!(payload["add"], "203.0.113.7");
        assert_eq!(payload["port"], "443");
        assert_eq!(payload["id"], value.uuid.as_deref().unwrap());
        assert_eq!(payload["net"], "grpc");
        assert_eq!(payload["type"], "gun");
        assert_eq!(payload["path"], "blackwire-grpc");
        assert_eq!(payload["host"], "edge.example.com");
        assert_eq!(payload["tls"], "tls");
        assert_eq!(payload["sni"], "cover.example.com");
        assert_eq!(payload["ps"], value.email);
    }

    #[test]
    fn hiddify_trojan_hysteria2_and_tuic_links_are_parseable() {
        let trojan = Url::parse(&link(&record("trojan"))).unwrap();
        assert_eq!(trojan.username(), "p%40ss%3Aword%2F%2B");
        assert_eq!(
            query(&trojan).get("sni").map(String::as_str),
            Some("cover.example.com")
        );
        assert_eq!(query(&trojan).get("type").map(String::as_str), Some("tcp"));

        let hysteria = Url::parse(&link(&record("hysteria2"))).unwrap();
        assert_eq!(hysteria.username(), "auth%3Auser%2Fvalue");
        assert_eq!(hysteria.path(), "/");
        assert_eq!(
            query(&hysteria).get("sni").map(String::as_str),
            Some("cover.example.com")
        );
        assert!(!query(&hysteria).contains_key("insecure"));

        let tuic = Url::parse(&link(&record("tuic"))).unwrap();
        let params = query(&tuic);
        assert_eq!(tuic.username(), "1791a4cd-09e3-4a29-a36d-fea98300c845");
        assert_eq!(tuic.password(), Some("p%40ss%3Aword%2F%2B"));
        assert_eq!(
            params.get("sni").map(String::as_str),
            Some("cover.example.com")
        );
        assert_eq!(params.get("alpn").map(String::as_str), Some("h3"));
        assert_eq!(
            params.get("congestion_control").map(String::as_str),
            Some("bbr")
        );
        assert_eq!(
            params.get("udp_relay_mode").map(String::as_str),
            Some("native")
        );

        let mut insecure_hysteria = record("hysteria2");
        insecure_hysteria.security = "none".into();
        assert_eq!(
            query(&Url::parse(&link(&insecure_hysteria)).unwrap())
                .get("insecure")
                .map(String::as_str),
            Some("1")
        );

        let mut insecure_tuic = record("tuic");
        insecure_tuic.security = "none".into();
        assert_eq!(
            query(&Url::parse(&link(&insecure_tuic)).unwrap())
                .get("allow_insecure")
                .map(String::as_str),
            Some("1")
        );
    }

    #[test]
    fn hiddify_shadowsocks_uses_unpadded_url_safe_sip002_credentials() {
        let value = record("shadowsocks");
        let parsed = Url::parse(&link(&value)).expect("valid Shadowsocks URI");
        assert!(!parsed.username().contains(['+', '/', '=']));
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parsed.username())
            .unwrap();
        assert_eq!(
            String::from_utf8(decoded).unwrap(),
            "2022-blake3-aes-256-gcm:p@ss:word/+"
        );
    }

    #[test]
    fn subscription_export_encodes_labels_ipv6_and_base64_envelope() {
        let value = record("vless");
        let exported = build_subscription_link(&value, "2001:db8::7").unwrap();
        let parsed = Url::parse(&exported).unwrap();
        assert_eq!(parsed.host_str(), Some("[2001:db8::7]"));
        assert_eq!(
            parsed.fragment(),
            Some("Alice%20%2B%20mobile%40example.com")
        );
        let envelope = encode_subscription_content(&exported);
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(envelope)
                .unwrap(),
            exported.as_bytes()
        );
        assert!(build_subscription_link(&record("unsupported"), "example.com").is_none());
    }
}
