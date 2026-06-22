//! Black UI server — management panel backend for Blackwire.

mod app;
mod auth;
mod autotune;
mod capabilities;
mod config;
mod db;
mod enforcement;
mod error;
mod firewall;
mod handlers;
mod models;
mod runtime;
mod service;
mod state;
mod util;

use anyhow::Result;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let state = state::AppState::open()?;
    if let Err(e) = enforcement::run_startup_once(&state).await {
        warn!(error = %e, "startup quota/expiry enforcement failed");
    }
    if let Err(e) = autotune::run_startup_once(&state).await {
        warn!(error = %e, "startup adaptive tuning failed");
    }
    enforcement::spawn(state.clone());
    autotune::spawn(state.clone());

    let addr: SocketAddr = std::env::var("BLACK_UI_LISTEN")
        .unwrap_or_else(|_| "127.0.0.1:18080".into())
        .parse()?;
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "black-ui server listening");

    axum::serve(listener, app::router(state)).await?;
    Ok(())
}
