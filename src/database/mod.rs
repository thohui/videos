use deadpool_sqlite::{Config, Hook, HookError, Pool, Runtime};
use include_dir::{Dir, include_dir};
use rusqlite_migration::MigrationsBuilder;

pub mod channel;
pub mod video;

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("pool error: {0}")]
    Pool(#[from] deadpool_sqlite::PoolError),

    #[error("interact error: {0}")]
    Interact(String),

    #[error("query error: {0}")]
    Query(#[from] rusqlite::Error),

    #[error("migration error: {0}")]
    Migration(#[from] rusqlite_migration::Error),
}

impl DatabaseError {
    pub fn is_unique_constraint_violation(&self) -> bool {
        matches!(
            self,
            DatabaseError::Query(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::ConstraintViolation
                && matches!(
                    err.extended_code,
                    rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE | rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY
                )
        )
    }
}

impl From<deadpool_sqlite::InteractError> for DatabaseError {
    fn from(e: deadpool_sqlite::InteractError) -> Self {
        DatabaseError::Interact(e.to_string())
    }
}

#[derive(Clone)]
pub struct DatabasePool(Pool);

impl DatabasePool {
    pub async fn conn(&self) -> Result<deadpool_sqlite::Object, DatabaseError> {
        Ok(self.0.get().await?)
    }

    pub async fn interact<F, R>(&self, f: F) -> Result<R, DatabaseError>
    where
        F: FnOnce(&mut rusqlite::Connection) -> Result<R, rusqlite::Error> + Send + 'static,
        R: Send + 'static,
    {
        let conn = self.conn().await?;
        let result = conn.interact(f).await??;
        Ok(result)
    }

    pub async fn connect(db_path: &str) -> anyhow::Result<DatabasePool> {
        let pool = Config::new(db_path)
            .builder(Runtime::Tokio1)?
            .max_size(2)
            .post_create(Hook::async_fn(|conn, _| {
                Box::pin(async move {
                    conn.interact(|c| {
                        c.execute_batch(
                            r#"
                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = NORMAL;
                PRAGMA foreign_keys = ON;
                PRAGMA busy_timeout = 5000;
                PRAGMA wal_autocheckpoint = 1000;
                PRAGMA optimize=0x10002;
                "#,
                        )
                    })
                    .await
                    .map_err(|e| HookError::message(e.to_string()))?
                    .map_err(HookError::Backend)
                })
            }))
            .build()?;

        Ok(DatabasePool(pool))
    }
}
static MIGRATIONS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/migrations");

pub async fn run_migrations(connection: &DatabasePool) -> Result<(), DatabaseError> {
    let migrations = MigrationsBuilder::from_directory(&MIGRATIONS_DIR)?.finalize();
    let conn = connection.conn().await?;
    conn.interact(move |c| migrations.to_latest(c)).await??;
    Ok(())
}
