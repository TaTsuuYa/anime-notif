//! Versioned schema migrations, tracked in a `_migrations` table and applied
//! in order by [`crate::Store::open`].

use libsql::Connection;

use crate::error::StoreError;

/// One migration: a monotonically increasing version and the SQL script to
/// bring the schema from `version - 1` to `version`.
struct Migration {
    version: i64,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    sql: include_str!("../migrations/0001_init.sql"),
}];

pub(crate) async fn run(conn: &Connection) -> Result<(), StoreError> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL)",
        (),
    )
    .await?;

    let mut rows = conn.query("SELECT version FROM _migrations", ()).await?;
    let mut applied = std::collections::HashSet::new();
    while let Some(row) = rows.next().await? {
        let version: i64 = row.get(0)?;
        applied.insert(version);
    }

    for migration in MIGRATIONS {
        if applied.contains(&migration.version) {
            continue;
        }
        conn.execute_batch(migration.sql).await?;
        conn.execute(
            "INSERT INTO _migrations (version, applied_at) VALUES (?1, ?2)",
            libsql::params![migration.version, chrono::Utc::now().to_rfc3339()],
        )
        .await?;
        tracing::info!(version = migration.version, "applied migration");
    }

    Ok(())
}
