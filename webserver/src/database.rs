use std::str::FromStr;

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

pub async fn init_db(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let connect_opts = SqliteConnectOptions::from_str(database_url)?.create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_opts)
        .await?;
    // 创建 tasks 表
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT,
            status TEXT NOT NULL,
            error_message TEXT,
            upload_time DATETIME NOT NULL,
            start_time DATETIME,
            end_time DATETIME
        )
        "#,
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}
