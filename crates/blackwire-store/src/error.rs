use thiserror::Error;

use crate::sqlx;

pub type StoreResult<T> = Result<T, StoreError>;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("MySQL connection is not configured; set BLACKWIRE_DATABASE_URL or BLACKWIRE_DATABASE_URL_FILE")]
    MissingDatabaseUrl,
    #[error("database URL credential file '{path}' could not be read: {source}")]
    CredentialFile {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("database schema is not initialized; run `blackwire db init`")]
    SchemaMissing,
    #[error("database schema version {actual} is incompatible with required version {expected}")]
    SchemaVersion { expected: i64, actual: i64 },
    #[error("configuration revision conflict: expected {expected}, current desired revision is {actual}")]
    RevisionConflict { expected: i64, actual: i64 },
    #[error("invalid configuration mutation: {0}")]
    InvalidConfiguration(String),
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
}
