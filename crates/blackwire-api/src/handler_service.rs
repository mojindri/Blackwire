//! `HandlerService` gRPC — inbound/outbound tag listing and VLESS user management.
//!
//! ## Supported operations
//! - `ListInbounds` — returns inbound tags
//! - `ListOutbounds` — returns outbound tags
//! - `GetInboundUsersCount` — count of VLESS users on a named inbound
//! - `GetInboundUsers` — list VLESS users on a named inbound
//! - `AlterInbound` — add or remove a VLESS user via `AddUserOperation` / `RemoveUserOperation`
//! - `AddInbound` / `RemoveInbound` — structural runtime changes through the
//!   CLI control-plane handle
//! - `AddOutbound` / `RemoveOutbound` / `AlterOutbound` — structural runtime
//!   changes through the CLI control-plane handle

use prost::Message;
use tonic::{Request, Response, Status};

use crate::handler_proto::handler_service_server::HandlerService;
use crate::handler_proto::{
    AddInboundRequest, AddInboundResponse, AddOutboundRequest, AddOutboundResponse,
    AddUserOperation, AlterInboundRequest, AlterInboundResponse, AlterOutboundRequest,
    AlterOutboundResponse, CloseConnectionsRequest, CloseConnectionsResponse, ConnectionEntry,
    GetInboundUserRequest, GetInboundUserResponse, GetInboundUsersCountResponse,
    InboundHandlerConfig, ListConnectionsRequest, ListConnectionsResponse, ListInboundsRequest,
    ListInboundsResponse, ListOutboundsRequest, ListOutboundsResponse, OutboundHandlerConfig,
    RemoveInboundRequest, RemoveInboundResponse, RemoveOutboundRequest, RemoveOutboundResponse,
    RemoveUserOperation, TypedMessage, User,
};
use crate::management::ManagementHandle;
use crate::management::NativeEndpointConfig;
use crate::vless_account_proto::Account;

const ADD_USER_TYPE: &str = "xray.app.proxyman.command.AddUserOperation";
const REMOVE_USER_TYPE: &str = "xray.app.proxyman.command.RemoveUserOperation";

/// HandlerService backed by [`ManagementHandle`].
pub struct HandlerServiceImpl {
    management: ManagementHandle,
}

impl HandlerServiceImpl {
    /// Create a service using the shared runtime management handle.
    pub fn new(management: ManagementHandle) -> Self {
        Self { management }
    }
}

fn parse_vless_uuid_from_user(user: &User) -> Result<String, String> {
    let account = user
        .account
        .as_ref()
        .ok_or_else(|| "user.account is required for VLESS AddUser".to_string())?;
    if let Ok(acc) = Account::decode(account.value.as_slice()) {
        if !acc.id.is_empty() {
            return Ok(acc.id);
        }
    }
    if let Ok(text) = std::str::from_utf8(&account.value) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    Err("could not parse VLESS UUID from user.account".into())
}

fn endpoint_json_from_typed_message(
    tag: &str,
    typed: Option<TypedMessage>,
    kind: &str,
) -> Result<NativeEndpointConfig, Status> {
    let typed = typed.ok_or_else(|| {
        Status::invalid_argument(format!(
            "{kind} config must include proxy_settings containing native blackwire JSON"
        ))
    })?;
    let raw = std::str::from_utf8(&typed.value)
        .map_err(|e| Status::invalid_argument(format!("{kind} config is not UTF-8 JSON: {e}")))?;
    let mut value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| Status::invalid_argument(format!("{kind} config JSON decode: {e}")))?;
    let obj = value
        .as_object_mut()
        .ok_or_else(|| Status::invalid_argument(format!("{kind} config JSON must be an object")))?;

    match obj.get("tag").and_then(|v| v.as_str()) {
        Some(existing) if !tag.is_empty() && existing != tag => Err(Status::invalid_argument(
            format!("{kind} tag mismatch: request tag '{tag}' != JSON tag '{existing}'"),
        )),
        Some(existing) => Ok(NativeEndpointConfig {
            tag: existing.to_string(),
            config: value,
        }),
        None if !tag.is_empty() => {
            obj.insert("tag".into(), serde_json::Value::String(tag.to_string()));
            Ok(NativeEndpointConfig {
                tag: tag.to_string(),
                config: value,
            })
        }
        None => Err(Status::invalid_argument(format!(
            "{kind} config JSON must include tag when request tag is empty"
        ))),
    }
}

fn native_inbound_config(cfg: InboundHandlerConfig) -> Result<NativeEndpointConfig, Status> {
    endpoint_json_from_typed_message(&cfg.tag, cfg.proxy_settings, "inbound")
}

fn native_outbound_config(cfg: OutboundHandlerConfig) -> Result<NativeEndpointConfig, Status> {
    endpoint_json_from_typed_message(&cfg.tag, cfg.proxy_settings, "outbound")
}

#[tonic::async_trait]
impl HandlerService for HandlerServiceImpl {
    async fn list_inbounds(
        &self,
        request: Request<ListInboundsRequest>,
    ) -> Result<Response<ListInboundsResponse>, Status> {
        let _only_tags = request.into_inner().is_only_tags;
        let inbounds = self
            .management
            .list_inbound_tags()
            .await
            .into_iter()
            .map(|tag| InboundHandlerConfig {
                tag,
                receiver_settings: None,
                proxy_settings: None,
            })
            .collect();
        Ok(Response::new(ListInboundsResponse { inbounds }))
    }

    async fn get_inbound_users_count(
        &self,
        request: Request<GetInboundUserRequest>,
    ) -> Result<Response<GetInboundUsersCountResponse>, Status> {
        let req = request.into_inner();
        let count = self
            .management
            .vless_user_count(&req.tag)
            .await
            .ok_or_else(|| Status::not_found(format!("inbound '{}' not found", req.tag)))?;
        Ok(Response::new(GetInboundUsersCountResponse { count }))
    }

    async fn get_inbound_users(
        &self,
        request: Request<GetInboundUserRequest>,
    ) -> Result<Response<GetInboundUserResponse>, Status> {
        let req = request.into_inner();
        let records = self
            .management
            .list_vless_users(&req.tag, &req.email)
            .await
            .map_err(Status::failed_precondition)?;
        let users = records
            .into_iter()
            .map(|r| {
                let account_bytes = Account {
                    id: r.uuid,
                    flow: r.flow,
                    encryption: String::new(),
                }
                .encode_to_vec();
                User {
                    level: r.level,
                    email: r.email,
                    account: Some(TypedMessage {
                        r#type: "xray.proxy.vless.Account".into(),
                        value: account_bytes,
                    }),
                }
            })
            .collect();
        Ok(Response::new(GetInboundUserResponse { users }))
    }

    async fn alter_inbound(
        &self,
        request: Request<AlterInboundRequest>,
    ) -> Result<Response<AlterInboundResponse>, Status> {
        let req = request.into_inner();
        let op = req
            .operation
            .ok_or_else(|| Status::invalid_argument("operation is required"))?;
        let tag = req.tag;

        if op.r#type == ADD_USER_TYPE || op.r#type.ends_with("AddUserOperation") {
            let add = AddUserOperation::decode(op.value.as_slice())
                .map_err(|e| Status::invalid_argument(format!("AddUserOperation decode: {e}")))?;
            let user = add
                .user
                .ok_or_else(|| Status::invalid_argument("AddUserOperation.user is required"))?;
            let email = user.email.clone();
            let flow = user
                .account
                .as_ref()
                .and_then(|a| Account::decode(a.value.as_slice()).ok())
                .map(|a| a.flow)
                .unwrap_or_default();
            let uuid = parse_vless_uuid_from_user(&user).map_err(Status::invalid_argument)?;
            self.management
                .add_vless_user(&tag, &email, &uuid, &flow)
                .await
                .map_err(Status::failed_precondition)?;
            return Ok(Response::new(AlterInboundResponse {}));
        }

        if op.r#type == REMOVE_USER_TYPE || op.r#type.ends_with("RemoveUserOperation") {
            let remove = RemoveUserOperation::decode(op.value.as_slice()).map_err(|e| {
                Status::invalid_argument(format!("RemoveUserOperation decode: {e}"))
            })?;
            self.management
                .remove_vless_user(&tag, &remove.email)
                .await
                .map_err(Status::not_found)?;
            return Ok(Response::new(AlterInboundResponse {}));
        }

        Err(Status::unimplemented(format!(
            "unsupported AlterInbound operation type '{}'",
            op.r#type
        )))
    }

    async fn list_outbounds(
        &self,
        _request: Request<ListOutboundsRequest>,
    ) -> Result<Response<ListOutboundsResponse>, Status> {
        let outbounds = self
            .management
            .list_outbound_tags()
            .await
            .into_iter()
            .map(|tag| OutboundHandlerConfig {
                tag,
                sender_settings: None,
                proxy_settings: None,
                expire: 0,
                comment: String::new(),
            })
            .collect();
        Ok(Response::new(ListOutboundsResponse { outbounds }))
    }

    async fn add_inbound(
        &self,
        request: Request<AddInboundRequest>,
    ) -> Result<Response<AddInboundResponse>, Status> {
        let req = request.into_inner();
        let cfg = req
            .inbound
            .ok_or_else(|| Status::invalid_argument("AddInboundRequest.inbound is required"))
            .and_then(native_inbound_config)?;
        self.management
            .add_inbound(cfg)
            .await
            .map_err(Status::failed_precondition)?;
        Ok(Response::new(AddInboundResponse {}))
    }

    async fn remove_inbound(
        &self,
        request: Request<RemoveInboundRequest>,
    ) -> Result<Response<RemoveInboundResponse>, Status> {
        let req = request.into_inner();
        if req.tag.is_empty() {
            return Err(Status::invalid_argument(
                "RemoveInboundRequest.tag is required",
            ));
        }
        self.management
            .remove_inbound(&req.tag)
            .await
            .map_err(Status::not_found)?;
        Ok(Response::new(RemoveInboundResponse {}))
    }

    async fn add_outbound(
        &self,
        request: Request<AddOutboundRequest>,
    ) -> Result<Response<AddOutboundResponse>, Status> {
        let req = request.into_inner();
        let cfg = req
            .outbound
            .ok_or_else(|| Status::invalid_argument("AddOutboundRequest.outbound is required"))
            .and_then(native_outbound_config)?;
        self.management
            .add_outbound(cfg)
            .await
            .map_err(Status::failed_precondition)?;
        Ok(Response::new(AddOutboundResponse {}))
    }

    async fn remove_outbound(
        &self,
        request: Request<RemoveOutboundRequest>,
    ) -> Result<Response<RemoveOutboundResponse>, Status> {
        let req = request.into_inner();
        if req.tag.is_empty() {
            return Err(Status::invalid_argument(
                "RemoveOutboundRequest.tag is required",
            ));
        }
        self.management
            .remove_outbound(&req.tag)
            .await
            .map_err(Status::not_found)?;
        Ok(Response::new(RemoveOutboundResponse {}))
    }

    async fn alter_outbound(
        &self,
        request: Request<AlterOutboundRequest>,
    ) -> Result<Response<AlterOutboundResponse>, Status> {
        let req = request.into_inner();
        if req.tag.is_empty() {
            return Err(Status::invalid_argument(
                "AlterOutboundRequest.tag is required",
            ));
        }
        let cfg = endpoint_json_from_typed_message(&req.tag, req.operation, "outbound")?;
        self.management
            .alter_outbound(cfg)
            .await
            .map_err(Status::failed_precondition)?;
        Ok(Response::new(AlterOutboundResponse {}))
    }

    async fn list_connections(
        &self,
        _request: Request<ListConnectionsRequest>,
    ) -> Result<Response<ListConnectionsResponse>, Status> {
        let connections = self
            .management
            .list_connections()
            .await
            .into_iter()
            .map(|snapshot| ConnectionEntry {
                id: snapshot.id,
                inbound: snapshot.inbound,
                outbound: snapshot.outbound,
                user: snapshot.user.unwrap_or_default(),
                protocol: snapshot.protocol.as_str().to_string(),
                transport: snapshot.transport.as_str().to_string(),
                age_seconds: snapshot.age_secs,
                bytes_up: snapshot.bytes_up,
                bytes_down: snapshot.bytes_down,
                relay_path: snapshot.relay_path.as_str().to_string(),
                close_reason: snapshot.close_reason.as_str().to_string(),
            })
            .collect();
        Ok(Response::new(ListConnectionsResponse { connections }))
    }

    async fn close_connections(
        &self,
        request: Request<CloseConnectionsRequest>,
    ) -> Result<Response<CloseConnectionsResponse>, Status> {
        let req = request.into_inner();
        let selector = match req.selector {
            Some(crate::handler_proto::close_connections_request::Selector::Id(id)) => {
                blackwire_connmgr::CloseSelector::Id(id)
            }
            Some(crate::handler_proto::close_connections_request::Selector::User(user)) => {
                blackwire_connmgr::CloseSelector::User(user)
            }
            Some(crate::handler_proto::close_connections_request::Selector::Inbound(inbound)) => {
                blackwire_connmgr::CloseSelector::Inbound(inbound)
            }
            Some(crate::handler_proto::close_connections_request::Selector::Outbound(outbound)) => {
                blackwire_connmgr::CloseSelector::Outbound(outbound)
            }
            None => {
                return Err(Status::invalid_argument(
                    "CloseConnections requires one selector",
                ));
            }
        };
        let matched = self
            .management
            .close_connections(selector)
            .await
            .map_err(Status::failed_precondition)?;
        Ok(Response::new(CloseConnectionsResponse {
            matched: matched as u64,
        }))
    }
}
