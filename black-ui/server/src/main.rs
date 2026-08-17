//! Black UI server — management panel backend for Blackwire.

mod app;
mod capabilities;
mod control_handlers;
mod error;
mod models;
mod mysql_auth;
mod mysql_state;
mod service;
mod util;

use anyhow::Result;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let state = mysql_state::AppState::open().await?;

    let addr: SocketAddr = std::env::var("BLACK_UI_LISTEN")
        .unwrap_or_else(|_| "127.0.0.1:18080".into())
        .parse()?;
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "black-ui server listening");

    axum::serve(listener, app::router(state)).await?;
    Ok(())
}
