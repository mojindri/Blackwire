//! blackwire — command-line entry point.
//!
//! This binary is the "front door" to the entire proxy platform. Everything
//! you do — start the proxy, test a config file, generate crypto keys — goes
//! through one of the subcommands defined here.
//!
//! # Subcommands
//!
//! | Command            | What it does                                              |
//! |--------------------|-----------------------------------------------------------|
//! | `run  -c PATH`     | Load the config file and start the proxy.                 |
//! | `test -c PATH`     | Parse and validate the config; print OK or errors. Exit.  |
//! | `x25519`           | Generate a new X25519 key pair (for REALITY).             |
//! | `uuid`             | Generate a random UUID v4 (for VLESS user IDs).           |
//! | `version`          | Print the binary version and quit.                        |
//!
//! # How startup works
//!
//! `run`:
//!   1. Initialise the tracing/logging subsystem.
//!   2. Load the config file via `ConfigManager::load()`.
//!   3. Start the config file watcher (so SIGHUP / file changes hot-reload).
//!   4. Build the proxy `Instance` from the config.
//!   5. Install signal handlers for SIGTERM / SIGINT.
//!   6. Wait for either the instance to exit or a shutdown signal.

#[cfg(all(feature = "jemalloc", feature = "mimalloc", not(windows)))]
compile_error!("enable at most one allocator feature: jemalloc or mimalloc");

#[cfg(all(feature = "jemalloc", not(windows)))]
#[global_allocator]
static GLOBAL_ALLOCATOR: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use clap::{Parser, Subcommand};
use tracing::{error, info};
use validator::Validate;

use blackwire_api::management::{InboundManagement, NativeEndpointConfig};
use blackwire_config::schema::{
    explain_cost, validate_fast_profile, Config, InboundConfig, OutboundConfig, ProfileMode,
    ProfileViolation,
};
use blackwire_config::ConfigManager;
use blackwire_core::{requires_instance_restart, Instance};

struct RunningInstance {
    config: Arc<Config>,
    instance: Instance,
}

#[derive(Clone)]
struct RuntimeControl {
    instance: Arc<tokio::sync::Mutex<Option<RunningInstance>>>,
    profile_override: Option<ProfileMode>,
}

// ── Top-level CLI struct ──────────────────────────────────────────────────────

/// A production-grade, v2ray-compatible proxy platform.
///
/// Run `blackwire help <COMMAND>` for detailed usage of any subcommand.
#[derive(Parser)]
#[command(
    name    = "blackwire",
    version = env!("CARGO_PKG_VERSION"),
    about   = "A v2ray-compatible proxy platform written in pure Rust.",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the proxy with the given config file.
    ///
    /// The proxy runs until you press Ctrl-C or send SIGTERM/SIGINT.
    /// If the config file changes on disk while running, the proxy
    /// automatically reloads it without dropping any live connections.
    Run(RunArgs),

    /// Parse and validate a config file, then exit.
    ///
    /// Prints "Config OK" and exits 0 if the config is valid.
    /// Prints a detailed error and exits 1 if anything is wrong.
    Test(TestArgs),

    /// Generate a new X25519 key pair for use with REALITY transport.
    ///
    /// Prints the private key and public key as hex strings.
    /// Copy them into your config.json under `realitySettings`.
    X25519,

    /// Generate a new random UUID v4 for use as a VLESS user ID.
    ///
    /// Prints the UUID in the standard `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`
    /// format. Copy it into your config.json under `clients[n].id`.
    Uuid,

    /// Inspect or close active in-process connections.
    Connections(ConnectionsArgs),

    /// Explain the hot-path cost of a config and suggest lower-cost changes.
    ExplainCost(ExplainCostArgs),

    /// Run a native Hysteria2 UDP datagram benchmark.
    Hy2UdpBench(Hy2UdpBenchArgs),

    /// Run a mixed Hysteria2 UDP benchmark with DNS, interactive, and bulk flows.
    Hy2UdpMixBench(Hy2UdpMixBenchArgs),

    /// Print the build version and quit.
    Version,
}

/// Arguments for the `run` subcommand.
#[derive(clap::Args)]
struct RunArgs {
    /// Path to the JSON config file.
    ///
    /// Example: `blackwire run -c /etc/blackwire/config.json`
    #[arg(short = 'c', long = "config", value_name = "PATH")]
    config: PathBuf,

    /// Override the operating profile (`compat` or `fast`).
    ///
    /// Overrides the `profile` field in the config file. `fast` enforces a
    /// latency-first subset: VLESS+TCP only, no sniffing, no FakeIP.
    ///
    /// Example: `blackwire run -c config.json --profile fast`
    #[arg(long = "profile", value_name = "PROFILE")]
    profile: Option<ProfileMode>,
}

/// Arguments for the `test` subcommand.
#[derive(clap::Args)]
struct TestArgs {
    /// Path to the JSON config file to validate.
    ///
    /// Example: `blackwire test -c /etc/blackwire/config.json`
    #[arg(short = 'c', long = "config", value_name = "PATH")]
    config: PathBuf,

    /// Override the operating profile (`compat` or `fast`).
    ///
    /// Validates the config against the given profile's constraints.
    #[arg(long = "profile", value_name = "PROFILE")]
    profile: Option<ProfileMode>,
}

/// Arguments for the `explain-cost` subcommand.
#[derive(clap::Args)]
struct ExplainCostArgs {
    /// Path to the JSON config file to inspect.
    #[arg(short = 'c', long = "config", value_name = "PATH")]
    config: PathBuf,

    /// Override the operating profile before calculating cost.
    #[arg(long = "profile", value_name = "PROFILE")]
    profile: Option<ProfileMode>,
}

/// Arguments for the `hy2-udp-bench` subcommand.
#[derive(clap::Args)]
struct Hy2UdpBenchArgs {
    /// Hysteria2 server UDP socket, e.g. <server-host>:10310.
    #[arg(long = "server", value_name = "ADDR")]
    server: std::net::SocketAddr,

    /// TLS SNI for the Hysteria2 connection.
    #[arg(long = "server-name", value_name = "NAME")]
    server_name: String,

    /// Hysteria2 shared password.
    #[arg(long = "auth", value_name = "PASSWORD")]
    auth: String,

    /// Skip TLS certificate validation. Intended for lab self-signed certs only.
    #[arg(long = "skip-cert-verify", default_value_t = false)]
    skip_cert_verify: bool,

    /// UDP destination host as seen by the Hysteria2 server.
    #[arg(long = "dest-host", value_name = "HOST")]
    dest_host: String,

    /// UDP destination port as seen by the Hysteria2 server.
    #[arg(long = "dest-port", value_name = "PORT")]
    dest_port: u16,

    /// Number of sequential UDP probes.
    #[arg(long = "count", default_value_t = 500)]
    count: usize,

    /// Maximum number of in-flight UDP probes.
    #[arg(long = "concurrency", default_value_t = 1)]
    concurrency: usize,

    /// Probe payload size in bytes.
    #[arg(long = "payload-bytes", default_value_t = 64)]
    payload_bytes: usize,

    /// Per-probe response timeout in milliseconds.
    #[arg(long = "timeout-ms", default_value_t = 3000)]
    timeout_ms: u64,

    /// Hysteria2 congestion mode.
    #[arg(long = "mode", default_value = "badnet-low-latency")]
    mode: String,

    /// Upload Mbps.
    #[arg(long = "up-mbps", default_value_t = 100)]
    up_mbps: u64,

    /// Download Mbps.
    #[arg(long = "down-mbps", default_value_t = 100)]
    down_mbps: u64,

    /// Hysteria2 endpoint shards.
    #[arg(long = "endpoint-shards", default_value_t = 4)]
    endpoint_shards: usize,

    /// Datagram policy: standard or h2-plus.
    #[arg(long = "datagram-policy", default_value = "h2-plus")]
    datagram_policy: String,

    /// Enable DNS fast retry in h2-plus mode.
    #[arg(long = "fast-dns-retry", default_value_t = false)]
    fast_dns_retry: bool,

    /// DNS fast retry delay in milliseconds.
    #[arg(long = "fast-dns-retry-delay-ms", default_value_t = 20)]
    fast_dns_retry_delay_ms: u64,

    /// FEC mode: off, xor1-of-n, reed-solomon, raptor-like, auto.
    #[arg(long = "fec-mode", default_value = "off")]
    fec_mode: String,

    /// FEC overhead cap percent.
    #[arg(long = "fec-overhead-percent", default_value_t = 20)]
    fec_overhead_percent: u8,

    /// Variant label emitted in JSON output.
    #[arg(long = "variant", default_value = "blackwire-candidate-hy2-udp")]
    variant: String,

    /// Scenario label emitted in JSON output.
    #[arg(long = "scenario", default_value = "hysteria2-udp-dns")]
    scenario: String,
}

/// Arguments for the `hy2-udp-mix-bench` subcommand.
#[derive(clap::Args)]
struct Hy2UdpMixBenchArgs {
    #[command(flatten)]
    common: Hy2UdpBenchArgs,

    /// DNS echo destination port.
    #[arg(long = "dns-port", default_value_t = 5353)]
    dns_port: u16,

    /// Interactive echo destination port.
    #[arg(long = "interactive-port", default_value_t = 1054)]
    interactive_port: u16,

    /// Bulk echo destination port.
    #[arg(long = "bulk-port", default_value_t = 1055)]
    bulk_port: u16,

    /// Number of DNS probes.
    #[arg(long = "dns-count", default_value_t = 200)]
    dns_count: usize,

    /// Number of interactive probes.
    #[arg(long = "interactive-count", default_value_t = 200)]
    interactive_count: usize,

    /// Number of bulk probes.
    #[arg(long = "bulk-count", default_value_t = 400)]
    bulk_count: usize,

    /// Bulk payload size.
    #[arg(long = "bulk-payload-bytes", default_value_t = 1200)]
    bulk_payload_bytes: usize,
}

#[derive(clap::Args)]
struct ConnectionsArgs {
    #[command(subcommand)]
    command: ConnectionsCommand,
}

#[derive(Subcommand)]
enum ConnectionsCommand {
    /// List active connections.
    List,

    /// Show active connections sorted by bytes or age.
    Top {
        #[arg(long = "sort", value_enum, default_value_t = ConnectionSort::Bytes)]
        sort: ConnectionSort,
    },

    /// Close active connections by id, user, inbound, or outbound.
    Close(ConnectionCloseArgs),
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum ConnectionSort {
    Bytes,
    Age,
}

#[derive(clap::Args)]
struct ConnectionCloseArgs {
    #[arg(long = "id", conflicts_with_all = ["user", "inbound", "outbound"])]
    id: Option<u64>,

    #[arg(long = "user", conflicts_with_all = ["id", "inbound", "outbound"])]
    user: Option<String>,

    #[arg(long = "inbound", conflicts_with_all = ["id", "user", "outbound"])]
    inbound: Option<String>,

    #[arg(long = "outbound", conflicts_with_all = ["id", "user", "inbound"])]
    outbound: Option<String>,
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Run(args) => {
            // Build the async runtime first, then hand control to `run_proxy`.
            // We use 2× CPU cores: relay tasks are I/O-bound and yield frequently,
            // but at high PPS spare threads let new-connection tasks run without
            // waiting behind an active relay task's local queue.
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(num_cpus::get() * 2)
                .enable_all()
                .build()
            {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error: failed to build Tokio runtime: {e}");
                    std::process::exit(1);
                }
            };

            // `block_on` runs the async function to completion on this thread.
            // It returns only when the proxy exits (Ctrl-C or error).
            if let Err(e) = rt.block_on(run_proxy(args)) {
                // Print a human-readable error chain, e.g.:
                //   Error: failed to start proxy
                //     caused by: building VLESS outbound 'out-vless'
                //     caused by: invalid VLESS server address '999.0.0.1:443'
                eprintln!("Error: {e:#}");
                std::process::exit(1);
            }
        }

        Command::Test(args) => {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error: failed to build Tokio runtime: {e}");
                    std::process::exit(1);
                }
            };

            if let Err(e) = rt.block_on(test_config(args)) {
                eprintln!("Config error: {e:#}");
                std::process::exit(1);
            }
            println!("Config OK");
        }

        Command::X25519 => cmd_x25519(),
        Command::Uuid => cmd_uuid(),
        Command::Connections(args) => cmd_connections(args),
        Command::ExplainCost(args) => {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error: failed to build Tokio runtime: {e}");
                    std::process::exit(1);
                }
            };

            if let Err(e) = rt.block_on(cmd_explain_cost(args)) {
                eprintln!("Error: {e:#}");
                std::process::exit(1);
            }
        }
        Command::Hy2UdpBench(args) => {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error: failed to build Tokio runtime: {e}");
                    std::process::exit(1);
                }
            };

            if let Err(e) = rt.block_on(cmd_hy2_udp_bench(args)) {
                eprintln!("Error: {e:#}");
                std::process::exit(1);
            }
        }
        Command::Hy2UdpMixBench(args) => {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error: failed to build Tokio runtime: {e}");
                    std::process::exit(1);
                }
            };

            if let Err(e) = rt.block_on(cmd_hy2_udp_mix_bench(args)) {
                eprintln!("Error: {e:#}");
                std::process::exit(1);
            }
        }

        Command::Version => {
            println!("blackwire {}", env!("CARGO_PKG_VERSION"));
        }
    }
}

// ── `run` subcommand ──────────────────────────────────────────────────────────

/// Load config, build the Instance, run until a shutdown signal arrives.
///
/// This is an `async fn` so it can use `.await` for Tokio-based I/O.
async fn run_proxy(args: RunArgs) -> Result<()> {
    // Step 1: Initialise logging.
    // We do this before anything else so all startup messages are captured.
    init_tracing();

    info!(
        version = env!("CARGO_PKG_VERSION"),
        config  = %args.config.display(),
        "blackwire starting"
    );

    // Step 2: Load and validate the config.
    // `ConfigManager::load()` reads the file, substitutes ${ENV} vars,
    // parses JSON, and runs the validator rules.
    let manager: Arc<ConfigManager> = ConfigManager::load(&args.config)
        .await
        .with_context(|| format!("loading config from {}", args.config.display()))?;

    // Apply CLI profile override and run Fast Profile validation.
    let profile_override = args.profile;
    apply_profile_override_and_validate(&manager.get(), profile_override)?;

    // Step 3: Start the file watcher for hot-reload.
    // The watcher runs in a background Tokio task. When the config file
    // changes on disk, `ConfigManager::watch()` parses the new version and
    // atomically swaps it in. This does NOT restart any listeners — only
    // config values that are consulted per-connection (like routing rules)
    // change immediately.
    {
        let manager_clone = Arc::clone(&manager);
        tokio::spawn(async move {
            if let Err(e) = manager_clone.watch().await {
                error!(error = %e, "config watcher failed");
            }
        });
    }

    // Step 4: Build the proxy Instance.
    // `Instance::from_config()` reads the current config snapshot, builds
    // all inbound/outbound handlers, and starts all TCP listener tasks.
    let config = effective_config(manager.get(), profile_override);
    let api_addr = config
        .api
        .as_ref()
        .and_then(blackwire_api::server::api_listen_addr);
    let runtime_config = instance_runtime_config(&config);
    let instance = Arc::new(tokio::sync::Mutex::new(Some(RunningInstance {
        config: Arc::clone(&runtime_config),
        instance: Instance::from_config(runtime_config)
            .await
            .context("building proxy instance from config")?,
    })));

    if let Some(api_addr) = api_addr {
        let management: blackwire_api::management::ManagementHandle = Arc::new(RuntimeControl {
            instance: Arc::clone(&instance),
            profile_override,
        });
        blackwire_api::server::start_api_server(&api_addr, management)
            .with_context(|| format!("starting blackwire-api gRPC server on '{api_addr}'"))?;
        info!(addr = %api_addr, "blackwire-api gRPC server started");
    }

    // Step 4b: Apply hot-reload when config file changes (routing + VLESS users).
    // Listeners keep running; only per-connection lookup tables are refreshed.
    {
        let live_instance = Arc::clone(&instance);
        let mut reload_rx = manager.subscribe();
        tokio::spawn(async move {
            loop {
                if reload_rx.changed().await.is_err() {
                    break;
                }
                let effective =
                    effective_config(reload_rx.borrow_and_update().clone(), profile_override);
                let new_config = instance_runtime_config(&effective);

                let should_restart = {
                    let guard = live_instance.lock().await;
                    let Some(running) = guard.as_ref() else {
                        break;
                    };
                    requires_instance_restart(&running.config, &new_config)
                };

                if should_restart {
                    info!("structural config change detected — rebuilding running instance");
                    let (old_config, old_instance) = {
                        let mut guard = live_instance.lock().await;
                        let Some(running) = guard.take() else {
                            break;
                        };
                        (running.config, running.instance)
                    };
                    drop(old_instance);

                    let rebuilt = match Instance::from_config(Arc::clone(&new_config)).await {
                        Ok(instance) => {
                            info!("instance rebuilt successfully after config change");
                            Some(RunningInstance {
                                config: Arc::clone(&new_config),
                                instance,
                            })
                        }
                        Err(e) => {
                            error!(error = %e, "instance rebuild failed — attempting rollback to previous config");
                            match Instance::from_config(Arc::clone(&old_config)).await {
                                Ok(instance) => Some(RunningInstance {
                                    config: old_config,
                                    instance,
                                }),
                                Err(rollback_err) => {
                                    error!(error = %rollback_err, "rollback failed — no running instance remains");
                                    None
                                }
                            }
                        }
                    };

                    let mut guard = live_instance.lock().await;
                    *guard = rebuilt;
                    continue;
                }

                let reload = {
                    let guard = live_instance.lock().await;
                    let Some(running) = guard.as_ref() else {
                        break;
                    };
                    running.instance.reload.clone()
                };
                if let Err(e) = reload.apply(&new_config) {
                    error!(error = %e, "config reload apply failed — keeping prior routing/users");
                }
            }
        });
    }

    info!("blackwire started — waiting for connections");

    // Step 5: Wait for a shutdown signal or for all listeners to exit.
    // We listen for Ctrl-C (SIGINT) plus SIGTERM on Unix (what systemd sends).
    shutdown_signal(instance).await;

    Ok(())
}

/// Wait for a shutdown signal or for all listeners to exit.
///
/// On Unix, listens for both SIGINT (Ctrl-C) and SIGTERM (systemd stop).
/// On other platforms, only SIGINT.
async fn shutdown_signal(instance: Arc<tokio::sync::Mutex<Option<RunningInstance>>>) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(v) => Some(v),
            Err(e) => {
                info!("SIGTERM handler unavailable ({e}); waiting for SIGINT/listener exit");
                None
            }
        };

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("received SIGINT — shutting down");
            }
            _ = async {
                if let Some(sigterm) = sigterm.as_mut() {
                    sigterm.recv().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                info!("received SIGTERM — shutting down");
            }
        }
    }

    #[cfg(not(unix))]
    {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("received SIGINT — shutting down");
            }
        }
    }

    let mut guard = instance.lock().await;
    if let Some(running) = guard.take() {
        running.instance.shutdown();
    }
}

impl RuntimeControl {
    async fn with_reload<T>(
        &self,
        f: impl FnOnce(&blackwire_core::ReloadState) -> T,
    ) -> Result<T, String> {
        let guard = self.instance.lock().await;
        let running = guard
            .as_ref()
            .ok_or_else(|| "no running instance is available".to_string())?;
        Ok(f(&running.instance.reload))
    }

    async fn rebuild_with_config(&self, new_config: Config) -> Result<(), String> {
        let runtime_config = Arc::new(new_config);
        runtime_config
            .validate()
            .map_err(|e| format!("config validation failed: {e}"))?;
        apply_profile_override_and_validate(&runtime_config, self.profile_override)
            .map_err(|e| format!("{e:#}"))?;

        let (old_config, old_instance) = {
            let mut guard = self.instance.lock().await;
            let running = guard
                .take()
                .ok_or_else(|| "no running instance is available".to_string())?;
            (running.config, running.instance)
        };
        drop(old_instance);

        match Instance::from_config(Arc::clone(&runtime_config)).await {
            Ok(instance) => {
                let mut guard = self.instance.lock().await;
                *guard = Some(RunningInstance {
                    config: runtime_config,
                    instance,
                });
                Ok(())
            }
            Err(e) => {
                let rollback = Instance::from_config(Arc::clone(&old_config)).await;
                let mut guard = self.instance.lock().await;
                match rollback {
                    Ok(instance) => {
                        *guard = Some(RunningInstance {
                            config: old_config,
                            instance,
                        });
                        Err(format!("instance rebuild failed; rolled back: {e:#}"))
                    }
                    Err(rollback_err) => {
                        *guard = None;
                        Err(format!(
                            "instance rebuild failed and rollback failed; no instance is running: rebuild={e:#}; rollback={rollback_err:#}"
                        ))
                    }
                }
            }
        }
    }

    async fn mutate_config(
        &self,
        f: impl FnOnce(&mut Config) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut new_config = {
            let guard = self.instance.lock().await;
            guard
                .as_ref()
                .ok_or_else(|| "no running instance is available".to_string())?
                .config
                .as_ref()
                .clone()
        };
        f(&mut new_config)?;
        self.rebuild_with_config(new_config).await
    }
}

fn parse_inbound_endpoint(config: NativeEndpointConfig) -> Result<InboundConfig, String> {
    let endpoint: InboundConfig = serde_json::from_value(config.config)
        .map_err(|e| format!("invalid inbound config JSON: {e}"))?;
    if endpoint.tag != config.tag {
        return Err(format!(
            "inbound tag mismatch: request tag '{}' != config tag '{}'",
            config.tag, endpoint.tag
        ));
    }
    endpoint
        .validate()
        .map_err(|e| format!("inbound validation failed: {e}"))?;
    Ok(endpoint)
}

fn parse_outbound_endpoint(config: NativeEndpointConfig) -> Result<OutboundConfig, String> {
    let endpoint: OutboundConfig = serde_json::from_value(config.config)
        .map_err(|e| format!("invalid outbound config JSON: {e}"))?;
    if endpoint.tag != config.tag {
        return Err(format!(
            "outbound tag mismatch: request tag '{}' != config tag '{}'",
            config.tag, endpoint.tag
        ));
    }
    endpoint
        .validate()
        .map_err(|e| format!("outbound validation failed: {e}"))?;
    Ok(endpoint)
}

#[async_trait]
impl InboundManagement for RuntimeControl {
    async fn list_inbound_tags(&self) -> Vec<String> {
        self.with_reload(|r| r.inbound_tags.read().map(|t| t.clone()).unwrap_or_default())
            .await
            .unwrap_or_default()
    }

    async fn list_outbound_tags(&self) -> Vec<String> {
        self.with_reload(|r| {
            r.outbound_tags
                .read()
                .map(|t| t.clone())
                .unwrap_or_default()
        })
        .await
        .unwrap_or_default()
    }

    async fn vless_user_count(&self, inbound_tag: &str) -> Option<i64> {
        self.with_reload(|r| {
            r.vless_registries
                .get(inbound_tag)
                .map(|registry| registry.len() as i64)
        })
        .await
        .ok()
        .flatten()
    }

    async fn list_vless_users(
        &self,
        inbound_tag: &str,
        email: &str,
    ) -> Result<Vec<blackwire_api::management::VlessUserRecord>, String> {
        self.with_reload(|r| {
            r.vless_registries
                .get(inbound_tag)
                .map(|registry| {
                    registry
                        .list_users(email)
                        .into_iter()
                        .map(|u| blackwire_api::management::VlessUserRecord {
                            email: u.email.to_string(),
                            uuid: uuid::Uuid::from_bytes(u.uuid).to_string(),
                            flow: u.flow.clone(),
                            level: 0,
                        })
                        .collect()
                })
                .ok_or_else(|| format!("inbound '{inbound_tag}' has no VLESS user registry"))
        })
        .await?
    }

    async fn add_vless_user(
        &self,
        inbound_tag: &str,
        email: &str,
        uuid: &str,
        flow: &str,
    ) -> Result<(), String> {
        self.with_reload(|r| {
            r.vless_registries
                .get(inbound_tag)
                .ok_or_else(|| format!("inbound '{inbound_tag}' has no VLESS user registry"))
                .and_then(|registry| {
                    let uuid = uuid::Uuid::parse_str(uuid)
                        .map_err(|e| format!("invalid UUID '{uuid}': {e}"))?
                        .into_bytes();
                    registry.add_user(blackwire_protocol::vless::VlessUser {
                        email: email.into(),
                        uuid,
                        flow: flow.to_string(),
                    });
                    Ok(())
                })
        })
        .await?
    }

    async fn remove_vless_user(&self, inbound_tag: &str, email: &str) -> Result<(), String> {
        self.with_reload(|r| {
            r.vless_registries
                .get(inbound_tag)
                .ok_or_else(|| format!("inbound '{inbound_tag}' has no VLESS user registry"))
                .and_then(|registry| {
                    if registry.remove_user_by_email(email) {
                        Ok(())
                    } else {
                        Err(format!(
                            "no VLESS user with email '{email}' on inbound '{inbound_tag}'"
                        ))
                    }
                })
        })
        .await?
    }

    async fn add_inbound(&self, config: NativeEndpointConfig) -> Result<(), String> {
        let endpoint = parse_inbound_endpoint(config)?;
        self.mutate_config(|cfg| {
            if cfg.inbounds.iter().any(|i| i.tag == endpoint.tag) {
                return Err(format!("inbound '{}' already exists", endpoint.tag));
            }
            cfg.inbounds.push(endpoint);
            Ok(())
        })
        .await
    }

    async fn remove_inbound(&self, tag: &str) -> Result<(), String> {
        self.mutate_config(|cfg| {
            let before = cfg.inbounds.len();
            cfg.inbounds.retain(|i| i.tag != tag);
            if cfg.inbounds.len() == before {
                return Err(format!("inbound '{tag}' not found"));
            }
            Ok(())
        })
        .await
    }

    async fn add_outbound(&self, config: NativeEndpointConfig) -> Result<(), String> {
        let endpoint = parse_outbound_endpoint(config)?;
        self.mutate_config(|cfg| {
            if cfg.outbounds.iter().any(|o| o.tag == endpoint.tag) {
                return Err(format!("outbound '{}' already exists", endpoint.tag));
            }
            cfg.outbounds.push(endpoint);
            Ok(())
        })
        .await
    }

    async fn remove_outbound(&self, tag: &str) -> Result<(), String> {
        self.mutate_config(|cfg| {
            let before = cfg.outbounds.len();
            cfg.outbounds.retain(|o| o.tag != tag);
            if cfg.outbounds.len() == before {
                return Err(format!("outbound '{tag}' not found"));
            }
            Ok(())
        })
        .await
    }

    async fn alter_outbound(&self, config: NativeEndpointConfig) -> Result<(), String> {
        let endpoint = parse_outbound_endpoint(config)?;
        self.mutate_config(|cfg| {
            let existing = cfg
                .outbounds
                .iter_mut()
                .find(|o| o.tag == endpoint.tag)
                .ok_or_else(|| format!("outbound '{}' not found", endpoint.tag))?;
            *existing = endpoint;
            Ok(())
        })
        .await
    }

    async fn list_connections(&self) -> Vec<blackwire_connmgr::ConnectionSnapshot> {
        blackwire_connmgr::global_manager().list()
    }

    async fn close_connections(
        &self,
        selector: blackwire_connmgr::CloseSelector,
    ) -> Result<usize, String> {
        Ok(blackwire_connmgr::global_manager().close(selector).matched)
    }
}

// ── `test` subcommand ─────────────────────────────────────────────────────────

/// Parse and validate the config file; return Ok or an error.
async fn test_config(args: TestArgs) -> Result<()> {
    let manager = ConfigManager::load(&args.config)
        .await
        .with_context(|| format!("loading config from {}", args.config.display()))?;
    apply_profile_override_and_validate(&manager.get(), args.profile)?;
    Ok(())
}

async fn cmd_explain_cost(args: ExplainCostArgs) -> Result<()> {
    let manager = ConfigManager::load(&args.config)
        .await
        .with_context(|| format!("loading config from {}", args.config.display()))?;
    let config = effective_config(manager.get(), args.profile);
    let report = explain_cost(&config);
    print!("{}", report.render_text());
    Ok(())
}

async fn cmd_hy2_udp_bench(args: Hy2UdpBenchArgs) -> Result<()> {
    let datagram_mode = parse_datagram_priority_mode(&args.datagram_policy)?;
    let fec_mode = parse_fec_mode(&args.fec_mode)?;
    let congestion_mode = args
        .mode
        .parse::<blackwire_transport::CongestionMode>()
        .map_err(anyhow::Error::msg)?;
    let dest = parse_hy2_udp_destination(&args.dest_host, args.dest_port);
    let config = hy2_udp_bench_config(&args, congestion_mode, datagram_mode, fec_mode);

    let session = blackwire_transport::Hysteria2UdpSession::connect(&config)
        .await
        .map_err(|e| anyhow::anyhow!("Hysteria2 UDP connect failed: {e}"))?;
    let stats = run_udp_probe_set(
        &session,
        dest,
        args.count,
        args.payload_bytes,
        args.concurrency,
        Duration::from_millis(args.timeout_ms.max(1)),
        0,
    )
    .await?;
    let fec_snapshot = session.fec_snapshot();

    let row = serde_json::json!({
        "variant": args.variant,
        "scenario": args.scenario,
        "protocol": "hysteria2",
        "transport": "quic-datagram",
        "profile": args.mode,
        "payload_size": args.payload_bytes,
        "concurrency": args.concurrency.max(1),
        "requests": args.count,
        "ok": stats.ok(),
        "errors": stats.errors,
        "stale_replies": stats.stale_replies,
        "requests_per_sec": stats.rps(),
        "duration_secs": stats.duration_secs,
        "latency_p50_ms": percentile_ms(&stats.latencies_us, 50.0),
        "latency_p90_ms": percentile_ms(&stats.latencies_us, 90.0),
        "latency_p95_ms": percentile_ms(&stats.latencies_us, 95.0),
        "latency_p99_ms": percentile_ms(&stats.latencies_us, 99.0),
        "latency_p999_ms": percentile_ms(&stats.latencies_us, 99.9),
        "bytes_up": stats.bytes_up,
        "bytes_down": stats.bytes_down,
        "datagram_policy": format!("{:?}", datagram_mode),
        "fast_dns_retry": args.fast_dns_retry,
        "fec_mode": format!("{:?}", fec_mode),
        "fec_overhead_percent": args.fec_overhead_percent,
        "fec_client_parity_packets": fec_snapshot.parity_packets,
        "fec_client_overhead_bytes": fec_snapshot.overhead_bytes,
        "fec_client_recovered_packets": fec_snapshot.recovered_packets,
        "fec_client_stale_drops": fec_snapshot.stale_drops,
        "fec_client_duplicate_safe_skips": fec_snapshot.duplicate_safe_skips,
    });
    println!("{}", serde_json::to_string(&row)?);
    Ok(())
}

async fn cmd_hy2_udp_mix_bench(args: Hy2UdpMixBenchArgs) -> Result<()> {
    let datagram_mode = parse_datagram_priority_mode(&args.common.datagram_policy)?;
    let fec_mode = parse_fec_mode(&args.common.fec_mode)?;
    let congestion_mode = args
        .common
        .mode
        .parse::<blackwire_transport::CongestionMode>()
        .map_err(anyhow::Error::msg)?;
    let config = hy2_udp_bench_config(&args.common, congestion_mode, datagram_mode, fec_mode);
    let session = blackwire_transport::Hysteria2UdpSession::connect(&config)
        .await
        .map_err(|e| anyhow::anyhow!("Hysteria2 UDP connect failed: {e}"))?;

    let stats = run_udp_mixed_probe_set(&session, &args).await?;
    let fec_snapshot = session.fec_snapshot();
    let row = serde_json::json!({
        "variant": args.common.variant,
        "scenario": args.common.scenario,
        "protocol": "hysteria2",
        "transport": "quic-datagram",
        "profile": args.common.mode,
        "concurrency": args.common.concurrency.max(1),
        "dns_requests": args.dns_count,
        "dns_ok": stats.dns.ok(),
        "dns_errors": stats.dns.errors,
        "dns_stale_replies": stats.dns.stale_replies,
        "dns_rps": stats.dns.rps(),
        "dns_latency_p95_ms": percentile_ms(&stats.dns.latencies_us, 95.0),
        "dns_latency_p99_ms": percentile_ms(&stats.dns.latencies_us, 99.0),
        "interactive_requests": args.interactive_count,
        "interactive_ok": stats.interactive.ok(),
        "interactive_errors": stats.interactive.errors,
        "interactive_stale_replies": stats.interactive.stale_replies,
        "interactive_rps": stats.interactive.rps(),
        "interactive_latency_p95_ms": percentile_ms(&stats.interactive.latencies_us, 95.0),
        "interactive_latency_p99_ms": percentile_ms(&stats.interactive.latencies_us, 99.0),
        "bulk_requests": args.bulk_count,
        "bulk_ok": stats.bulk.ok(),
        "bulk_errors": stats.bulk.errors,
        "bulk_stale_replies": stats.bulk.stale_replies,
        "bulk_rps": stats.bulk.rps(),
        "bulk_latency_p95_ms": percentile_ms(&stats.bulk.latencies_us, 95.0),
        "bulk_latency_p99_ms": percentile_ms(&stats.bulk.latencies_us, 99.0),
        "bytes_up": stats.dns.bytes_up + stats.interactive.bytes_up + stats.bulk.bytes_up,
        "bytes_down": stats.dns.bytes_down + stats.interactive.bytes_down + stats.bulk.bytes_down,
        "datagram_policy": format!("{:?}", datagram_mode),
        "fast_dns_retry": args.common.fast_dns_retry,
        "fec_mode": format!("{:?}", fec_mode),
        "fec_overhead_percent": args.common.fec_overhead_percent,
        "fec_client_parity_packets": fec_snapshot.parity_packets,
        "fec_client_overhead_bytes": fec_snapshot.overhead_bytes,
        "fec_client_recovered_packets": fec_snapshot.recovered_packets,
        "fec_client_stale_drops": fec_snapshot.stale_drops,
        "fec_client_duplicate_safe_skips": fec_snapshot.duplicate_safe_skips,
    });
    println!("{}", serde_json::to_string(&row)?);
    Ok(())
}

fn hy2_udp_bench_config(
    args: &Hy2UdpBenchArgs,
    congestion_mode: blackwire_transport::CongestionMode,
    datagram_mode: blackwire_transport::DatagramPriorityMode,
    fec_mode: blackwire_transport::FecMode,
) -> blackwire_transport::Hysteria2ClientConfig {
    blackwire_transport::Hysteria2ClientConfig {
        server: args.server,
        server_name: args.server_name.clone(),
        password: args.auth.clone(),
        up_mbps: args.up_mbps,
        down_mbps: args.down_mbps,
        skip_cert_verify: args.skip_cert_verify,
        congestion: blackwire_transport::CongestionConfig {
            mode: congestion_mode,
            up_mbps: args.up_mbps,
            down_mbps: args.down_mbps,
            min_ack_rate: 0.9,
            max_queue_delay: Duration::from_millis(50),
            pacing_gain: 0.9,
            loss_compensation: true,
        },
        endpoint_shards: args.endpoint_shards,
        socket: blackwire_transport::QuicSocketConfig::default(),
        datagram_enabled: true,
        fec: blackwire_transport::FecPolicy {
            mode: fec_mode,
            max_overhead_percent: args.fec_overhead_percent,
            group_size: (100usize.div_ceil(args.fec_overhead_percent.max(1) as usize))
                .max(2)
                .min(u8::MAX as usize) as u8,
            ..blackwire_transport::FecPolicy::default()
        },
        datagram_policy: blackwire_transport::DatagramPolicy {
            mode: datagram_mode,
            max_queue_delay_ms: 25,
            fast_dns_retry: args.fast_dns_retry,
            fast_dns_retry_delay_ms: args.fast_dns_retry_delay_ms,
        },
    }
}

#[derive(Default)]
struct UdpBenchStats {
    latencies_us: Vec<u64>,
    errors: usize,
    stale_replies: usize,
    bytes_up: usize,
    bytes_down: usize,
    duration_secs: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum BenchClass {
    Dns,
    Interactive,
    Bulk,
}

#[derive(Default)]
struct UdpMixedBenchStats {
    dns: UdpBenchStats,
    interactive: UdpBenchStats,
    bulk: UdpBenchStats,
}

impl UdpMixedBenchStats {
    fn by_class_mut(&mut self, class: BenchClass) -> &mut UdpBenchStats {
        match class {
            BenchClass::Dns => &mut self.dns,
            BenchClass::Interactive => &mut self.interactive,
            BenchClass::Bulk => &mut self.bulk,
        }
    }
}

struct InFlightProbe {
    class: BenchClass,
    sent_at: Instant,
}

async fn run_udp_mixed_probe_set(
    session: &blackwire_transport::Hysteria2UdpSession,
    args: &Hy2UdpMixBenchArgs,
) -> Result<UdpMixedBenchStats> {
    let dns_dest = parse_hy2_udp_destination(&args.common.dest_host, args.dns_port);
    let interactive_dest = parse_hy2_udp_destination(&args.common.dest_host, args.interactive_port);
    let bulk_dest = parse_hy2_udp_destination(&args.common.dest_host, args.bulk_port);
    let timeout_per_probe = Duration::from_millis(args.common.timeout_ms.max(1));
    let concurrency = args.common.concurrency.max(1);
    let started = Instant::now();
    let mut stats = UdpMixedBenchStats::default();
    stats.dns.bytes_up = args
        .dns_count
        .saturating_mul(args.common.payload_bytes.max(8));
    stats.interactive.bytes_up = args
        .interactive_count
        .saturating_mul(args.common.payload_bytes.max(8));
    stats.bulk.bytes_up = args
        .bulk_count
        .saturating_mul(args.bulk_payload_bytes.max(8));

    let mut sent_dns = 0usize;
    let mut sent_interactive = 0usize;
    let mut sent_bulk = 0usize;
    let mut next_seq = 1u64;
    let mut class_cursor = 0usize;
    let h2_plus_mix = parse_datagram_priority_mode(&args.common.datagram_policy)?
        == blackwire_transport::DatagramPriorityMode::H2Plus;
    let mut in_flight: HashMap<u64, InFlightProbe> = HashMap::new();

    while sent_dns < args.dns_count
        || sent_interactive < args.interactive_count
        || sent_bulk < args.bulk_count
        || !in_flight.is_empty()
    {
        let priority_pending =
            h2_plus_mix && (sent_dns < args.dns_count || sent_interactive < args.interactive_count);
        let send_limit = if priority_pending {
            concurrency.min(16)
        } else {
            concurrency
        };
        while in_flight.len() < send_limit {
            let mut next = None;
            let order = if h2_plus_mix { [1, 0, 2] } else { [0, 1, 2] };
            for _ in 0..3 {
                let class_idx = if h2_plus_mix {
                    order
                        .iter()
                        .copied()
                        .find(|idx| match idx {
                            0 => sent_dns < args.dns_count,
                            1 => sent_interactive < args.interactive_count,
                            2 => sent_bulk < args.bulk_count,
                            _ => false,
                        })
                        .unwrap_or(order[class_cursor % 3])
                } else {
                    order[class_cursor % 3]
                };
                match class_idx {
                    0 if sent_dns < args.dns_count => {
                        sent_dns += 1;
                        next = Some((BenchClass::Dns, dns_dest.clone(), args.common.payload_bytes));
                    }
                    1 if sent_interactive < args.interactive_count => {
                        sent_interactive += 1;
                        next = Some((
                            BenchClass::Interactive,
                            interactive_dest.clone(),
                            args.common.payload_bytes,
                        ));
                    }
                    2 if sent_bulk < args.bulk_count => {
                        sent_bulk += 1;
                        next = Some((BenchClass::Bulk, bulk_dest.clone(), args.bulk_payload_bytes));
                    }
                    _ => {}
                }
                class_cursor += 1;
                if next.is_some() {
                    break;
                }
            }
            let Some((class, dest, payload_bytes)) = next else {
                break;
            };
            let seq = next_seq;
            next_seq += 1;
            let payload = bench_payload(seq, payload_bytes);
            session
                .send(dest, bytes::Bytes::from(payload))
                .map_err(|e| anyhow::anyhow!("Hysteria2 UDP mixed send failed: {e}"))?;
            in_flight.insert(
                seq,
                InFlightProbe {
                    class,
                    sent_at: Instant::now(),
                },
            );
        }

        let now = Instant::now();
        let expired: Vec<u64> = in_flight
            .iter()
            .filter_map(|(seq, probe)| {
                (now.duration_since(probe.sent_at) >= timeout_per_probe).then_some(*seq)
            })
            .collect();
        for seq in expired {
            if let Some(probe) = in_flight.remove(&seq) {
                stats.by_class_mut(probe.class).errors += 1;
            }
        }
        if in_flight.is_empty() {
            continue;
        }

        let remaining = in_flight
            .values()
            .map(|probe| timeout_per_probe.saturating_sub(probe.sent_at.elapsed()))
            .min()
            .unwrap_or(timeout_per_probe)
            .max(Duration::from_millis(1));

        if let Ok(Ok(reply)) = tokio::time::timeout(remaining, session.recv()).await {
            if let Some(seq) = bench_payload_seq(reply.data.as_ref()) {
                if let Some(probe) = in_flight.remove(&seq) {
                    let class_stats = stats.by_class_mut(probe.class);
                    class_stats
                        .latencies_us
                        .push(probe.sent_at.elapsed().as_micros() as u64);
                    class_stats.bytes_down += reply.data.len();
                } else {
                    stats.bulk.stale_replies += 1;
                }
            } else {
                stats.bulk.stale_replies += 1;
            }
        }
    }

    let elapsed = started.elapsed().as_secs_f64().max(0.000_001);
    for class in [BenchClass::Dns, BenchClass::Interactive, BenchClass::Bulk] {
        let class_stats = stats.by_class_mut(class);
        class_stats.duration_secs = elapsed;
        class_stats.latencies_us.sort_unstable();
    }
    Ok(stats)
}

impl UdpBenchStats {
    fn ok(&self) -> usize {
        self.latencies_us.len()
    }

    fn rps(&self) -> f64 {
        self.ok() as f64 / self.duration_secs.max(0.000_001)
    }
}

async fn run_udp_probe_set(
    session: &blackwire_transport::Hysteria2UdpSession,
    dest: blackwire_transport::UdpDestination,
    count: usize,
    payload_bytes: usize,
    concurrency: usize,
    timeout_per_probe: Duration,
    seq_base: u64,
) -> Result<UdpBenchStats> {
    let started = Instant::now();
    let mut stats = UdpBenchStats {
        latencies_us: Vec::with_capacity(count),
        bytes_up: count.saturating_mul(payload_bytes.max(8)),
        ..UdpBenchStats::default()
    };

    let concurrency = concurrency.max(1);
    let mut next_seq = 0usize;
    let mut in_flight: HashMap<u64, Instant> = HashMap::new();

    while next_seq < count || !in_flight.is_empty() {
        while next_seq < count && in_flight.len() < concurrency {
            let seq = next_seq as u64;
            let wire_seq = seq_base + seq;
            let payload = bench_payload(wire_seq, payload_bytes);
            let sent_at = Instant::now();
            session
                .send(dest.clone(), bytes::Bytes::from(payload))
                .map_err(|e| anyhow::anyhow!("Hysteria2 UDP send failed: {e}"))?;
            in_flight.insert(seq, sent_at);
            next_seq += 1;
        }

        let now = Instant::now();
        let expired: Vec<u64> = in_flight
            .iter()
            .filter_map(|(seq, sent_at)| {
                (now.duration_since(*sent_at) >= timeout_per_probe).then_some(*seq)
            })
            .collect();
        for seq in expired {
            if in_flight.remove(&seq).is_some() {
                stats.errors += 1;
            }
        }
        if in_flight.is_empty() {
            continue;
        }

        let remaining = in_flight
            .values()
            .map(|sent_at| timeout_per_probe.saturating_sub(sent_at.elapsed()))
            .min()
            .unwrap_or(timeout_per_probe)
            .max(Duration::from_millis(1));

        match tokio::time::timeout(remaining, session.recv()).await {
            Ok(Ok(reply)) => {
                if let Some(seq) = bench_payload_seq(reply.data.as_ref()) {
                    if let Some(sent_at) = in_flight.remove(&seq.saturating_sub(seq_base)) {
                        stats
                            .latencies_us
                            .push(sent_at.elapsed().as_micros() as u64);
                        stats.bytes_down += reply.data.len();
                    } else {
                        stats.stale_replies += 1;
                    }
                } else {
                    stats.stale_replies += 1;
                }
            }
            Ok(Err(_)) => {}
            Err(_) => {}
        }
    }
    stats.duration_secs = started.elapsed().as_secs_f64().max(0.000_001);
    stats.latencies_us.sort_unstable();
    Ok(stats)
}

fn parse_hy2_udp_destination(host: &str, port: u16) -> blackwire_transport::UdpDestination {
    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        return blackwire_transport::UdpDestination::V4(ip, port);
    }
    if let Ok(ip) = host.parse::<std::net::Ipv6Addr>() {
        return blackwire_transport::UdpDestination::V6(ip, port);
    }
    blackwire_transport::UdpDestination::Domain(host.to_string(), port)
}

fn parse_datagram_priority_mode(value: &str) -> Result<blackwire_transport::DatagramPriorityMode> {
    match value {
        "standard" => Ok(blackwire_transport::DatagramPriorityMode::Standard),
        "h2-plus" | "h2plus" | "h2_plus" => Ok(blackwire_transport::DatagramPriorityMode::H2Plus),
        other => anyhow::bail!("unknown datagram policy '{other}'"),
    }
}

fn parse_fec_mode(value: &str) -> Result<blackwire_transport::FecMode> {
    match value {
        "off" => Ok(blackwire_transport::FecMode::Off),
        "xor1-of-n" | "xor1OfN" | "xor" => Ok(blackwire_transport::FecMode::Xor1OfN),
        "reed-solomon" | "reedSolomon" => Ok(blackwire_transport::FecMode::ReedSolomon),
        "raptor-like" | "raptorLike" => Ok(blackwire_transport::FecMode::RaptorLike),
        "auto" => Ok(blackwire_transport::FecMode::Auto),
        other => anyhow::bail!("unknown FEC mode '{other}'"),
    }
}

fn bench_payload(seq: u64, payload_bytes: usize) -> Vec<u8> {
    let len = payload_bytes.max(8);
    let mut payload = vec![0u8; len];
    payload[..8].copy_from_slice(&seq.to_be_bytes());
    for (idx, byte) in payload[8..].iter_mut().enumerate() {
        *byte = (idx as u8).wrapping_mul(31).wrapping_add(17);
    }
    payload
}

fn bench_payload_seq(payload: &[u8]) -> Option<u64> {
    let seq_bytes: [u8; 8] = payload.get(..8)?.try_into().ok()?;
    Some(u64::from_be_bytes(seq_bytes))
}

fn percentile_ms(sorted_us: &[u64], percentile: f64) -> f64 {
    if sorted_us.is_empty() {
        return 0.0;
    }
    let rank = ((percentile / 100.0) * sorted_us.len() as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(sorted_us.len() - 1);
    sorted_us[idx] as f64 / 1000.0
}

// ── Profile helpers ───────────────────────────────────────────────────────────

/// Return an `Arc<Config>` with the CLI profile override applied (if any).
fn effective_config(
    base: Arc<blackwire_config::schema::Config>,
    override_: Option<ProfileMode>,
) -> Arc<Config> {
    let Some(profile) = override_ else {
        return base;
    };
    if base.profile == profile {
        return base;
    }
    let mut cfg = (*base).clone();
    cfg.profile = profile;
    Arc::new(cfg)
}

/// The CLI owns the gRPC API server so HandlerService can rebuild the live
/// `Instance`. Strip `api` before handing config to core to avoid a second API
/// server being started by direct `Instance::from_config` compatibility code.
fn instance_runtime_config(base: &Arc<Config>) -> Arc<Config> {
    if base.api.is_none() {
        return Arc::clone(base);
    }
    let mut cfg = base.as_ref().clone();
    cfg.api = None;
    Arc::new(cfg)
}

/// Run Fast Profile validation on `config`, printing warnings and returning an
/// error if any hard violations are present.
fn apply_profile_override_and_validate(
    config: &blackwire_config::schema::Config,
    override_: Option<ProfileMode>,
) -> Result<()> {
    // Build effective config for validation (clone only if override is set).
    let effective_profile = override_.unwrap_or(config.profile);
    if effective_profile != ProfileMode::Fast {
        return Ok(());
    }

    // Temporarily override profile in a clone for validation.
    let validated = if override_.is_some() && config.profile != effective_profile {
        let mut c = config.clone();
        c.profile = effective_profile;
        std::borrow::Cow::Owned(c)
    } else {
        std::borrow::Cow::Borrowed(config)
    };

    let violations = validate_fast_profile(&validated);

    for v in &violations {
        match v {
            ProfileViolation::Warning(msg) => {
                eprintln!("Fast Profile warning: {msg}");
            }
            ProfileViolation::Error(_) => {}
        }
    }

    let errors: Vec<&str> = violations
        .iter()
        .filter(|v| v.is_error())
        .map(|v| v.message())
        .collect();

    if !errors.is_empty() {
        let mut msg = format!(
            "config rejected by Fast Profile ({} error(s)):\n",
            errors.len()
        );
        for e in errors {
            msg.push_str(&format!("  • {e}\n"));
        }
        anyhow::bail!("{}", msg.trim_end());
    }

    Ok(())
}

// ── `x25519` subcommand ───────────────────────────────────────────────────────

/// Generate a fresh X25519 key pair and print it.
///
/// X25519 is the elliptic-curve Diffie-Hellman algorithm used in REALITY.
/// The server holds the private key; the public key goes in client configs.
fn cmd_x25519() {
    use x25519_dalek::{PublicKey, StaticSecret};

    // `StaticSecret` is a long-term key suitable for REALITY configuration.
    // It is generated from the OS CSPRNG and can be serialised to bytes.
    let secret = StaticSecret::random();
    let public = PublicKey::from(&secret);

    // Print as hex so the user can paste them into a JSON config file.
    // The private key stays on the server; the public key goes in client configs.
    println!(
        "Private key (server config): {}",
        hex::encode(secret.to_bytes())
    );
    println!(
        "Public key  (client config): {}",
        hex::encode(public.as_bytes())
    );
}

// ── `uuid` subcommand ─────────────────────────────────────────────────────────

/// Generate a random UUID v4 and print it in the standard dashed format.
///
/// UUID v4 is entirely random (122 random bits). It is used as a VLESS
/// user identifier — each user gets a unique UUID that acts as an
/// authentication token.
fn cmd_uuid() {
    // `uuid::Uuid::new_v4()` generates cryptographically random bytes using
    // the OS CSPRNG and formats them with the version (4) and variant bits set.
    let id = uuid::Uuid::new_v4();
    println!("{id}");
}

// ── `connections` subcommand ─────────────────────────────────────────────────

fn cmd_connections(args: ConnectionsArgs) {
    match args.command {
        ConnectionsCommand::List => print_connections(blackwire_connmgr::global_manager().list()),
        ConnectionsCommand::Top { sort } => {
            let mut snapshots = blackwire_connmgr::global_manager().list();
            match sort {
                ConnectionSort::Bytes => {
                    snapshots.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.total_bytes()));
                }
                ConnectionSort::Age => {
                    snapshots.sort_by(|a, b| {
                        b.age_secs
                            .partial_cmp(&a.age_secs)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
            }
            print_connections(snapshots);
        }
        ConnectionsCommand::Close(args) => {
            let selector = if let Some(id) = args.id {
                blackwire_connmgr::CloseSelector::Id(id)
            } else if let Some(user) = args.user {
                blackwire_connmgr::CloseSelector::User(user)
            } else if let Some(inbound) = args.inbound {
                blackwire_connmgr::CloseSelector::Inbound(inbound)
            } else if let Some(outbound) = args.outbound {
                blackwire_connmgr::CloseSelector::Outbound(outbound)
            } else {
                eprintln!("Error: specify one of --id, --user, --inbound, or --outbound");
                std::process::exit(2);
            };
            let result = blackwire_connmgr::global_manager().close(selector);
            println!("closed {}", result.matched);
        }
    }
}

fn print_connections(snapshots: Vec<blackwire_connmgr::ConnectionSnapshot>) {
    println!(
        "{:<8} {:<14} {:<14} {:<18} {:<9} {:<10} {:>10} {:>10} {:>8}",
        "id", "inbound", "outbound", "user", "protocol", "transport", "up", "down", "age_s"
    );
    for snapshot in snapshots {
        println!(
            "{:<8} {:<14} {:<14} {:<18} {:<9} {:<10} {:>10} {:>10} {:>8.1}",
            snapshot.id,
            snapshot.inbound,
            snapshot.outbound,
            snapshot.user.as_deref().unwrap_or("-"),
            snapshot.protocol.as_str(),
            snapshot.transport.as_str(),
            snapshot.bytes_up,
            snapshot.bytes_down,
            snapshot.age_secs,
        );
    }
}

// ── Logging setup ─────────────────────────────────────────────────────────────

/// Initialise the tracing subscriber for structured logging.
///
/// Log level is controlled by the `RUST_LOG` environment variable.
/// Default level is `info` if `RUST_LOG` is not set.
///
/// Examples:
///   `RUST_LOG=debug blackwire run -c config.json`   — very verbose
///   `RUST_LOG=warn  blackwire run -c config.json`   — warnings only
fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};

    // `EnvFilter::try_from_default_env()` reads `RUST_LOG`.
    // If that env var isn't set, fall back to "info".
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    fmt()
        .with_env_filter(filter)
        // Print timestamps, log level, target module, and the message.
        .with_target(true)
        .with_line_number(false)
        .init();
}
