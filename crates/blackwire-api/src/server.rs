//! gRPC server wiring for Stats + Handler services.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use tokio::task::JoinHandle;
use tonic::transport::Server;
use tonic::{Request, Status};
use tracing::{error, info};

use crate::handler_proto::handler_service_server::HandlerServiceServer;
use crate::handler_service::HandlerServiceImpl;
use crate::management::ManagementHandle;
use crate::proto::stats_service_server::StatsServiceServer;
use crate::stats_service::StatsServiceImpl;

/// Parsed `api` listener settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiServerConfig {
    /// Address the gRPC API should bind to.
    pub listen_addr: String,
    /// Optional bearer/token value required by API requests.
    pub token: Option<String>,
}

/// Parse `api` listener settings from config (`"host:port"` string or object).
pub fn api_server_config(api: &blackwire_config::schema::ApiConfig) -> Option<ApiServerConfig> {
    (!api.listen.trim().is_empty()).then(|| ApiServerConfig {
        listen_addr: api.listen.clone(),
        token: api.token.clone(),
    })
}

/// Parse `api` listen address from config (`"host:port"` string or object).
pub fn api_listen_addr(api: &blackwire_config::schema::ApiConfig) -> Option<String> {
    api_server_config(api).map(|config| config.listen_addr)
}

/// Spawn the combined Stats + Handler gRPC server on `addr`.
pub fn start_api_server(
    addr: &str,
    management: ManagementHandle,
    token: Option<String>,
) -> anyhow::Result<JoinHandle<()>> {
    let addr: SocketAddr = addr
        .parse()
        .with_context(|| format!("invalid API listen address '{addr}'"))?;
    let allow_unauthenticated_loopback = token.is_none() && addr.ip().is_loopback();
    let token = Arc::new(token);
    let task = tokio::spawn(async move {
        info!(addr = %addr, authenticated = token.is_some(), loopback_only = allow_unauthenticated_loopback, "blackwire-api gRPC server starting");
        let stats_token = Arc::clone(&token);
        let handler_token = Arc::clone(&token);
        if let Err(e) = Server::builder()
            .add_service(StatsServiceServer::with_interceptor(
                StatsServiceImpl,
                move |request| {
                    authorize_api_request(
                        request,
                        stats_token.as_deref(),
                        allow_unauthenticated_loopback,
                    )
                },
            ))
            .add_service(HandlerServiceServer::with_interceptor(
                HandlerServiceImpl::new(management),
                move |request| {
                    authorize_api_request(
                        request,
                        handler_token.as_deref(),
                        allow_unauthenticated_loopback,
                    )
                },
            ))
            .serve(addr)
            .await
        {
            error!(error = %e, "blackwire-api gRPC server failed");
        }
    });
    Ok(task)
}

fn authorize_api_request<T>(
    request: Request<T>,
    token: Option<&str>,
    allow_unauthenticated_loopback: bool,
) -> Result<Request<T>, Status> {
    if allow_unauthenticated_loopback {
        return Ok(request);
    }

    let Some(token) = token else {
        return Err(Status::unauthenticated(
            "blackwire-api requires api.token unless bound to a loopback address",
        ));
    };

    let metadata = request.metadata();
    let bearer = format!("Bearer {token}");
    let authorized = metadata
        .get("x-blackwire-api-token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == token)
        || metadata
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == bearer);

    if authorized {
        Ok(request)
    } else {
        Err(Status::unauthenticated(
            "missing or invalid blackwire-api credentials",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_api_token_from_object_config() {
        let config = api_server_config(&blackwire_config::schema::ApiConfig {
            listen: "0.0.0.0:9000".into(),
            token: Some("secret".into()),
            services: Vec::new(),
        })
        .expect("api config");

        assert_eq!(config.listen_addr, "0.0.0.0:9000");
        assert_eq!(config.token.as_deref(), Some("secret"));
    }

    #[test]
    fn rejects_remote_unauthenticated_request_without_token() {
        let err = authorize_api_request(Request::new(()), None, false).unwrap_err();

        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn accepts_matching_api_token_metadata() {
        let mut request = Request::new(());
        request
            .metadata_mut()
            .insert("x-blackwire-api-token", "secret".parse().unwrap());

        assert!(authorize_api_request(request, Some("secret"), false).is_ok());
    }
}
