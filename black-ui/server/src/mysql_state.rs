use anyhow::Result;
use blackwire_store::Database;

#[derive(Clone)]
pub struct AppState {
    pub store: Database,
}

impl AppState {
    pub async fn open() -> Result<Self> {
        let store = Database::connect_from_env().await?;
        store.verify_schema().await?;
        Ok(Self { store })
    }
}
