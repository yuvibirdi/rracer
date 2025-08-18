use sqlx::{postgres::PgPoolOptions, PgPool};
use tracing::{info, warn};

pub async fn connect(database_url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;
    ensure_schema(&pool).await?;
    Ok(pool)
}

async fn ensure_schema(pool: &PgPool) -> anyhow::Result<()> {
    let ddl_table = r#"
        CREATE TABLE IF NOT EXISTS passages (
            id SERIAL PRIMARY KEY,
            text TEXT NOT NULL,
            source_url TEXT,
            created_at TIMESTAMPTZ DEFAULT now()
        );
    "#;
    sqlx::query(ddl_table).execute(pool).await?;

    // Ensure a unique index on text so we can upsert and avoid duplicates
    let ddl_index = r#"
        CREATE UNIQUE INDEX IF NOT EXISTS idx_passages_text ON passages (text);
    "#;
    sqlx::query(ddl_index).execute(pool).await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn get_random_passage(pool: Option<&PgPool>) -> String {
    if let Some(pool) = pool {
        match sqlx::query_scalar::<_, String>("SELECT text FROM passages ORDER BY random() LIMIT 1")
            .fetch_optional(pool)
            .await
        {
            Ok(Some(text)) => return text,
            Ok(None) => warn!("No passages in DB; using fallback list"),
            Err(e) => warn!("DB error fetching passage: {:?}; using fallback list", e),
        }
    } else {
        info!("DB disabled; using fallback list");
    }

    shared::passages::get_random_passage().to_string()
}
