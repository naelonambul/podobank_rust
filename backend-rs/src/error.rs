//! backend/app/core/errors.py와 같은 오류 응답: {"code", "message", "request_id", "fields"?} + X-Request-ID.
//! request_id는 미들웨어가 채우므로 여기서는 응답 확장(extension)에 오류 내용만 실어 보낸다.

use std::collections::BTreeMap;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Clone, Debug)]
pub struct AppError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: &'static str,
    pub fields: Option<BTreeMap<String, String>>,
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        if matches!(crate::db::error_number(&error), Some(1205 | 1213)) {
            return Self::new(
                409,
                "lock_timeout",
                "다른 거래를 처리하고 있습니다. 잠시 후 다시 시도해 주세요",
            );
        }
        if matches!(error, sqlx::Error::PoolTimedOut) {
            Self::new(
                503,
                "database_unavailable",
                "요청이 많아 잠시 처리할 수 없습니다",
            )
        } else {
            Self::new(
                503,
                "database_unavailable",
                "데이터베이스에 연결할 수 없습니다",
            )
        }
    }
}

impl AppError {
    pub fn new(status: u16, code: &'static str, message: &'static str) -> Self {
        AppError {
            status: StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            code,
            message,
            fields: None,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let mut response = self.status.into_response();
        response.extensions_mut().insert(self);
        response
    }
}

pub async fn not_found() -> AppError {
    AppError::new(404, "not_found", "요청한 주소를 찾을 수 없습니다")
}
