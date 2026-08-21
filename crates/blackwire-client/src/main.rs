use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use blackwire_client::{ClientConfig, ProtectedEgressGuard};
use blackwire_core::Instance;
use blackwire_transport::{create_tun, ensure_tun_runtime_supported, TunRuntime};
use clap::Parser;
use tokio::sync::watch;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "blackwire-client",
    version,
    about = "Blackwire local proxy and full-device TUN client"
)]
struct Cli {
    /// Path to a typed Blackwire client JSON configuration.
    #[arg(short, long, value_name = "FILE")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init()
        .ok();

    let args = Cli::parse();
    let bytes = tokio::fs::read(&args.config)
        .await
        .with_context(|| format!("reading client config '{}'", args.config.display()))?;
    let config = serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing client config '{}'", args.config.display()))?;
    let ClientConfig { proxy, tun } = ClientConfig::from_config(config)?;

    ensure_tun_runtime_supported()?;
    let _egress = ProtectedEgressGuard::apply(&tun)?;
    let instance = Instance::from_config(Arc::new(proxy))
        .await
        .context("starting local proxy runtime")?;
    let device =
        create_tun(&tun).context("creating TUN device; elevated privileges are required")?;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut tun_task = tokio::spawn(TunRuntime::new(tun).run(device, shutdown_rx));
    info!("Blackwire client started");

    tokio::select! {
        signal = tokio::signal::ctrl_c() => {
            signal.context("waiting for shutdown signal")?;
            info!("shutdown signal received");
        }
        result = &mut tun_task => {
            drop(instance);
            return match result {
                Ok(Ok(())) => Ok(()),
                Ok(Err(error)) => Err(error).context("TUN runtime failed"),
                Err(error) => Err(error).context("TUN runtime task failed"),
            };
        }
    }

    let _ = shutdown_tx.send(true);
    match tokio::time::timeout(std::time::Duration::from_secs(5), &mut tun_task).await {
        Ok(Ok(Ok(()))) => {}
        Ok(Ok(Err(error))) => error!(%error, "TUN runtime stopped with an error"),
        Ok(Err(error)) => error!(%error, "TUN runtime task failed during shutdown"),
        Err(_) => {
            error!("TUN runtime cleanup timed out; stopping the task");
            tun_task.abort();
            let _ = tun_task.await;
        }
    }
    drop(instance);
    Ok(())
}
