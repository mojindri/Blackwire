use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActivationClass {
    HotSwap,
    ListenerHandover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActivationState {
    Active,
    Activating,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    pub revision: i64,
    pub parent_revision: Option<i64>,
    pub actor: String,
    pub summary: String,
    pub activation_class: ActivationClass,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationState {
    pub desired_revision: i64,
    pub active_revision: Option<i64>,
    pub activation_state: ActivationState,
    pub last_error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationResult {
    pub revision: i64,
    pub parent_revision: i64,
    pub active_revision: Option<i64>,
    pub state: ActivationState,
    pub activation_class: ActivationClass,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionSummary {
    pub revision: i64,
    pub parent_revision: Option<i64>,
    pub actor: String,
    pub summary: String,
    pub activation_class: ActivationClass,
    pub created_at: DateTime<Utc>,
}
