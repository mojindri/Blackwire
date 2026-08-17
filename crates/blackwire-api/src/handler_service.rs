//! Read-only runtime inspection and connection operations.
//!
//! ## Supported operations
//! - `ListInbounds` — returns inbound tags
//! - `ListOutbounds` — returns outbound tags
//! - `GetInboundUsersCount` — count of VLESS users on a named inbound
//! - `GetInboundUsers` — list VLESS users on a named inbound
//! Configuration mutations are rejected. MySQL revisions are the only
//! configuration write path.

use prost::Message;
use tonic::{Request, Response, Status};

use crate::handler_proto::handler_service_server::HandlerService;
use crate::handler_proto::{
    AddInboundRequest, AddInboundResponse, AddOutboundRequest, AddOutboundResponse,
    AlterInboundRequest, AlterInboundResponse, AlterOutboundRequest,
    AlterOutboundResponse, CloseConnectionsRequest, CloseConnectionsResponse, ConnectionEntry,
    GetInboundUserRequest, GetInboundUserResponse, GetInboundUsersCountResponse,
    InboundHandlerConfig, ListConnectionsRequest, ListConnectionsResponse, ListInboundsRequest,
    ListInboundsResponse, ListOutboundsRequest, ListOutboundsResponse, OutboundHandlerConfig,
    RemoveInboundRequest, RemoveInboundResponse, RemoveOutboundRequest, RemoveOutboundResponse,
    TypedMessage, User,
};
use crate::management::ManagementHandle;
use crate::vless_account_proto::Account;

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
        _request: Request<AlterInboundRequest>,
    ) -> Result<Response<AlterInboundResponse>, Status> {
        Err(Status::failed_precondition("configuration mutations must be committed as MySQL revisions"))
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
        _request: Request<AddInboundRequest>,
    ) -> Result<Response<AddInboundResponse>, Status> {
        Err(Status::failed_precondition("configuration mutations must be committed as MySQL revisions"))
    }

    async fn remove_inbound(
        &self,
        _request: Request<RemoveInboundRequest>,
    ) -> Result<Response<RemoveInboundResponse>, Status> {
        Err(Status::failed_precondition("configuration mutations must be committed as MySQL revisions"))
    }

    async fn add_outbound(
        &self,
        _request: Request<AddOutboundRequest>,
    ) -> Result<Response<AddOutboundResponse>, Status> {
        Err(Status::failed_precondition("configuration mutations must be committed as MySQL revisions"))
    }

    async fn remove_outbound(
        &self,
        _request: Request<RemoveOutboundRequest>,
    ) -> Result<Response<RemoveOutboundResponse>, Status> {
        Err(Status::failed_precondition("configuration mutations must be committed as MySQL revisions"))
    }

    async fn alter_outbound(
        &self,
        _request: Request<AlterOutboundRequest>,
    ) -> Result<Response<AlterOutboundResponse>, Status> {
        Err(Status::failed_precondition("configuration mutations must be committed as MySQL revisions"))
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
