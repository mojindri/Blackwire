use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::{Database, StoreResult};

#[derive(Debug, Clone)]
pub struct AdminRecord {
    pub id: i64,
    pub username: String,
    pub password_hash: Vec<u8>,
    pub password_salt: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelSettings {
    pub public_base_url: String,
    pub subscription_host: String,
    pub firewall_auto_open: bool,
    pub enforcement_interval_seconds: u64,
    pub adaptive_routing_enabled: bool,
    pub adaptive_tuning_mode: String,
    pub adaptive_tuning_interval_seconds: u64,
    pub adaptive_tuning_cooldown_seconds: u64,
    pub adaptive_tuning_max_hysteria2_mbps: u64,
}

impl Database {
    pub async fn setup_required(&self) -> StoreResult<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM panel_admins")
            .fetch_one(self.pool())
            .await?;
        Ok(count == 0)
    }

    pub async fn create_first_admin(
        &self,
        username: &str,
        password_hash: &[u8],
        password_salt: &[u8],
    ) -> StoreResult<bool> {
        let mut tx = self.pool().begin().await?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM panel_admins FOR UPDATE")
            .fetch_one(&mut *tx)
            .await?;
        if count != 0 {
            tx.rollback().await?;
            return Ok(false);
        }
        sqlx::query("INSERT INTO panel_admins (username, password_hash, password_salt, created_at) VALUES (?, ?, ?, UTC_TIMESTAMP(6))")
            .bind(username).bind(password_hash).bind(password_salt).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn admin_by_username(&self, username: &str) -> StoreResult<Option<AdminRecord>> {
        let row = sqlx::query("SELECT admin_id, username, password_hash, password_salt FROM panel_admins WHERE username = ?")
            .bind(username).fetch_optional(self.pool()).await?;
        match row {
            Some(row) => Ok(Some(AdminRecord {
                id: row.try_get("admin_id")?,
                username: row.try_get("username")?,
                password_hash: row.try_get("password_hash")?,
                password_salt: row.try_get("password_salt")?,
            })),
            None => Ok(None),
        }
    }

    pub async fn admin_username(&self, admin_id: i64) -> StoreResult<Option<String>> {
        Ok(sqlx::query_scalar("SELECT username FROM panel_admins WHERE admin_id = ?")
            .bind(admin_id).fetch_optional(self.pool()).await?)
    }

    pub async fn create_session(&self, token_hash: &[u8], admin_id: i64, expires_at: DateTime<Utc>) -> StoreResult<()> {
        sqlx::query("DELETE FROM panel_sessions WHERE expires_at <= UTC_TIMESTAMP(6)").execute(self.pool()).await?;
        sqlx::query("INSERT INTO panel_sessions (token_hash, admin_id, created_at, expires_at) VALUES (?, ?, UTC_TIMESTAMP(6), ?)")
            .bind(token_hash).bind(admin_id).bind(expires_at).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn session_admin(&self, token_hash: &[u8]) -> StoreResult<Option<i64>> {
        Ok(sqlx::query_scalar("SELECT admin_id FROM panel_sessions WHERE token_hash = ? AND expires_at > UTC_TIMESTAMP(6)")
            .bind(token_hash).fetch_optional(self.pool()).await?)
    }

    pub async fn delete_session(&self, token_hash: &[u8]) -> StoreResult<()> {
        sqlx::query("DELETE FROM panel_sessions WHERE token_hash = ?").bind(token_hash).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn panel_settings(&self) -> StoreResult<PanelSettings> {
        let row = sqlx::query("SELECT public_base_url, subscription_host, firewall_auto_open, enforcement_interval_seconds, adaptive_routing_enabled, adaptive_tuning_mode, adaptive_tuning_interval_seconds, adaptive_tuning_cooldown_seconds, adaptive_tuning_max_hysteria2_mbps FROM panel_settings WHERE singleton_id=1")
            .fetch_one(self.pool()).await?;
        Ok(PanelSettings {
            public_base_url: row.try_get("public_base_url")?, subscription_host: row.try_get("subscription_host")?,
            firewall_auto_open: row.try_get("firewall_auto_open")?, enforcement_interval_seconds: row.try_get("enforcement_interval_seconds")?,
            adaptive_routing_enabled: row.try_get("adaptive_routing_enabled")?, adaptive_tuning_mode: row.try_get("adaptive_tuning_mode")?,
            adaptive_tuning_interval_seconds: row.try_get("adaptive_tuning_interval_seconds")?,
            adaptive_tuning_cooldown_seconds: row.try_get("adaptive_tuning_cooldown_seconds")?,
            adaptive_tuning_max_hysteria2_mbps: row.try_get("adaptive_tuning_max_hysteria2_mbps")?,
        })
    }

    pub async fn save_panel_settings(&self, settings: &PanelSettings) -> StoreResult<()> {
        sqlx::query("UPDATE panel_settings SET public_base_url=?, subscription_host=?, firewall_auto_open=?, enforcement_interval_seconds=?, adaptive_routing_enabled=?, adaptive_tuning_mode=?, adaptive_tuning_interval_seconds=?, adaptive_tuning_cooldown_seconds=?, adaptive_tuning_max_hysteria2_mbps=? WHERE singleton_id=1")
            .bind(&settings.public_base_url).bind(&settings.subscription_host).bind(settings.firewall_auto_open)
            .bind(settings.enforcement_interval_seconds).bind(settings.adaptive_routing_enabled).bind(&settings.adaptive_tuning_mode)
            .bind(settings.adaptive_tuning_interval_seconds).bind(settings.adaptive_tuning_cooldown_seconds)
            .bind(settings.adaptive_tuning_max_hysteria2_mbps).execute(self.pool()).await?;
        Ok(())
    }
}
