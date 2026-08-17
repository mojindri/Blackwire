use axum::{
    extract::{Path, State},
    http::{header, HeaderMap},
    response::IntoResponse,
    Json,
};
use blackwire_store::{InboundWrite, OutboundWrite, PanelSettings, UserWrite};
use chrono::{DateTime, Utc};
use serde_json::json;

use crate::{
    capabilities,
    error::{ApiResult, AppError},
    models::{
        ApplyResult, Inbound, InboundInput, LoginInput, LoginResponse, ManagedUser,
        Outbound, OutboundInput, ServiceStatus, Settings, SetupInput, Status, TrafficSnapshot,
        UserInput,
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
        Ok((token, username)) => ([(header::SET_COOKIE, auth::session_cookie(&token))], Json(LoginResponse { token, username })).into_response(),
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

pub async fn me(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<serde_json::Value> {
    let admin_id = auth::require(&headers, &state).await?;
    let username = state
        .store
        .admin_username(admin_id)
        .await
        .map_err(store_error)?
        .ok_or_else(AppError::unauthorized)?;
    Ok(Json(json!({ "username": username })))
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
    let inbounds = state.store.list_inbounds(config.desired_revision).await.map_err(store_error)?;
    let outbounds = state.store.list_outbounds(config.desired_revision).await.map_err(store_error)?;
    let users = state.store.list_users(config.desired_revision).await.map_err(store_error)?;
    Ok(Json(Status {
        setup_required,
        database_connected: true,
        schema_version: blackwire_store::EXPECTED_SCHEMA_VERSION,
        desired_revision: config.desired_revision,
        active_revision: config.active_revision,
        pending_maintenance_revision: config.pending_maintenance_revision,
        activation_state: config.activation_state,
        last_activation_error: config.last_error,
        grpc_reachable: false,
        inbounds: inbounds.len(),
        outbounds: outbounds.len(),
        users: users.len(),
        active_users: users.iter().filter(|user| user.enabled).count(),
    }))
}

pub async fn get_settings(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Settings> {
    auth::require(&headers, &state).await?;
    Ok(Json(state.store.panel_settings().await.map_err(store_error)?.into()))
}

pub async fn update_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(settings): Json<Settings>,
) -> ApiResult<Settings> {
    auth::require(&headers, &state).await?;
    let stored: PanelSettings = settings.into();
    state.store.save_panel_settings(&stored).await.map_err(store_error)?;
    Ok(Json(stored.into()))
}

pub async fn list_inbounds(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Vec<Inbound>> {
    auth::require(&headers, &state).await?;
    let revision = state.store.state().await.map_err(store_error)?.desired_revision;
    let records = state.store.list_inbounds(revision).await.map_err(store_error)?;
    Ok(Json(records.into_iter().map(Inbound::from).collect()))
}

pub async fn create_inbound(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<InboundInput>) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    validate_inbound(&input)?;
    Ok(Json(state.store.save_inbound("black-ui", inbound_write(None, input)).await.map_err(store_error)?.into()))
}

pub async fn update_inbound(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<i64>, Json(input): Json<InboundInput>) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    validate_inbound(&input)?;
    Ok(Json(state.store.save_inbound("black-ui", inbound_write(Some(id), input)).await.map_err(store_error)?.into()))
}

pub async fn delete_inbound(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<i64>) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    Ok(Json(state.store.delete_inbound("black-ui", id).await.map_err(store_error)?.into()))
}

pub async fn list_outbounds(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Vec<Outbound>> {
    auth::require(&headers, &state).await?;
    let revision = state.store.state().await.map_err(store_error)?.desired_revision;
    let records = state.store.list_outbounds(revision).await.map_err(store_error)?;
    Ok(Json(records.into_iter().map(Outbound::from).collect()))
}

pub async fn create_outbound(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<OutboundInput>) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    validate_tag(&input.tag)?;
    Ok(Json(state.store.save_outbound("black-ui", outbound_write(None, input)).await.map_err(store_error)?.into()))
}

pub async fn update_outbound(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<i64>, Json(input): Json<OutboundInput>) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    validate_tag(&input.tag)?;
    Ok(Json(state.store.save_outbound("black-ui", outbound_write(Some(id), input)).await.map_err(store_error)?.into()))
}

pub async fn delete_outbound(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<i64>) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    Ok(Json(state.store.delete_outbound("black-ui", id).await.map_err(store_error)?.into()))
}

pub async fn list_users(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Vec<ManagedUser>> {
    auth::require(&headers, &state).await?;
    let revision = state.store.state().await.map_err(store_error)?.desired_revision;
    let records = state.store.list_users(revision).await.map_err(store_error)?;
    Ok(Json(records.into_iter().map(ManagedUser::from).collect()))
}

pub async fn create_user(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<UserInput>) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    Ok(Json(state.store.save_user("black-ui", user_write(None, input)?).await.map_err(store_error)?.into()))
}

pub async fn update_user(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<i64>, Json(input): Json<UserInput>) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    Ok(Json(state.store.save_user("black-ui", user_write(Some(id), input)?).await.map_err(store_error)?.into()))
}

pub async fn delete_user(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<i64>) -> ApiResult<ApplyResult> {
    auth::require(&headers, &state).await?;
    Ok(Json(state.store.delete_user("black-ui", id).await.map_err(store_error)?.into()))
}

pub async fn enable_user(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<i64>) -> ApiResult<ApplyResult> {
    set_user_enabled(&state, &headers, id, true).await
}

pub async fn disable_user(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<i64>) -> ApiResult<ApplyResult> {
    set_user_enabled(&state, &headers, id, false).await
}

async fn set_user_enabled(state: &AppState, headers: &HeaderMap, id: i64, enabled: bool) -> ApiResult<ApplyResult> {
    auth::require(headers, state).await?;
    let revision = state.store.state().await.map_err(store_error)?.desired_revision;
    let user = state.store.list_users(revision).await.map_err(store_error)?.into_iter().find(|user| user.id == id).ok_or_else(|| AppError::not_found("user not found"))?;
    let input = UserWrite { id: Some(id), inbound_id: user.inbound_id, email: user.email, enabled, flow: user.flow, note: user.note, traffic_limit_bytes: user.traffic_limit_bytes, expiry_at: user.expiry_at, subscription_token: user.subscription_token, credential_kind: user.credential_kind, uuid: user.uuid, password: None, method: user.method, auth: None };
    Ok(Json(state.store.save_user("black-ui", input).await.map_err(store_error)?.into()))
}

pub async fn generate_uuid(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<serde_json::Value> {
    auth::require(&headers, &state).await?;
    Ok(Json(json!({ "uuid": uuid::Uuid::new_v4().to_string() })))
}

pub async fn runtime_traffic(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<TrafficSnapshot> {
    auth::require(&headers, &state).await?;
    Ok(Json(TrafficSnapshot { users: Vec::new(), inbounds: Vec::new() }))
}

pub async fn service_status(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<ServiceStatus> {
    auth::require(&headers, &state).await?;
    Ok(Json(service::blackwire_status()))
}

pub async fn service_restart_blackwire(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<ServiceStatus> {
    auth::require(&headers, &state).await?;
    Ok(Json(service::restart_blackwire().map_err(AppError::internal)?))
}

pub async fn service_logs(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Vec<String>> {
    auth::require(&headers, &state).await?;
    Ok(Json(service::recent_logs()))
}

pub async fn subscription_base64(Path(_token): Path<String>) -> Result<String, AppError> {
    Err(AppError::not_found("subscription not found"))
}

pub async fn subscription_raw(Path(_token): Path<String>) -> Result<String, AppError> {
    Err(AppError::not_found("subscription not found"))
}

fn inbound_write(id: Option<i64>, input: InboundInput) -> InboundWrite {
    InboundWrite { id, tag: input.tag, listen: input.listen, port: input.port, protocol: input.protocol, enabled: input.enabled, transport: input.transport, security: input.security }
}

fn outbound_write(id: Option<i64>, input: OutboundInput) -> OutboundWrite {
    OutboundWrite { id, tag: input.tag, protocol: input.protocol, enabled: input.enabled, address: input.address, port: input.port, transport: input.transport, security: input.security }
}

fn user_write(id: Option<i64>, input: UserInput) -> Result<UserWrite, AppError> {
    if input.email.trim().is_empty() { return Err(AppError::bad_request("email is required")); }
    let expiry_at = input.expiry_at.as_deref().map(DateTime::parse_from_rfc3339).transpose().map_err(|_| AppError::bad_request("expiry must be RFC 3339"))?.map(|value| value.with_timezone(&Utc));
    Ok(UserWrite { id, inbound_id: input.inbound_id, email: input.email, enabled: input.enabled, flow: input.flow.unwrap_or_default(), note: input.note.unwrap_or_default(), traffic_limit_bytes: input.traffic_limit_bytes, expiry_at, subscription_token: input.subscription_token.unwrap_or_else(|| util::random_token(24)), credential_kind: input.credential_kind.unwrap_or_else(|| "uuid".into()), uuid: Some(input.uuid), password: input.password, method: input.method, auth: input.auth })
}

fn validate_inbound(input: &InboundInput) -> Result<(), AppError> {
    validate_tag(&input.tag)?;
    if input.port == 0 { return Err(AppError::bad_request("port must be between 1 and 65535")); }
    Ok(())
}

fn validate_tag(tag: &str) -> Result<(), AppError> {
    if tag.trim().is_empty() { Err(AppError::bad_request("tag is required")) } else { Ok(()) }
}

fn store_error(error: blackwire_store::StoreError) -> AppError { AppError::internal(error.into()) }

impl From<blackwire_store::MutationResult> for ApplyResult {
    fn from(value: blackwire_store::MutationResult) -> Self {
        Self { revision: value.revision, parent_revision: value.parent_revision, active_revision: value.active_revision, state: value.state, activation_class: value.activation_class, message: value.message }
    }
}
