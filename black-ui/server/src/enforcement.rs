use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::params;
use tracing::warn;

use crate::{config, db, runtime, state::AppState, util};

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let mut force_live_sync = true;
        loop {
            let interval = match state.db.lock() {
                Ok(conn) => db::load_settings(&conn)
                    .map(|s| s.enforcement_interval_seconds)
                    .unwrap_or(30)
                    .max(5),
                Err(_) => 30,
            };
            tokio::time::sleep(Duration::from_secs(interval)).await;
            if let Err(e) = run_once_with_options(&state, force_live_sync).await {
                warn!(error = %e, "quota/expiry enforcement failed");
            }
            force_live_sync = false;
        }
    });
}

pub(crate) async fn run_once(state: &AppState) -> Result<()> {
    run_once_with_options(state, false).await
}

pub(crate) async fn run_startup_once(state: &AppState) -> Result<()> {
    run_once_with_options(state, true).await
}

async fn run_once_with_options(state: &AppState, force_live_sync: bool) -> Result<()> {
    let settings = {
        let conn = state.lock_db()?;
        db::load_settings(&conn)?
    };

    refresh_traffic_with_settings(state, &settings).await?;
    let changed = enforce_limits(state).await?;
    reconcile_config(state, &settings, changed || force_live_sync).await
}

async fn refresh_traffic_with_settings(
    state: &AppState,
    settings: &crate::models::Settings,
) -> Result<()> {
    if settings.grpc_enabled {
        if let Ok(snapshot) = runtime::fetch_traffic(&settings.grpc_address).await {
            let conn = state.lock_db()?;
            db::apply_user_traffic_snapshot(&conn, &snapshot.users)?;
        }
    }
    Ok(())
}

async fn enforce_limits(state: &AppState) -> Result<bool> {
    let mut changed = false;
    {
        let conn = state.lock_db()?;
        for user in db::load_users(&conn)? {
            if !user.enabled {
                continue;
            }
            let mut status = None;
            if let Some(limit) = user.traffic_limit_bytes {
                if limit > 0 && user.upload_bytes + user.download_bytes >= limit {
                    status = Some("quota exceeded");
                }
            }
            if status.is_none() {
                if let Some(expiry) = &user.expiry_at {
                    if DateTime::parse_from_rfc3339(expiry)?.with_timezone(&Utc) <= Utc::now() {
                        status = Some("expired");
                    }
                }
            }
            if let Some(status) = status {
                conn.execute(
                    "UPDATE users SET enabled=0, enforcement_status=?1, updated_at=?2 WHERE id=?3",
                    params![status, util::now(), user.id],
                )?;
                changed = true;
            }
        }
    }

    Ok(changed)
}

async fn reconcile_config(
    state: &AppState,
    settings: &crate::models::Settings,
    force_live_sync: bool,
) -> Result<()> {
    let desired = config::build_value(state)?;
    let file_needs_write = std::fs::read_to_string(&settings.config_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .map_or(true, |current| current != desired);

    let mut sync_needed = force_live_sync;
    if file_needs_write {
        if let Err(e) = config::write(state) {
            warn!(error = %e, "enforcement: config write failed");
        } else {
            sync_needed = true;
        }
    }

    if sync_needed && settings.grpc_enabled {
        if let Err(e) = runtime::sync_config(state, &settings.grpc_address).await {
            warn!(error = %e, "enforcement: live sync failed after user enforcement");
        }
    }
    Ok(())
}
