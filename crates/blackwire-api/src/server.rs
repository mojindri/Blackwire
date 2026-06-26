//! gRPC server wiring for Stats + Handler services.

use std::net::SocketAddr;

use anyhow::Context;
use serde_json::Value;
use tokio::task::JoinHandle;
use tonic::transport::Server;
use tracing::{error, info};

use crate::handler_proto::handler_service_server::HandlerServiceServer;
use crate::handler_service::HandlerServiceImpl;
use crate::management::ManagementHandle;
use crate::proto::stats_service_server::StatsServiceServer;
use crate::stats_service::StatsServiceImpl;

/// Parse `api` listen address from config (`"host:port"` string or object).
pub fn api_listen_addr(api: &Value) -> Option<String> {
    if let Some(addr) = api.as_str() {
        return Some(addr.to_string());
    }
    api.get("listen")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            let host = api.get("host").and_then(Value::as_str)?;
            let port = api.get("port").and_then(Value::as_u64)?;
            Some(format!("{host}:{port}"))
        })
}

/// Spawn the combined Stats + Handler gRPC server on `addr`.
pub fn start_api_server(
    addr: &str,
    management: ManagementHandle,
) -> anyhow::Result<JoinHandle<()>> {
    let addr: SocketAddr = addr
        .parse()
        .with_context(|| format!("invalid API listen address '{addr}'"))?;
    anyhow::ensure!(
        addr.ip().is_loopback(),
        "blackwire-api gRPC management server must listen on a loopback address; refusing '{addr}'"
    );
    let task = tokio::spawn(async move {
        info!(addr = %addr, "blackwire-api gRPC server starting");
        if let Err(e) = Server::builder()
            .add_service(StatsServiceServer::new(StatsServiceImpl))
            .add_service(HandlerServiceServer::new(HandlerServiceImpl::new(
                management,
            )))
            .serve(addr)
            .await
        {
            error!(error = %e, "blackwire-api gRPC server failed");
        }
    });
    Ok(task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct NoopManagement;

    #[async_trait]
    impl crate::management::InboundManagement for NoopManagement {
        async fn list_inbound_tags(&self) -> Vec<String> {
            Vec::new()
        }

        async fn list_outbound_tags(&self) -> Vec<String> {
            Vec::new()
        }

        async fn vless_user_count(&self, _inbound_tag: &str) -> Option<i64> {
            None
        }

        async fn list_vless_users(
            &self,
            _inbound_tag: &str,
            _email: &str,
        ) -> Result<Vec<crate::management::VlessUserRecord>, String> {
            Ok(Vec::new())
        }

        async fn add_vless_user(
            &self,
            _inbound_tag: &str,
            _email: &str,
            _uuid: &str,
            _flow: &str,
        ) -> Result<(), String> {
            Ok(())
        }

        async fn remove_vless_user(&self, _inbound_tag: &str, _email: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn start_api_server_rejects_non_loopback_listeners() {
        let management: ManagementHandle = std::sync::Arc::new(NoopManagement);
        let err = start_api_server("0.0.0.0:10085", management).unwrap_err();
        assert!(
            err.to_string()
                .contains("must listen on a loopback address"),
            "unexpected error: {err:#}"
        );
    }

    #[tokio::test]
    async fn start_api_server_allows_loopback_listeners() {
        let management: ManagementHandle = std::sync::Arc::new(NoopManagement);
        let handle = start_api_server("127.0.0.1:0", management).unwrap();
        handle.abort();
    }
}
