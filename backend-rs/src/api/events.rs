//! 이벤트는 금융 거래 커밋 후 같은 요청에서 기록한다. 실패해도 금융 결과를 바꾸지 않는다.
use crate::{
    AppState,
    error::AppError,
    request_id::{RequestId, StartedAt},
};
use axum::{extract::FromRequestParts, http::request::Parts};
use chrono::{FixedOffset, NaiveDateTime, TimeZone, Utc};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;

#[derive(Clone)]
pub struct Context {
    pub id: String,
    pub started: std::time::Instant,
}
impl FromRequestParts<AppState> for Context {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, _: &AppState) -> Result<Self, AppError> {
        Ok(Self {
            id: parts
                .extensions
                .get::<RequestId>()
                .expect("request id middleware")
                .0
                .clone(),
            started: parts
                .extensions
                .get::<StartedAt>()
                .expect("request timer middleware")
                .0,
        })
    }
}

pub fn now() -> NaiveDateTime {
    Utc::now()
        .with_timezone(&FixedOffset::east_opt(9 * 3600).unwrap())
        .naive_local()
}
pub fn iso(time: NaiveDateTime) -> String {
    FixedOffset::east_opt(9 * 3600)
        .unwrap()
        .from_local_datetime(&time)
        .single()
        .unwrap()
        .to_rfc3339()
}
pub fn band(amount: i64) -> &'static str {
    match amount {
        ..10_000 => "under_10k",
        10_000..100_000 => "under_100k",
        100_000..1_000_000 => "under_1m",
        _ => "over_1m",
    }
}

pub async fn write_line(path: &std::path::Path, value: &Value) -> std::io::Result<()> {
    // 한 줄 전체를 잠근 상태에서 써서 동시 요청의 JSON이 섞이지 않게 한다.
    static LOG_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = LOG_LOCK.lock().await;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');
    file.write_all(&line).await
}

pub async fn warning(state: &AppState, message: &'static str) {
    let _ = write_line(&state.settings.app_log_path, &json!({"timestamp": iso(now()), "level":"WARNING", "message": message, "instance": state.settings.instance_name})).await;
}

pub async fn publish(
    state: &AppState,
    ctx: &Context,
    kind: &str,
    outcome: &str,
    reason: &str,
    properties: Value,
) {
    let value = json!({"event_id": uuid::Uuid::new_v4().to_string(), "event_type":kind,
        "occurred_at": iso(now()), "source":"api", "outcome":outcome, "reason":reason,
        "duration_ms":ctx.started.elapsed().as_millis() as u64, "request_id":ctx.id,
        "properties":properties, "instance":state.settings.instance_name});
    if write_line(&state.settings.event_log_path, &value)
        .await
        .is_err()
    {
        warning(state, "event_log_write_failed").await;
    }
    let document = match mongodb::bson::to_document(&value) {
        Ok(d) => d,
        Err(_) => {
            warning(state, "mongodb_write_failed").await;
            return;
        }
    };
    if !matches!(
        tokio::time::timeout(
            crate::db::MONGO_SOCKET_TIMEOUT,
            state
                .mongo
                .collection::<mongodb::bson::Document>("events_bronze")
                .insert_one(document)
                .into_future()
        )
        .await,
        Ok(Ok(_))
    ) {
        warning(state, "mongodb_write_failed").await;
    }
}

use std::future::IntoFuture;

pub async fn ensure_indexes(state: &AppState) {
    use mongodb::{IndexModel, bson::doc, options::IndexOptions};
    for name in ["events_bronze", "card_payment_locations"] {
        let index = IndexModel::builder()
            .keys(doc! {"event_id":1})
            .options(IndexOptions::builder().unique(true).build())
            .build();
        if !matches!(
            tokio::time::timeout(
                crate::db::MONGO_SOCKET_TIMEOUT,
                state
                    .mongo
                    .collection::<mongodb::bson::Document>(name)
                    .create_index(index)
                    .into_future()
            )
            .await,
            Ok(Ok(_))
        ) {
            warning(state, "mongodb_index_setup_skipped").await;
        }
    }
}

pub async fn application_log(
    axum::extract::State(state): axum::extract::State<AppState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let id = request
        .extensions()
        .get::<RequestId>()
        .map(|id| id.0.clone());
    let started = std::time::Instant::now();
    let method = request.method().to_string();
    let route = request
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".to_owned());
    let response = next.run(request).await;
    let status = response.status().as_u16();
    let error = response.extensions().get::<AppError>().map(|e| e.code);
    let level = if status >= 500 {
        "ERROR"
    } else if status >= 400 {
        "WARNING"
    } else {
        "INFO"
    };
    if write_line(
        &state.settings.app_log_path,
        &json!({"timestamp":iso(now()),"level":level,"message":"request_completed",
        "instance":state.settings.instance_name,"method":method,"route":route,"status_code":status,
        "duration_ms":started.elapsed().as_millis() as u64,"request_id":id,"error_code":error}),
    )
    .await
    .is_err()
    {
        eprintln!("application_log_write_failed");
    }
    response
}
