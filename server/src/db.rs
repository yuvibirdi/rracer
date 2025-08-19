use sqlx::{postgres::PgPoolOptions, PgPool};

/// Connect to Postgres using the provided DATABASE_URL.
pub async fn connect(url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(url)
        .await?;
    // Ensure table exists
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS passages (
            id SERIAL PRIMARY KEY,
            text TEXT UNIQUE NOT NULL,
            source_url TEXT,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        )
        "#,
    )
    .execute(&pool)
    .await?;
    Ok(pool)
}

/// Get a random passage from DB if available; otherwise fall back to static list.
#[allow(dead_code)]
pub async fn get_random_passage(db: Option<&PgPool>) -> String {
    if let Some(pool) = db {
        match sqlx::query_scalar::<_, String>(
            "SELECT text FROM passages ORDER BY random() LIMIT 1",
        )
        .fetch_one(pool)
        .await {
            Ok(row) => {
                tracing::info!("passage_source = db");
                return row;
            }
            Err(e) => {
                tracing::warn!("db_passage_fetch_failed = {:?}", e);
            }
        }
    } else {
        tracing::warn!("db_unavailable_for_passage = true");
    }
    // Fallback to static
    tracing::error!("passage_source = fallback_static");
    shared::passages::get_random_passage().to_string()
}
