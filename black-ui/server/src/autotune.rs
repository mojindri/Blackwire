use std::{collections::HashMap, fs, time::Duration};

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::params;
use serde_json::{json, Value};
use tracing::warn;

use crate::{config, db, runtime, state::AppState, util};

const MIN_HYSTERIA2_MBPS: u64 = 100;
const ACTIVE_TRAFFIC_BYTES: i64 = 5 * 1024 * 1024;
const HEADROOM_TRAFFIC_BYTES: i64 = 20 * 1024 * 1024;

pub(crate) async fn run_startup_once(state: &AppState) -> Result<()> {
    run_once(state, true).await
}

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        loop {
            let interval = match state.db.lock() {
                Ok(conn) => db::load_settings(&conn)
                    .map(|s| s.adaptive_tuning_interval_seconds)
                    .unwrap_or(600)
                    .max(60),
                Err(_) => 600,
            };
            tokio::time::sleep(Duration::from_secs(interval)).await;
            if let Err(e) = run_once(&state, false).await {
                warn!(error = %e, "adaptive tuning failed");
            }
        }
    });
}

async fn run_once(state: &AppState, startup: bool) -> Result<()> {
    let settings = {
        let conn = state.lock_db()?;
        db::load_settings(&conn)?
    };
    let mode = settings.adaptive_tuning_mode.trim().to_ascii_lowercase();
    if mode == "off" {
        return Ok(());
    }

    let now = Utc::now();
    let previous = settings.adaptive_tuning_state.clone();
    if !startup && in_cooldown(&previous, now, settings.adaptive_tuning_cooldown_seconds) {
        return Ok(());
    }

    let cpu_count = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    let cpu = sample_blackwire_cpu(previous.get("cpuSample"));
    let traffic_refresh_error = refresh_traffic(state, &settings).await;
    let inbounds;
    let users;
    {
        let conn = state.lock_db()?;
        inbounds = db::load_inbounds(&conn)?;
        users = db::load_users(&conn)?;
    }

    let previous_bytes = previous
        .get("inboundBytes")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let current_bytes = inbound_user_bytes(&users);
    let mut actions = Vec::new();
    let mut changed = false;

    for inbound in inbounds
        .iter()
        .filter(|inbound| inbound.enabled && inbound.protocol == "hysteria2")
    {
        let current_total = *current_bytes.get(&inbound.id).unwrap_or(&0);
        let previous_total = previous_bytes
            .get(&inbound.tag)
            .and_then(Value::as_i64)
            .unwrap_or(current_total);
        let traffic_delta = current_total.saturating_sub(previous_total);
        let settings_json = parse_object(&inbound.settings);
        let recommendation = recommend_hysteria2(
            &settings_json,
            cpu_count,
            settings.adaptive_tuning_max_hysteria2_mbps,
            cpu.percent,
            traffic_delta,
        );

        let mut action = json!({
            "tag": inbound.tag,
            "protocol": inbound.protocol,
            "trafficDeltaBytes": traffic_delta,
            "cpuPercent": cpu.percent,
            "current": {
                "upMbps": recommendation.current_up,
                "downMbps": recommendation.current_down,
            },
            "recommended": {
                "upMbps": recommendation.target_up,
                "downMbps": recommendation.target_down,
            },
            "action": recommendation.action,
            "reason": recommendation.reason,
            "applied": false,
        });

        if mode == "auto" && recommendation.should_apply() {
            let mut next_settings = settings_json;
            next_settings["upMbps"] = json!(recommendation.target_up);
            next_settings["downMbps"] = json!(recommendation.target_down);
            let raw = serde_json::to_string(&next_settings)?;
            {
                let conn = state.lock_db()?;
                conn.execute(
                    "UPDATE inbounds SET settings=?1, updated_at=?2 WHERE id=?3",
                    params![raw, util::now(), inbound.id],
                )?;
            }
            changed = true;
            action["applied"] = json!(true);
        }
        actions.push(action);
    }

    let inbound_bytes_json = inbounds
        .iter()
        .map(|inbound| {
            (
                inbound.tag.clone(),
                json!(*current_bytes.get(&inbound.id).unwrap_or(&0)),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let mut state_json = json!({
        "lastRunAt": now.to_rfc3339(),
        "mode": mode,
        "startup": startup,
        "cpuCount": cpu_count,
        "cpuPercent": cpu.percent,
        "cpuSample": cpu.next_sample,
        "trafficRefreshError": traffic_refresh_error,
        "inboundBytes": inbound_bytes_json,
        "actions": actions,
    });

    if changed {
        config::write(state)?;
        state_json["configWritten"] = json!(true);
        if settings.grpc_enabled && runtime::probe(&settings.grpc_address).await {
            match runtime::sync_config(state, &settings.grpc_address).await {
                Ok(()) => state_json["liveApplied"] = json!(true),
                Err(e) => state_json["liveApplyError"] = json!(e.to_string()),
            }
        }
        state_json["lastAppliedAt"] = json!(now.to_rfc3339());
    } else if let Some(last_applied) = previous.get("lastAppliedAt") {
        state_json["lastAppliedAt"] = last_applied.clone();
    }

    let conn = state.lock_db()?;
    db::save_setting(
        &conn,
        "adaptiveTuningState",
        &serde_json::to_string(&state_json)?,
    )?;
    Ok(())
}

async fn refresh_traffic(state: &AppState, settings: &crate::models::Settings) -> Option<String> {
    if !settings.grpc_enabled {
        return None;
    }
    match runtime::fetch_traffic(&settings.grpc_address).await {
        Ok(snapshot) => match state.lock_db() {
            Ok(conn) => db::apply_user_traffic_snapshot(&conn, &snapshot.users)
                .err()
                .map(|e| e.to_string()),
            Err(e) => Some(e.to_string()),
        },
        Err(e) => Some(e.to_string()),
    }
}

fn in_cooldown(previous: &Value, now: DateTime<Utc>, cooldown_seconds: u64) -> bool {
    previous
        .get("lastAppliedAt")
        .and_then(Value::as_str)
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|last| {
            now.signed_duration_since(last.with_timezone(&Utc))
                .num_seconds()
                < cooldown_seconds as i64
        })
        .unwrap_or(false)
}

fn inbound_user_bytes(users: &[crate::models::ManagedUser]) -> HashMap<i64, i64> {
    let mut totals = HashMap::new();
    for user in users {
        *totals.entry(user.inbound_id).or_insert(0) += user.upload_bytes + user.download_bytes;
    }
    totals
}

fn parse_object(raw: &str) -> Value {
    serde_json::from_str::<Value>(raw)
        .ok()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

#[derive(Debug, Clone, PartialEq)]
struct Hysteria2Recommendation {
    current_up: u64,
    current_down: u64,
    target_up: u64,
    target_down: u64,
    action: &'static str,
    reason: String,
}

impl Hysteria2Recommendation {
    fn should_apply(&self) -> bool {
        self.current_up != self.target_up || self.current_down != self.target_down
    }
}

fn recommend_hysteria2(
    settings: &Value,
    cpu_count: usize,
    configured_max_mbps: u64,
    cpu_percent: Option<f64>,
    traffic_delta_bytes: i64,
) -> Hysteria2Recommendation {
    let current_up = settings
        .get("upMbps")
        .and_then(Value::as_u64)
        .unwrap_or(100);
    let current_down = settings
        .get("downMbps")
        .and_then(Value::as_u64)
        .unwrap_or(100);
    let current = current_up.max(current_down);
    let max_mbps = configured_max_mbps.max(MIN_HYSTERIA2_MBPS);
    let host_baseline = host_hysteria2_baseline(cpu_count).min(max_mbps);

    let (target, action, reason) = if current <= 100 && host_baseline > current {
        (
            host_baseline,
            "raise_default_bandwidth",
            format!("default Hysteria2 bandwidth is below host baseline for {cpu_count} CPU(s)"),
        )
    } else if traffic_delta_bytes >= ACTIVE_TRAFFIC_BYTES
        && cpu_percent.is_some_and(|cpu| cpu >= 90.0)
    {
        (
            ((current as f64) * 0.8).round() as u64,
            "lower_cpu_pressure",
            "Blackwire CPU pressure is high during active Hysteria2 traffic".into(),
        )
    } else if traffic_delta_bytes >= HEADROOM_TRAFFIC_BYTES
        && cpu_percent.is_some_and(|cpu| cpu < 60.0)
        && current < host_baseline
    {
        (
            ((current as f64) * 1.1).round() as u64,
            "raise_with_headroom",
            "CPU headroom is available during active Hysteria2 traffic".into(),
        )
    } else {
        (
            current,
            "hold",
            "settings are within the safe adaptive band".into(),
        )
    };

    let target = round_mbps(target.clamp(MIN_HYSTERIA2_MBPS, max_mbps));
    Hysteria2Recommendation {
        current_up,
        current_down,
        target_up: target,
        target_down: target,
        action,
        reason,
    }
}

fn host_hysteria2_baseline(cpu_count: usize) -> u64 {
    match cpu_count {
        0 | 1 => 500,
        2 => 800,
        _ => 1000,
    }
}

fn round_mbps(value: u64) -> u64 {
    ((value + 5) / 10) * 10
}

#[derive(Debug, Default)]
struct CpuSample {
    percent: Option<f64>,
    next_sample: Value,
}

fn sample_blackwire_cpu(previous: Option<&Value>) -> CpuSample {
    let Some((pid, process_ticks)) = blackwire_process_ticks() else {
        return CpuSample::default();
    };
    let Some(total_ticks) = total_cpu_ticks() else {
        return CpuSample::default();
    };
    let previous_process = previous
        .and_then(|v| v.get("processTicks"))
        .and_then(Value::as_u64);
    let previous_total = previous
        .and_then(|v| v.get("totalTicks"))
        .and_then(Value::as_u64);
    let previous_pid = previous.and_then(|v| v.get("pid")).and_then(Value::as_u64);
    let cpu_count = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1) as f64;
    let percent = match (previous_pid, previous_process, previous_total) {
        (Some(old_pid), Some(old_process), Some(old_total)) if old_pid == pid as u64 => {
            let process_delta = process_ticks.saturating_sub(old_process);
            let total_delta = total_ticks.saturating_sub(old_total);
            (total_delta > 0)
                .then(|| (process_delta as f64 / total_delta as f64) * cpu_count * 100.0)
        }
        _ => None,
    };
    CpuSample {
        percent,
        next_sample: json!({
            "pid": pid,
            "processTicks": process_ticks,
            "totalTicks": total_ticks,
        }),
    }
}

fn total_cpu_ticks() -> Option<u64> {
    let raw = fs::read_to_string("/proc/stat").ok()?;
    let line = raw.lines().next()?;
    let mut parts = line.split_whitespace();
    (parts.next()? == "cpu").then_some(())?;
    Some(parts.filter_map(|part| part.parse::<u64>().ok()).sum())
}

fn blackwire_process_ticks() -> Option<(u32, u64)> {
    for entry in fs::read_dir("/proc").ok()? {
        let entry = entry.ok()?;
        let pid = entry.file_name().to_string_lossy().parse::<u32>().ok()?;
        let comm = fs::read_to_string(entry.path().join("comm")).ok()?;
        if comm.trim() != "blackwire" {
            continue;
        }
        let stat = fs::read_to_string(entry.path().join("stat")).ok()?;
        let end = stat.rfind(") ")?;
        let parts = stat[end + 2..].split_whitespace().collect::<Vec<_>>();
        let utime = parts.get(11)?.parse::<u64>().ok()?;
        let stime = parts.get(12)?.parse::<u64>().ok()?;
        return Some((pid, utime + stime));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hysteria2_recommendation_raises_default_bandwidth() {
        let rec = recommend_hysteria2(&json!({"auth":"secret"}), 1, 1000, None, 0);

        assert_eq!(rec.action, "raise_default_bandwidth");
        assert_eq!((rec.target_up, rec.target_down), (500, 500));
    }

    #[test]
    fn hysteria2_recommendation_lowers_on_cpu_pressure() {
        let rec = recommend_hysteria2(
            &json!({"upMbps":500,"downMbps":500}),
            1,
            1000,
            Some(95.0),
            ACTIVE_TRAFFIC_BYTES,
        );

        assert_eq!(rec.action, "lower_cpu_pressure");
        assert_eq!((rec.target_up, rec.target_down), (400, 400));
    }

    #[test]
    fn hysteria2_recommendation_holds_without_traffic() {
        let rec = recommend_hysteria2(
            &json!({"upMbps":500,"downMbps":500}),
            1,
            1000,
            Some(95.0),
            0,
        );

        assert_eq!(rec.action, "hold");
        assert_eq!((rec.target_up, rec.target_down), (500, 500));
    }
}
