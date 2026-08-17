use crate::sqlx;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::{ActivationClass, ActivationState, Database, MutationResult, StoreError, StoreResult};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RoutingDnsRecord {
    pub domain_strategy: String,
    pub dns_servers: Vec<String>,
    pub rules: Vec<RouteWrite>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RoutingDnsWrite {
    pub domain_strategy: String,
    pub dns_servers: Vec<String>,
    pub rules: Vec<RouteWrite>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RouteWrite {
    pub rule_type: String,
    pub port: Option<String>,
    pub outbound_tag: String,
    pub domains: Vec<String>,
    pub ips: Vec<String>,
    pub inbound_tags: Vec<String>,
    pub protocols: Vec<String>,
    pub users: Vec<String>,
}

impl Database {
    pub async fn routing_dns(&self, revision: i64) -> StoreResult<RoutingDnsRecord> {
        let domain_strategy = sqlx::query_scalar::<_, Option<String>>(
            "SELECT domain_strategy FROM routing_config WHERE revision_id=? AND enabled=TRUE",
        )
        .bind(revision)
        .fetch_optional(self.pool())
        .await?
        .flatten()
        .unwrap_or_else(|| "AsIs".into());
        let dns_servers = sqlx::query_scalar(
            "SELECT address FROM dns_servers WHERE revision_id=? ORDER BY position",
        )
        .bind(revision)
        .fetch_all(self.pool())
        .await?;
        let rows = sqlx::query("SELECT r.rule_id,r.rule_type,r.port_expression,o.tag outbound_tag FROM routing_rules r JOIN outbounds o ON o.revision_id=r.revision_id AND o.outbound_id=r.outbound_id WHERE r.revision_id=? ORDER BY r.position")
            .bind(revision).fetch_all(self.pool()).await?;
        let mut rules = Vec::with_capacity(rows.len());
        for row in rows {
            let rule_id: i64 = row.try_get("rule_id")?;
            let values = sqlx::query("SELECT value_kind,value_text FROM routing_rule_values WHERE revision_id=? AND rule_id=? ORDER BY value_kind,position")
                .bind(revision).bind(rule_id).fetch_all(self.pool()).await?;
            let mut rule = RouteWrite {
                rule_type: row.try_get("rule_type")?,
                port: row.try_get("port_expression")?,
                outbound_tag: row.try_get("outbound_tag")?,
                ..Default::default()
            };
            for value in values {
                let text = value.try_get("value_text")?;
                match value.try_get::<String, _>("value_kind")?.as_str() {
                    "domain" => rule.domains.push(text),
                    "ip" => rule.ips.push(text),
                    "inbound_tag" => rule.inbound_tags.push(text),
                    "protocol" => rule.protocols.push(text),
                    "user" => rule.users.push(text),
                    _ => {}
                }
            }
            rules.push(rule);
        }
        Ok(RoutingDnsRecord {
            domain_strategy,
            dns_servers,
            rules,
        })
    }

    pub async fn save_routing_dns(
        &self,
        actor: &str,
        expected_revision: i64,
        input: RoutingDnsWrite,
    ) -> StoreResult<MutationResult> {
        for server in &input.dns_servers {
            if server.trim().is_empty() {
                return Err(StoreError::InvalidConfiguration(
                    "DNS servers must not be empty".into(),
                ));
            }
        }
        let state = self.state().await?;
        let class = ActivationClass::HotSwap;
        let (mut tx, revision) = self
            .fork_revision(expected_revision, actor, "Save routing and DNS", class)
            .await?;
        sqlx::query("DELETE FROM routing_config WHERE revision_id=?")
            .bind(revision)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM dns_config WHERE revision_id=?")
            .bind(revision)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO routing_config (revision_id,enabled,domain_strategy,geoip_file,geosite_file) VALUES (?,TRUE,?,NULL,NULL)")
            .bind(revision).bind(input.domain_strategy).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO dns_config (revision_id,enabled,fake_ip_enabled,fake_ip_pool) VALUES (?,TRUE,FALSE,NULL)")
            .bind(revision).execute(&mut *tx).await?;
        for (position, server) in input.dns_servers.iter().enumerate() {
            sqlx::query("INSERT INTO dns_servers (revision_id,position,address) VALUES (?,?,?)")
                .bind(revision)
                .bind(position as u32)
                .bind(server.trim())
                .execute(&mut *tx)
                .await?;
        }
        for (position, rule) in input.rules.iter().enumerate() {
            if rule.outbound_tag.trim().is_empty() {
                return Err(StoreError::InvalidConfiguration(
                    "every route requires an outbound".into(),
                ));
            }
            let outbound_id = sqlx::query_scalar::<_, i64>(
                "SELECT outbound_id FROM outbounds WHERE revision_id=? AND tag=?",
            )
            .bind(revision)
            .bind(&rule.outbound_tag)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| {
                StoreError::InvalidConfiguration(format!(
                    "unknown outbound '{}'",
                    rule.outbound_tag
                ))
            })?;
            let rule_id = position as i64 + 1;
            sqlx::query("INSERT INTO routing_rules (revision_id,rule_id,position,rule_type,port_expression,outbound_id) VALUES (?,?,?,?,?,?)")
                .bind(revision).bind(rule_id).bind(position as u32).bind(if rule.rule_type.is_empty() { "field" } else { &rule.rule_type }).bind(&rule.port).bind(outbound_id).execute(&mut *tx).await?;
            for (kind, values) in [
                ("domain", &rule.domains),
                ("ip", &rule.ips),
                ("inbound_tag", &rule.inbound_tags),
                ("protocol", &rule.protocols),
                ("user", &rule.users),
            ] {
                for (value_position, value) in values
                    .iter()
                    .filter(|value| !value.trim().is_empty())
                    .enumerate()
                {
                    sqlx::query("INSERT INTO routing_rule_values (revision_id,rule_id,value_kind,position,value_text) VALUES (?,?,?,?,?)")
                        .bind(revision).bind(rule_id).bind(kind).bind(value_position as u32).bind(value.trim()).execute(&mut *tx).await?;
                }
            }
        }
        Database::publish_revision(&mut tx, revision, class).await?;
        tx.commit().await?;
        Ok(MutationResult {
            revision,
            parent_revision: state.desired_revision,
            active_revision: state.active_revision,
            state: ActivationState::Activating,
            activation_class: class,
            message: "Routing and DNS revision saved".into(),
        })
    }
}
