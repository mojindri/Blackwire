use sqlx::Row;

use crate::{Database, StoreResult};

#[derive(Debug, Clone)]
pub struct UserTrafficRecord {
    pub email: String,
    pub upload_bytes: u64,
    pub download_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct InboundTrafficRecord {
    pub tag: String,
    pub upload_bytes: u64,
    pub download_bytes: u64,
}

impl Database {
    pub async fn heartbeat(&self, instance_id: &str, active_revision: i64) -> StoreResult<()> {
        sqlx::query("INSERT INTO runtime_instances (instance_id,active_revision,state,last_error,heartbeat_at) VALUES (?,?,'running',NULL,UTC_TIMESTAMP(6)) ON DUPLICATE KEY UPDATE active_revision=VALUES(active_revision),state='running',last_error=NULL,heartbeat_at=UTC_TIMESTAMP(6)")
            .bind(instance_id).bind(active_revision).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn runtime_healthy(&self) -> StoreResult<bool> {
        Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runtime_instances WHERE state='running' AND heartbeat_at >= UTC_TIMESTAMP(6) - INTERVAL 15 SECOND)")
            .fetch_one(self.pool()).await?)
    }

    pub async fn persist_runtime_counters(
        &self,
        users: &[(String, u64, u64)],
        inbounds: &[(String, u64, u64)],
    ) -> StoreResult<()> {
        let Some(revision) = self.state().await?.active_revision else {
            return Ok(());
        };
        let mut tx = self.pool().begin().await?;
        for (email, upload, download) in users {
            if let Some(user_id) = sqlx::query_scalar::<_, i64>(
                "SELECT user_id FROM users WHERE revision_id=? AND email=?",
            )
            .bind(revision)
            .bind(email)
            .fetch_optional(&mut *tx)
            .await?
            {
                sqlx::query("INSERT INTO user_traffic (user_id,upload_bytes,download_bytes,last_runtime_upload_bytes,last_runtime_download_bytes,updated_at) VALUES (?,?,?,?,?,UTC_TIMESTAMP(6)) ON DUPLICATE KEY UPDATE upload_bytes=upload_bytes+IF(VALUES(last_runtime_upload_bytes)>=last_runtime_upload_bytes,VALUES(last_runtime_upload_bytes)-last_runtime_upload_bytes,VALUES(last_runtime_upload_bytes)),download_bytes=download_bytes+IF(VALUES(last_runtime_download_bytes)>=last_runtime_download_bytes,VALUES(last_runtime_download_bytes)-last_runtime_download_bytes,VALUES(last_runtime_download_bytes)),last_runtime_upload_bytes=VALUES(last_runtime_upload_bytes),last_runtime_download_bytes=VALUES(last_runtime_download_bytes),updated_at=UTC_TIMESTAMP(6)")
                    .bind(user_id).bind(upload).bind(download).bind(upload).bind(download).execute(&mut *tx).await?;
            }
        }
        for (tag, upload, download) in inbounds {
            if let Some(inbound_id) = sqlx::query_scalar::<_, i64>(
                "SELECT inbound_id FROM inbounds WHERE revision_id=? AND tag=?",
            )
            .bind(revision)
            .bind(tag)
            .fetch_optional(&mut *tx)
            .await?
            {
                sqlx::query("INSERT INTO inbound_traffic (inbound_id,upload_bytes,download_bytes,last_runtime_upload_bytes,last_runtime_download_bytes,updated_at) VALUES (?,?,?,?,?,UTC_TIMESTAMP(6)) ON DUPLICATE KEY UPDATE upload_bytes=upload_bytes+IF(VALUES(last_runtime_upload_bytes)>=last_runtime_upload_bytes,VALUES(last_runtime_upload_bytes)-last_runtime_upload_bytes,VALUES(last_runtime_upload_bytes)),download_bytes=download_bytes+IF(VALUES(last_runtime_download_bytes)>=last_runtime_download_bytes,VALUES(last_runtime_download_bytes)-last_runtime_download_bytes,VALUES(last_runtime_download_bytes)),last_runtime_upload_bytes=VALUES(last_runtime_upload_bytes),last_runtime_download_bytes=VALUES(last_runtime_download_bytes),updated_at=UTC_TIMESTAMP(6)")
                    .bind(inbound_id).bind(upload).bind(download).bind(upload).bind(download).execute(&mut *tx).await?;
            }
        }
        sqlx::query("INSERT INTO enforcement_state (user_id,status,reason,evaluated_at) SELECT u.user_id,CASE WHEN NOT u.enabled THEN 'disabled' WHEN u.expiry_at IS NOT NULL AND u.expiry_at<=UTC_TIMESTAMP(6) THEN 'expired' WHEN u.traffic_limit_bytes IS NOT NULL AND COALESCE(t.upload_bytes,0)+COALESCE(t.download_bytes,0)>=u.traffic_limit_bytes THEN 'traffic_limited' ELSE 'current' END,CASE WHEN NOT u.enabled THEN 'user disabled' WHEN u.expiry_at IS NOT NULL AND u.expiry_at<=UTC_TIMESTAMP(6) THEN 'subscription expired' WHEN u.traffic_limit_bytes IS NOT NULL AND COALESCE(t.upload_bytes,0)+COALESCE(t.download_bytes,0)>=u.traffic_limit_bytes THEN 'traffic limit reached' ELSE NULL END,UTC_TIMESTAMP(6) FROM users u LEFT JOIN user_traffic t ON t.user_id=u.user_id WHERE u.revision_id=? ON DUPLICATE KEY UPDATE status=VALUES(status),reason=VALUES(reason),evaluated_at=VALUES(evaluated_at)")
            .bind(revision).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn traffic_snapshot(
        &self,
    ) -> StoreResult<(Vec<UserTrafficRecord>, Vec<InboundTrafficRecord>)> {
        let revision = self.state().await?.desired_revision;
        let users = sqlx::query("SELECT u.email,COALESCE(t.upload_bytes,0) upload_bytes,COALESCE(t.download_bytes,0) download_bytes FROM users u LEFT JOIN user_traffic t ON t.user_id=u.user_id WHERE u.revision_id=? ORDER BY u.email")
            .bind(revision).fetch_all(self.pool()).await?.into_iter().map(|row| Ok(UserTrafficRecord { email: row.try_get("email")?, upload_bytes: row.try_get("upload_bytes")?, download_bytes: row.try_get("download_bytes")? })).collect::<Result<_, sqlx::Error>>()?;
        let inbounds = sqlx::query("SELECT i.tag,COALESCE(t.upload_bytes,0) upload_bytes,COALESCE(t.download_bytes,0) download_bytes FROM inbounds i LEFT JOIN inbound_traffic t ON t.inbound_id=i.inbound_id WHERE i.revision_id=? ORDER BY i.position")
            .bind(revision).fetch_all(self.pool()).await?.into_iter().map(|row| Ok(InboundTrafficRecord { tag: row.try_get("tag")?, upload_bytes: row.try_get("upload_bytes")?, download_bytes: row.try_get("download_bytes")? })).collect::<Result<_, sqlx::Error>>()?;
        Ok((users, inbounds))
    }

    pub async fn prune_revision_history(&self) -> StoreResult<()> {
        sqlx::query("DELETE FROM configuration_revisions WHERE revision NOT IN (SELECT revision FROM (SELECT revision FROM configuration_revisions ORDER BY revision DESC LIMIT 20) recent) AND revision NOT IN (SELECT desired_revision FROM configuration_state UNION SELECT active_revision FROM configuration_state WHERE active_revision IS NOT NULL UNION SELECT pending_maintenance_revision FROM configuration_state WHERE pending_maintenance_revision IS NOT NULL) AND revision NOT IN (SELECT active_revision FROM runtime_instances WHERE active_revision IS NOT NULL)")
            .execute(self.pool()).await?;
        Ok(())
    }
}
