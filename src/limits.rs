use crate::http::error::{Error, Result};
use crate::{App, time::now};
pub async fn allow(app: &App, identity: &str, bucket: &str, seconds: i64, max: i64) -> Result<()> {
    let identity = crate::crypto::hash(identity);
    let window = now() / seconds;
    let n: Option<i64> = sqlx::query_scalar(
        "INSERT INTO rate_limits(identity,bucket,window,count,expires_at)
         SELECT ?,?,?,1,?
         WHERE EXISTS(SELECT 1 FROM rate_limits WHERE identity=? AND bucket=? AND window=?)
            OR (SELECT COUNT(*) FROM rate_limits WHERE expires_at>?)<?
         ON CONFLICT(identity,bucket,window) DO UPDATE SET count=count+1 RETURNING count",
    )
    .bind(&identity)
    .bind(bucket)
    .bind(window)
    .bind((window + 1) * seconds)
    .bind(&identity)
    .bind(bucket)
    .bind(window)
    .bind(now())
    .bind(app.config.limits.rate_limit_entries)
    .fetch_optional(&app.db)
    .await?;
    let Some(n) = n else {
        return Err(Error(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
        ));
    };
    if n > max {
        return Err(Error(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
        ));
    }
    Ok(())
}
