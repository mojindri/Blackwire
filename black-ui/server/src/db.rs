use std::path::Path;

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::{
    models::{ConfigSection, Inbound, ManagedUser, Outbound, Settings, UserTraffic},
    util,
};

pub fn init(conn: &Connection, data_dir: &Path) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS admins (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            salt TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS sessions (
            token TEXT PRIMARY KEY,
            admin_id INTEGER NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS inbounds (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            tag TEXT NOT NULL UNIQUE,
            listen TEXT NOT NULL,
            port INTEGER NOT NULL,
            protocol TEXT NOT NULL DEFAULT 'vless',
            enabled INTEGER NOT NULL,
            transport TEXT NOT NULL,
            settings TEXT NOT NULL DEFAULT '',
            stream_settings TEXT NOT NULL,
            sniffing TEXT NOT NULL DEFAULT '',
            limits TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS outbounds (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            tag TEXT NOT NULL UNIQUE,
            protocol TEXT NOT NULL,
            enabled INTEGER NOT NULL,
            settings TEXT NOT NULL,
            stream_settings TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS config_sections (
            name TEXT PRIMARY KEY,
            enabled INTEGER NOT NULL,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            inbound_id INTEGER NOT NULL,
            email TEXT NOT NULL UNIQUE,
            uuid TEXT NOT NULL UNIQUE,
            flow TEXT NOT NULL,
            credential_json TEXT NOT NULL DEFAULT '',
            note TEXT NOT NULL,
            enabled INTEGER NOT NULL,
            traffic_limit_bytes INTEGER,
            expiry_at TEXT,
            upload_bytes INTEGER NOT NULL DEFAULT 0,
            download_bytes INTEGER NOT NULL DEFAULT 0,
            sub_token TEXT NOT NULL UNIQUE,
            enforcement_status TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(inbound_id) REFERENCES inbounds(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS user_traffic_cursors (
            user_id INTEGER PRIMARY KEY,
            last_upload_bytes INTEGER NOT NULL DEFAULT 0,
            last_download_bytes INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
        );
        "#,
    )?;
    migrate_existing_schema(conn)?;
    let panel_default_config_path = data_dir.join("config.json").to_string_lossy().to_string();
    let config_path = std::env::var("BLACK_UI_CONFIG_PATH")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| panel_default_config_path.clone());
    set_default(conn, "configPath", &config_path)?;
    migrate_panel_default_config_path(conn, &panel_default_config_path, &config_path)?;
    migrate_ephemeral_config_path(conn, &config_path)?;
    set_default(conn, "grpcEnabled", "true")?;
    set_default(conn, "grpcAddress", "127.0.0.1:62789")?;
    set_default(conn, "firewallAutoOpen", "false")?;
    let public_base_url = std::env::var("BLACK_UI_PUBLIC_BASE_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:18080".into());
    let subscription_host = std::env::var("BLACK_UI_SUBSCRIPTION_HOST")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "127.0.0.1".into());
    set_default(conn, "publicBaseUrl", &public_base_url)?;
    set_default(conn, "subscriptionHost", &subscription_host)?;
    migrate_local_public_settings(conn, &public_base_url, &subscription_host)?;
    set_default(conn, "enforcementIntervalSeconds", "30")?;
    set_default(conn, "adaptiveRoutingEnabled", "false")?;
    set_default(conn, "adaptiveTuningMode", "recommend")?;
    set_default(conn, "adaptiveTuningIntervalSeconds", "600")?;
    set_default(conn, "adaptiveTuningCooldownSeconds", "600")?;
    set_default(conn, "adaptiveTuningMaxHysteria2Mbps", "1000")?;
    set_default(conn, "adaptiveTuningState", "{}")?;
    seed_default_outbound(conn)?;
    seed_default_sections(conn)?;
    enable_fast_dns_for_default_section(conn)?;
    Ok(())
}

fn migrate_panel_default_config_path(
    conn: &Connection,
    panel_default_config_path: &str,
    configured_config_path: &str,
) -> Result<()> {
    if panel_default_config_path == configured_config_path {
        return Ok(());
    }
    conn.execute(
        "UPDATE settings SET value=?1 WHERE key='configPath' AND value=?2",
        params![configured_config_path, panel_default_config_path],
    )?;
    Ok(())
}

fn migrate_ephemeral_config_path(conn: &Connection, configured_config_path: &str) -> Result<()> {
    if is_ephemeral_config_path(configured_config_path) {
        return Ok(());
    }
    let current = conn
        .query_row(
            "SELECT value FROM settings WHERE key='configPath'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if current
        .as_deref()
        .map(is_ephemeral_config_path)
        .unwrap_or(false)
    {
        conn.execute(
            "UPDATE settings SET value=?1 WHERE key='configPath'",
            params![configured_config_path],
        )?;
    }
    Ok(())
}

fn is_ephemeral_config_path(value: &str) -> bool {
    let value = value.trim();
    value.starts_with("/tmp/blackwire-qa")
        || value.starts_with("/var/tmp/blackwire-qa")
        || value.starts_with("/private/tmp/blackwire-qa")
        || value.contains("/black-ui-qa-")
}

fn migrate_existing_schema(conn: &Connection) -> Result<()> {
    add_column_if_missing(
        conn,
        "inbounds",
        "protocol",
        "TEXT NOT NULL DEFAULT 'vless'",
    )?;
    add_column_if_missing(conn, "inbounds", "settings", "TEXT NOT NULL DEFAULT ''")?;
    add_column_if_missing(conn, "inbounds", "sniffing", "TEXT NOT NULL DEFAULT ''")?;
    add_column_if_missing(conn, "inbounds", "limits", "TEXT NOT NULL DEFAULT ''")?;
    add_column_if_missing(conn, "users", "credential_json", "TEXT NOT NULL DEFAULT ''")?;
    Ok(())
}

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
    for row in rows {
        if row? == column {
            return Ok(());
        }
    }
    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )?;
    Ok(())
}

fn seed_default_outbound(conn: &Connection) -> Result<()> {
    let ts = util::now();
    conn.execute(
        "INSERT OR IGNORE INTO outbounds (tag, protocol, enabled, settings, stream_settings, created_at, updated_at)
         VALUES ('freedom', 'freedom', 1, '{}', '', ?1, ?1)",
        params![ts],
    )?;
    Ok(())
}

fn seed_default_sections(conn: &Connection) -> Result<()> {
    let ts = util::now();
    let defaults = [
        ("log", 1, r#"{"level":"info","json":false}"#),
        ("routing", 1, r#"{"rules":[{"outboundTag":"freedom"}]}"#),
        ("dns", 1, r#"{"servers":["1.1.1.1","8.8.8.8"]}"#),
        (
            "tun",
            0,
            r#"{"name":"blackwire-tun","address":"198.18.0.1","netmask":"255.255.0.0","mtu":1500,"bypass_mark":4660,"redirect_port":7890,"dns_port":5300}"#,
        ),
        ("limits", 0, r#"{}"#),
        ("stats", 0, r#"{}"#),
        (
            "api",
            1,
            r#"{"listen":"127.0.0.1:62789","tag":"api","services":["HandlerService","StatsService"]}"#,
        ),
        ("metricsAddr", 0, r#""127.0.0.1:9090""#),
        ("profile", 0, r#""compat""#),
        (
            "fast",
            0,
            r#"{"strictProduction":true,"pool":"disabled","splice":"adaptive"}"#,
        ),
    ];
    for (name, enabled, value) in defaults {
        conn.execute(
            "INSERT OR IGNORE INTO config_sections (name, enabled, value, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![name, enabled, value, ts],
        )?;
    }
    Ok(())
}

fn enable_fast_dns_for_default_section(conn: &Connection) -> Result<()> {
    let ts = util::now();
    conn.execute(
        "UPDATE config_sections
         SET enabled=1, value=?1, updated_at=?2
         WHERE name='dns'
           AND enabled=0
           AND replace(value, ' ', '')='{\"servers\":[]}'",
        params![r#"{"servers":["1.1.1.1","8.8.8.8"]}"#, ts],
    )?;
    Ok(())
}

fn set_default(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

fn migrate_local_public_settings(
    conn: &Connection,
    public_base_url: &str,
    subscription_host: &str,
) -> Result<()> {
    migrate_local_setting(conn, "publicBaseUrl", public_base_url)?;
    migrate_local_setting(conn, "subscriptionHost", subscription_host)?;
    Ok(())
}

fn migrate_local_setting(conn: &Connection, key: &str, env_value: &str) -> Result<()> {
    let env_value = env_value.trim();
    if env_value.is_empty() || is_local_public_setting(env_value) {
        return Ok(());
    }
    let current = conn
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if current
        .as_deref()
        .map(is_local_public_setting)
        .unwrap_or(true)
    {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, env_value],
        )?;
    }
    Ok(())
}

fn is_local_public_setting(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    value.is_empty()
        || value == "127.0.0.1"
        || value == "localhost"
        || value == "0.0.0.0"
        || value.starts_with("http://127.0.0.1")
        || value.starts_with("https://127.0.0.1")
        || value.starts_with("http://localhost")
        || value.starts_with("https://localhost")
        || value.starts_with("http://0.0.0.0")
        || value.starts_with("https://0.0.0.0")
}

pub fn count(conn: &Connection, table: &str) -> Result<i64> {
    Ok(conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?)
}

pub fn setup_required(conn: &Connection) -> Result<bool> {
    Ok(count(conn, "admins")? == 0)
}

pub fn load_settings(conn: &Connection) -> Result<Settings> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut map = std::collections::HashMap::new();
    for row in rows {
        let (k, v) = row?;
        map.insert(k, v);
    }
    Ok(Settings {
        config_path: map
            .get("configPath")
            .cloned()
            .unwrap_or_else(|| "black-ui/data/config.json".into()),
        grpc_enabled: map.get("grpcEnabled").map(|v| v == "true").unwrap_or(true),
        grpc_address: map
            .get("grpcAddress")
            .cloned()
            .unwrap_or_else(|| "127.0.0.1:62789".into()),
        firewall_auto_open: map
            .get("firewallAutoOpen")
            .map(|v| v == "true")
            .unwrap_or(false),
        public_base_url: map
            .get("publicBaseUrl")
            .cloned()
            .unwrap_or_else(|| "http://127.0.0.1:18080".into()),
        subscription_host: map
            .get("subscriptionHost")
            .cloned()
            .unwrap_or_else(|| "127.0.0.1".into()),
        enforcement_interval_seconds: map
            .get("enforcementIntervalSeconds")
            .and_then(|v| v.parse().ok())
            .unwrap_or(30),
        adaptive_routing_enabled: map
            .get("adaptiveRoutingEnabled")
            .map(|v| v == "true")
            .unwrap_or(false),
        adaptive_tuning_mode: map
            .get("adaptiveTuningMode")
            .cloned()
            .unwrap_or_else(|| "recommend".into()),
        adaptive_tuning_interval_seconds: map
            .get("adaptiveTuningIntervalSeconds")
            .and_then(|v| v.parse().ok())
            .unwrap_or(600),
        adaptive_tuning_cooldown_seconds: map
            .get("adaptiveTuningCooldownSeconds")
            .and_then(|v| v.parse().ok())
            .unwrap_or(600),
        adaptive_tuning_max_hysteria2_mbps: map
            .get("adaptiveTuningMaxHysteria2Mbps")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1000),
        adaptive_tuning_state: map
            .get("adaptiveTuningState")
            .and_then(|v| serde_json::from_str(v).ok())
            .unwrap_or_else(|| serde_json::json!({})),
    })
}

pub fn save_settings(conn: &Connection, settings: &Settings) -> Result<()> {
    let rows = [
        ("configPath", settings.config_path.clone()),
        ("grpcEnabled", settings.grpc_enabled.to_string()),
        ("grpcAddress", settings.grpc_address.clone()),
        ("firewallAutoOpen", settings.firewall_auto_open.to_string()),
        ("publicBaseUrl", settings.public_base_url.clone()),
        ("subscriptionHost", settings.subscription_host.clone()),
        (
            "enforcementIntervalSeconds",
            settings.enforcement_interval_seconds.to_string(),
        ),
        (
            "adaptiveRoutingEnabled",
            settings.adaptive_routing_enabled.to_string(),
        ),
        ("adaptiveTuningMode", settings.adaptive_tuning_mode.clone()),
        (
            "adaptiveTuningIntervalSeconds",
            settings.adaptive_tuning_interval_seconds.to_string(),
        ),
        (
            "adaptiveTuningCooldownSeconds",
            settings.adaptive_tuning_cooldown_seconds.to_string(),
        ),
        (
            "adaptiveTuningMaxHysteria2Mbps",
            settings.adaptive_tuning_max_hysteria2_mbps.to_string(),
        ),
        (
            "adaptiveTuningState",
            serde_json::to_string(&settings.adaptive_tuning_state).unwrap_or_else(|_| "{}".into()),
        ),
    ];
    for (key, value) in rows {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
    }
    Ok(())
}

pub fn save_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn load_inbounds(conn: &Connection) -> Result<Vec<Inbound>> {
    let mut stmt = conn.prepare(
        "SELECT id, tag, listen, port, protocol, enabled, transport, settings, stream_settings, sniffing, limits, created_at, updated_at
         FROM inbounds ORDER BY id",
    )?;
    let rows = stmt.query_map([], row_inbound)?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

pub fn load_inbound(conn: &Connection, id: i64) -> Result<Option<Inbound>> {
    conn.query_row(
        "SELECT id, tag, listen, port, protocol, enabled, transport, settings, stream_settings, sniffing, limits, created_at, updated_at
         FROM inbounds WHERE id=?1",
        params![id],
        row_inbound,
    )
    .optional()
    .map_err(Into::into)
}

fn row_inbound(r: &Row<'_>) -> rusqlite::Result<Inbound> {
    Ok(Inbound {
        id: r.get(0)?,
        tag: r.get(1)?,
        listen: r.get(2)?,
        port: r.get::<_, i64>(3)? as u16,
        protocol: r.get(4)?,
        enabled: r.get::<_, i64>(5)? == 1,
        transport: r.get(6)?,
        settings: r.get(7)?,
        stream_settings: r.get(8)?,
        sniffing: r.get(9)?,
        limits: r.get(10)?,
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
    })
}

pub fn load_outbounds(conn: &Connection) -> Result<Vec<Outbound>> {
    let mut stmt = conn.prepare(
        "SELECT id, tag, protocol, enabled, settings, stream_settings, created_at, updated_at
         FROM outbounds ORDER BY id",
    )?;
    let rows = stmt.query_map([], row_outbound)?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn row_outbound(r: &Row<'_>) -> rusqlite::Result<Outbound> {
    Ok(Outbound {
        id: r.get(0)?,
        tag: r.get(1)?,
        protocol: r.get(2)?,
        enabled: r.get::<_, i64>(3)? == 1,
        settings: r.get(4)?,
        stream_settings: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
    })
}

pub fn load_sections(conn: &Connection) -> Result<Vec<ConfigSection>> {
    let mut stmt =
        conn.prepare("SELECT name, enabled, value, updated_at FROM config_sections ORDER BY name")?;
    let rows = stmt.query_map([], |r| {
        Ok(ConfigSection {
            name: r.get(0)?,
            enabled: r.get::<_, i64>(1)? == 1,
            value: r.get(2)?,
            updated_at: r.get(3)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

pub fn load_section_map(
    conn: &Connection,
) -> Result<std::collections::HashMap<String, ConfigSection>> {
    Ok(load_sections(conn)?
        .into_iter()
        .map(|section| (section.name.clone(), section))
        .collect())
}

pub fn load_users(conn: &Connection) -> Result<Vec<ManagedUser>> {
    let mut stmt = conn.prepare(
        "SELECT id, inbound_id, email, uuid, flow, credential_json, note, enabled, traffic_limit_bytes, expiry_at,
         upload_bytes, download_bytes, sub_token, enforcement_status, created_at, updated_at
         FROM users ORDER BY id",
    )?;
    let rows = stmt.query_map([], row_user)?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

pub fn load_user(conn: &Connection, id: i64) -> Result<Option<ManagedUser>> {
    conn.query_row(
        "SELECT id, inbound_id, email, uuid, flow, credential_json, note, enabled, traffic_limit_bytes, expiry_at,
         upload_bytes, download_bytes, sub_token, enforcement_status, created_at, updated_at
         FROM users WHERE id=?1",
        params![id],
        row_user,
    )
    .optional()
    .map_err(Into::into)
}

pub fn load_user_by_token(conn: &Connection, token: &str) -> Result<Option<ManagedUser>> {
    conn.query_row(
        "SELECT id, inbound_id, email, uuid, flow, credential_json, note, enabled, traffic_limit_bytes, expiry_at,
         upload_bytes, download_bytes, sub_token, enforcement_status, created_at, updated_at
         FROM users WHERE sub_token=?1",
        params![token],
        row_user,
    )
    .optional()
    .map_err(Into::into)
}

fn row_user(r: &Row<'_>) -> rusqlite::Result<ManagedUser> {
    let credential_raw: String = r.get(5)?;
    let credential = if credential_raw.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(&credential_raw).unwrap_or_else(|_| serde_json::json!({}))
    };
    Ok(ManagedUser {
        id: r.get(0)?,
        inbound_id: r.get(1)?,
        email: r.get(2)?,
        uuid: r.get(3)?,
        flow: r.get(4)?,
        credential,
        note: r.get(6)?,
        enabled: r.get::<_, i64>(7)? == 1,
        traffic_limit_bytes: r.get(8)?,
        expiry_at: r.get(9)?,
        upload_bytes: r.get(10)?,
        download_bytes: r.get(11)?,
        sub_token: r.get(12)?,
        enforcement_status: r.get(13)?,
        created_at: r.get(14)?,
        updated_at: r.get(15)?,
    })
}

pub fn touch_user_status(conn: &Connection, id: i64, enabled: bool, status: &str) -> Result<()> {
    conn.execute(
        "UPDATE users SET enabled=?1, enforcement_status=?2, updated_at=?3 WHERE id=?4",
        params![util::bool_i(enabled), status, util::now(), id],
    )?;
    Ok(())
}

pub fn reset_user_usage(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE users SET upload_bytes=0, download_bytes=0, updated_at=?1 WHERE id=?2",
        params![util::now(), id],
    )?;
    conn.execute(
        "DELETE FROM user_traffic_cursors WHERE user_id=?1",
        params![id],
    )?;
    Ok(())
}

pub fn apply_user_traffic_snapshot(conn: &Connection, users: &[UserTraffic]) -> Result<()> {
    let now = util::now();
    for traffic in users {
        let Some((user_id, stored_upload, stored_download)) = conn
            .query_row(
                "SELECT id, upload_bytes, download_bytes FROM users WHERE email=?1",
                params![traffic.email],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?
        else {
            continue;
        };

        let raw_upload = traffic.upload_bytes.max(0);
        let raw_download = traffic.download_bytes.max(0);
        let cursor = conn
            .query_row(
                "SELECT last_upload_bytes, last_download_bytes FROM user_traffic_cursors WHERE user_id=?1",
                params![user_id],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()?;

        let (next_upload, next_download) = match cursor {
            Some((last_upload, last_download)) => {
                let upload_delta = if raw_upload >= last_upload {
                    raw_upload - last_upload
                } else {
                    0
                };
                let download_delta = if raw_download >= last_download {
                    raw_download - last_download
                } else {
                    0
                };
                (
                    stored_upload.saturating_add(upload_delta),
                    stored_download.saturating_add(download_delta),
                )
            }
            None => (
                stored_upload.max(raw_upload),
                stored_download.max(raw_download),
            ),
        };

        conn.execute(
            "UPDATE users SET upload_bytes=?1, download_bytes=?2 WHERE id=?3",
            params![next_upload, next_download, user_id],
        )?;
        conn.execute(
            "INSERT INTO user_traffic_cursors (user_id, last_upload_bytes, last_download_bytes, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(user_id) DO UPDATE SET
                last_upload_bytes=excluded.last_upload_bytes,
                last_download_bytes=excluded.last_download_bytes,
                updated_at=excluded.updated_at",
            params![user_id, raw_upload, raw_download, now],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_migrates_prototype_schema_without_losing_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE admins (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL UNIQUE,
                password_hash TEXT NOT NULL,
                salt TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE TABLE sessions (
                token TEXT PRIMARY KEY,
                admin_id INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE TABLE settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE inbounds (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tag TEXT NOT NULL UNIQUE,
                listen TEXT NOT NULL,
                port INTEGER NOT NULL,
                enabled INTEGER NOT NULL,
                transport TEXT NOT NULL,
                stream_settings TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                inbound_id INTEGER NOT NULL,
                email TEXT NOT NULL UNIQUE,
                uuid TEXT NOT NULL UNIQUE,
                flow TEXT NOT NULL,
                note TEXT NOT NULL,
                enabled INTEGER NOT NULL,
                traffic_limit_bytes INTEGER,
                expiry_at TEXT,
                upload_bytes INTEGER NOT NULL DEFAULT 0,
                download_bytes INTEGER NOT NULL DEFAULT 0,
                sub_token TEXT NOT NULL UNIQUE,
                enforcement_status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY(inbound_id) REFERENCES inbounds(id) ON DELETE CASCADE
            );
            INSERT INTO inbounds (tag, listen, port, enabled, transport, stream_settings, created_at, updated_at)
            VALUES ('old', '127.0.0.1', 443, 1, 'ws', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
            "#,
        )
        .unwrap();

        init(&conn, Path::new("/tmp/black-ui-db-migration-test")).unwrap();

        let inbound = load_inbounds(&conn).unwrap().remove(0);
        assert_eq!(inbound.tag, "old");
        assert_eq!(inbound.protocol, "vless");
        assert_eq!(inbound.settings, "");
        assert_eq!(count(&conn, "outbounds").unwrap(), 1);
        assert_eq!(count(&conn, "config_sections").unwrap(), 10);
        let dns = load_section_map(&conn).unwrap().remove("dns").unwrap();
        assert!(dns.enabled);
        assert_eq!(dns.value, r#"{"servers":["1.1.1.1","8.8.8.8"]}"#);
    }

    #[test]
    fn custom_dns_section_is_not_overwritten_on_init() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn, Path::new("/tmp/black-ui-db-custom-dns-test")).unwrap();
        let ts = util::now();
        conn.execute(
            "UPDATE config_sections SET enabled=1, value=?1, updated_at=?2 WHERE name='dns'",
            params![r#"{"servers":["9.9.9.9"]}"#, ts],
        )
        .unwrap();

        init(&conn, Path::new("/tmp/black-ui-db-custom-dns-test")).unwrap();

        let dns = load_section_map(&conn).unwrap().remove("dns").unwrap();
        assert!(dns.enabled);
        assert_eq!(dns.value, r#"{"servers":["9.9.9.9"]}"#);
    }

    #[test]
    fn init_uses_packaged_service_config_path_without_clobbering_custom_paths() {
        let conn = Connection::open_in_memory().unwrap();
        let data_dir = Path::new("/tmp/black-ui-db-config-path-test");
        let panel_default = data_dir.join("config.json").to_string_lossy().to_string();

        init(&conn, data_dir).unwrap();
        migrate_panel_default_config_path(&conn, &panel_default, "/etc/blackwire/config.json")
            .unwrap();
        assert_eq!(
            load_settings(&conn).unwrap().config_path,
            "/etc/blackwire/config.json"
        );

        conn.execute(
            "UPDATE settings SET value=?1 WHERE key='configPath'",
            params!["/srv/custom/blackwire.json"],
        )
        .unwrap();
        migrate_panel_default_config_path(&conn, &panel_default, "/etc/blackwire/config.json")
            .unwrap();
        assert_eq!(
            load_settings(&conn).unwrap().config_path,
            "/srv/custom/blackwire.json"
        );
    }

    #[test]
    fn init_migrates_ephemeral_qa_config_path_to_packaged_path() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn, Path::new("/tmp/black-ui-db-qa-config-path-test")).unwrap();
        conn.execute(
            "UPDATE settings SET value=?1 WHERE key='configPath'",
            params!["/tmp/blackwire-qa-config.json"],
        )
        .unwrap();

        migrate_ephemeral_config_path(&conn, "/etc/blackwire/config.json").unwrap();

        assert_eq!(
            load_settings(&conn).unwrap().config_path,
            "/etc/blackwire/config.json"
        );
    }

    #[test]
    fn init_preserves_custom_config_path_during_ephemeral_migration() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn, Path::new("/tmp/black-ui-db-custom-config-path-test")).unwrap();
        conn.execute(
            "UPDATE settings SET value=?1 WHERE key='configPath'",
            params!["/srv/custom/blackwire.json"],
        )
        .unwrap();

        migrate_ephemeral_config_path(&conn, "/etc/blackwire/config.json").unwrap();

        assert_eq!(
            load_settings(&conn).unwrap().config_path,
            "/srv/custom/blackwire.json"
        );
    }

    #[test]
    fn public_link_env_migration_replaces_local_defaults_only() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn, Path::new("/tmp/black-ui-db-public-link-test")).unwrap();

        migrate_local_public_settings(&conn, "http://203.0.113.10:18080", "203.0.113.10")
            .unwrap();
        let migrated = load_settings(&conn).unwrap();
        assert_eq!(migrated.public_base_url, "http://203.0.113.10:18080");
        assert_eq!(migrated.subscription_host, "203.0.113.10");

        conn.execute(
            "UPDATE settings SET value=?1 WHERE key='publicBaseUrl'",
            params!["https://panel.example.com"],
        )
        .unwrap();
        conn.execute(
            "UPDATE settings SET value=?1 WHERE key='subscriptionHost'",
            params!["sub.example.com"],
        )
        .unwrap();
        migrate_local_public_settings(&conn, "http://203.0.113.10:18080", "203.0.113.10").unwrap();
        let preserved = load_settings(&conn).unwrap();
        assert_eq!(preserved.public_base_url, "https://panel.example.com");
        assert_eq!(preserved.subscription_host, "sub.example.com");
    }

    #[test]
    fn traffic_snapshots_accumulate_deltas_without_losing_restart_history() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn, Path::new("/tmp/black-ui-db-traffic-test")).unwrap();
        let now = util::now();
        conn.execute(
            "INSERT INTO inbounds (tag, listen, port, protocol, enabled, transport, settings, stream_settings, sniffing, limits, created_at, updated_at)
             VALUES ('in', '127.0.0.1', 443, 'vless', 1, 'ws', '', '{}', '', '', ?1, ?1)",
            params![now],
        )
        .unwrap();
        let inbound_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO users (inbound_id, email, uuid, flow, credential_json, note, enabled, traffic_limit_bytes, expiry_at, upload_bytes, download_bytes, sub_token, enforcement_status, created_at, updated_at)
             VALUES (?1, 'quota@example.local', '11111111-1111-4111-8111-111111111111', '', '{}', '', 1, NULL, NULL, 0, 0, 'token', 'active', ?2, ?2)",
            params![inbound_id, now],
        )
        .unwrap();

        apply_user_traffic_snapshot(
            &conn,
            &[UserTraffic {
                email: "quota@example.local".into(),
                upload_bytes: 100,
                download_bytes: 900,
            }],
        )
        .unwrap();
        let user = load_users(&conn).unwrap().remove(0);
        assert_eq!((user.upload_bytes, user.download_bytes), (100, 900));

        apply_user_traffic_snapshot(
            &conn,
            &[UserTraffic {
                email: "quota@example.local".into(),
                upload_bytes: 250,
                download_bytes: 1200,
            }],
        )
        .unwrap();
        let user = load_users(&conn).unwrap().remove(0);
        assert_eq!((user.upload_bytes, user.download_bytes), (250, 1200));

        apply_user_traffic_snapshot(
            &conn,
            &[UserTraffic {
                email: "quota@example.local".into(),
                upload_bytes: 10,
                download_bytes: 20,
            }],
        )
        .unwrap();
        let user = load_users(&conn).unwrap().remove(0);
        assert_eq!((user.upload_bytes, user.download_bytes), (250, 1200));

        apply_user_traffic_snapshot(
            &conn,
            &[UserTraffic {
                email: "quota@example.local".into(),
                upload_bytes: 40,
                download_bytes: 70,
            }],
        )
        .unwrap();
        let user = load_users(&conn).unwrap().remove(0);
        assert_eq!((user.upload_bytes, user.download_bytes), (280, 1250));
    }

    #[test]
    fn reset_usage_clears_runtime_cursor() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn, Path::new("/tmp/black-ui-db-reset-traffic-test")).unwrap();
        let now = util::now();
        conn.execute(
            "INSERT INTO inbounds (tag, listen, port, protocol, enabled, transport, settings, stream_settings, sniffing, limits, created_at, updated_at)
             VALUES ('in', '127.0.0.1', 443, 'vless', 1, 'ws', '', '{}', '', '', ?1, ?1)",
            params![now],
        )
        .unwrap();
        let inbound_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO users (inbound_id, email, uuid, flow, credential_json, note, enabled, traffic_limit_bytes, expiry_at, upload_bytes, download_bytes, sub_token, enforcement_status, created_at, updated_at)
             VALUES (?1, 'quota-reset@example.local', '22222222-2222-4222-8222-222222222222', '', '{}', '', 1, NULL, NULL, 0, 0, 'token-reset', 'active', ?2, ?2)",
            params![inbound_id, now],
        )
        .unwrap();
        let user_id = conn.last_insert_rowid();

        apply_user_traffic_snapshot(
            &conn,
            &[UserTraffic {
                email: "quota-reset@example.local".into(),
                upload_bytes: 100,
                download_bytes: 900,
            }],
        )
        .unwrap();
        reset_user_usage(&conn, user_id).unwrap();

        let user = load_users(&conn).unwrap().remove(0);
        assert_eq!((user.upload_bytes, user.download_bytes), (0, 0));
        let cursor_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM user_traffic_cursors WHERE user_id=?1",
                params![user_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cursor_count, 0);
    }
}
