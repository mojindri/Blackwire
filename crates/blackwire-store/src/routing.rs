use crate::sqlx;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::{ActivationClass, ActivationState, Database, MutationResult, StoreError, StoreResult};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RoutingDnsRecord {
    pub domain_strategy: String,
    pub geoip_file: Option<String>,
    pub geosite_file: Option<String>,
    pub dns_servers: Vec<String>,
    pub fake_ip_enabled: bool,
    pub fake_ip_pool: String,
    pub rules: Vec<RouteWrite>,
    pub balancers: Vec<BalancerWrite>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RoutingDnsWrite {
    pub domain_strategy: String,
    pub geoip_file: Option<String>,
    pub geosite_file: Option<String>,
    pub dns_servers: Vec<String>,
    pub fake_ip_enabled: bool,
    pub fake_ip_pool: String,
    pub rules: Vec<RouteWrite>,
    pub balancers: Vec<BalancerWrite>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BalancerWrite {
    pub tag: String,
    pub strategy: String,
    pub members: Vec<BalancerMemberWrite>,
    pub adaptive: Option<AdaptiveBalancerWrite>,
    pub health_check: Option<HealthCheckWrite>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BalancerMemberWrite {
    pub outbound_tag: String,
    pub profile_name: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdaptiveBalancerWrite {
    pub failure_threshold: u32,
    pub cooldown_secs: u64,
    pub ewma_alpha: f64,
    pub switch_margin: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheckWrite {
    pub url: String,
    pub interval_secs: u64,
    pub timeout_secs: u64,
    pub max_failures: u32,
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
        let routing = sqlx::query(
            "SELECT domain_strategy,geoip_file,geosite_file FROM routing_config WHERE revision_id=? AND enabled=TRUE",
        )
        .bind(revision)
        .fetch_optional(self.pool())
        .await?;
        let domain_strategy = routing
            .as_ref()
            .and_then(|row| {
                row.try_get::<Option<String>, _>("domain_strategy")
                    .ok()
                    .flatten()
            })
            .unwrap_or_else(|| "AsIs".into());
        let geoip_file = routing
            .as_ref()
            .and_then(|row| row.try_get("geoip_file").ok())
            .flatten();
        let geosite_file = routing
            .as_ref()
            .and_then(|row| row.try_get("geosite_file").ok())
            .flatten();
        let dns = sqlx::query("SELECT fake_ip_enabled,fake_ip_pool FROM dns_config WHERE revision_id=? AND enabled=TRUE")
            .bind(revision).fetch_optional(self.pool()).await?;
        let fake_ip_enabled = dns
            .as_ref()
            .and_then(|row| row.try_get("fake_ip_enabled").ok())
            .unwrap_or(false);
        let fake_ip_pool = dns
            .as_ref()
            .and_then(|row| {
                row.try_get::<Option<String>, _>("fake_ip_pool")
                    .ok()
                    .flatten()
            })
            .unwrap_or_else(|| "198.18.0.0/15".into());
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
        let balancer_rows = sqlx::query("SELECT balancer_id,tag,strategy,failure_threshold,cooldown_seconds,ewma_alpha,switch_margin,health_url,health_interval_seconds,health_timeout_seconds,health_max_failures FROM routing_balancers WHERE revision_id=? ORDER BY position")
            .bind(revision).fetch_all(self.pool()).await?;
        let mut balancers = Vec::with_capacity(balancer_rows.len());
        for row in balancer_rows {
            let balancer_id: i64 = row.try_get("balancer_id")?;
            let member_rows = sqlx::query("SELECT o.tag outbound_tag,m.profile_name FROM routing_balancer_members m JOIN outbounds o ON o.revision_id=m.revision_id AND o.outbound_id=m.outbound_id WHERE m.revision_id=? AND m.balancer_id=? ORDER BY m.position")
                .bind(revision).bind(balancer_id).fetch_all(self.pool()).await?;
            let members = member_rows
                .into_iter()
                .map(|member| {
                    Ok(BalancerMemberWrite {
                        outbound_tag: member.try_get("outbound_tag")?,
                        profile_name: member.try_get("profile_name")?,
                    })
                })
                .collect::<StoreResult<Vec<_>>>()?;
            let adaptive =
                row.try_get::<Option<u32>, _>("failure_threshold")?
                    .map(|failure_threshold| AdaptiveBalancerWrite {
                        failure_threshold,
                        cooldown_secs: row
                            .try_get::<Option<u64>, _>("cooldown_seconds")
                            .unwrap_or(None)
                            .unwrap_or(30),
                        ewma_alpha: row
                            .try_get::<Option<f64>, _>("ewma_alpha")
                            .unwrap_or(None)
                            .unwrap_or(0.2),
                        switch_margin: row
                            .try_get::<Option<f64>, _>("switch_margin")
                            .unwrap_or(None)
                            .unwrap_or(0.15),
                    });
            let health_check =
                row.try_get::<Option<String>, _>("health_url")?
                    .map(|url| HealthCheckWrite {
                        url,
                        interval_secs: row
                            .try_get::<Option<u64>, _>("health_interval_seconds")
                            .unwrap_or(None)
                            .unwrap_or(30),
                        timeout_secs: row
                            .try_get::<Option<u64>, _>("health_timeout_seconds")
                            .unwrap_or(None)
                            .unwrap_or(5),
                        max_failures: row
                            .try_get::<Option<u32>, _>("health_max_failures")
                            .unwrap_or(None)
                            .unwrap_or(3),
                    });
            balancers.push(BalancerWrite {
                tag: row.try_get("tag")?,
                strategy: row.try_get("strategy")?,
                members,
                adaptive,
                health_check,
            });
        }
        Ok(RoutingDnsRecord {
            domain_strategy,
            geoip_file,
            geosite_file,
            dns_servers,
            fake_ip_enabled,
            fake_ip_pool,
            rules,
            balancers,
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
        if input.fake_ip_enabled && input.fake_ip_pool.trim().is_empty() {
            return Err(StoreError::InvalidConfiguration(
                "FakeIP pool must not be empty when FakeIP is enabled".into(),
            ));
        }
        let state = self.state().await?;
        let class = ActivationClass::HotSwap;
        let (mut tx, revision) = self
            .fork_revision(expected_revision, actor, "Save routing and DNS", class)
            .await?;
        // Only replace the fields represented by this write model. Removing
        // routing_config would cascade into balancers and would also discard
        // GeoIP/GeoSite paths; replacing dns_config would silently disable
        // FakeIP. Both are core settings that a basic UI edit must preserve.
        sqlx::query("INSERT INTO routing_config (revision_id,enabled,domain_strategy,geoip_file,geosite_file) VALUES (?,TRUE,?,?,?) ON DUPLICATE KEY UPDATE enabled=TRUE,domain_strategy=VALUES(domain_strategy),geoip_file=VALUES(geoip_file),geosite_file=VALUES(geosite_file)")
            .bind(revision).bind(input.domain_strategy).bind(trimmed_option(input.geoip_file)).bind(trimmed_option(input.geosite_file)).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO dns_config (revision_id,enabled,fake_ip_enabled,fake_ip_pool) VALUES (?,TRUE,?,?) ON DUPLICATE KEY UPDATE enabled=TRUE,fake_ip_enabled=VALUES(fake_ip_enabled),fake_ip_pool=VALUES(fake_ip_pool)")
            .bind(revision).bind(input.fake_ip_enabled).bind(input.fake_ip_enabled.then(|| input.fake_ip_pool.trim().to_owned())).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM routing_rules WHERE revision_id=?")
            .bind(revision)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM dns_servers WHERE revision_id=?")
            .bind(revision)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM routing_balancers WHERE revision_id=?")
            .bind(revision)
            .execute(&mut *tx)
            .await?;
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
        for (position, balancer) in input.balancers.iter().enumerate() {
            if balancer.tag.trim().is_empty() || balancer.members.is_empty() {
                return Err(StoreError::InvalidConfiguration(
                    "every balancer needs a tag and at least one outbound".into(),
                ));
            }
            let balancer_id = position as i64 + 1;
            let adaptive = balancer.adaptive.as_ref();
            let health = balancer.health_check.as_ref();
            sqlx::query("INSERT INTO routing_balancers (revision_id,balancer_id,tag,strategy,position,failure_threshold,cooldown_seconds,ewma_alpha,switch_margin,health_url,health_interval_seconds,health_timeout_seconds,health_max_failures) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(revision).bind(balancer_id).bind(balancer.tag.trim()).bind(if balancer.strategy.is_empty() { "latency" } else { balancer.strategy.as_str() }).bind(position as u32)
                .bind(adaptive.map(|value| value.failure_threshold)).bind(adaptive.map(|value| value.cooldown_secs)).bind(adaptive.map(|value| value.ewma_alpha)).bind(adaptive.map(|value| value.switch_margin))
                .bind(health.map(|value| value.url.trim())).bind(health.map(|value| value.interval_secs)).bind(health.map(|value| value.timeout_secs)).bind(health.map(|value| value.max_failures))
                .execute(&mut *tx).await?;
            for (member_position, member) in balancer.members.iter().enumerate() {
                let outbound_id = sqlx::query_scalar::<_, i64>(
                    "SELECT outbound_id FROM outbounds WHERE revision_id=? AND tag=?",
                )
                .bind(revision)
                .bind(member.outbound_tag.trim())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| {
                    StoreError::InvalidConfiguration(format!(
                        "unknown outbound '{}'",
                        member.outbound_tag
                    ))
                })?;
                sqlx::query("INSERT INTO routing_balancer_members (revision_id,balancer_id,position,outbound_id,profile_name) VALUES (?,?,?,?,?)")
                    .bind(revision).bind(balancer_id).bind(member_position as u32).bind(outbound_id).bind(trimmed_option(member.profile_name.clone())).execute(&mut *tx).await?;
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

fn trimmed_option(value: Option<String>) -> Option<String> {
    value.and_then(|value| (!value.trim().is_empty()).then(|| value.trim().to_owned()))
}
