//! backend/app/observability/request_id.py: 요청마다 uuid4를 만들고 X-Request-ID로 돌려준다.
//! AppError가 실린 응답은 여기서 JSON 본문으로 바꾼다(request_id를 넣어야 해서).

use std::collections::BTreeMap;
use std::time::Instant;

use axum::Json;
use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::error::AppError;

#[derive(Clone, Debug)]
pub struct RequestId(pub String);

#[derive(Clone, Copy, Debug)]
pub struct StartedAt(pub Instant);

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: &'a str,
    request_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    fields: Option<&'a BTreeMap<String, String>>,
}

pub async fn request_id(mut request: Request, next: Next) -> Response {
    let id = uuid::Uuid::new_v4().to_string();
    request.extensions_mut().insert(RequestId(id.clone()));
    request.extensions_mut().insert(StartedAt(Instant::now()));
    let mut response = next.run(request).await;
    if let Some(error) = response.extensions_mut().remove::<AppError>() {
        let body = ErrorBody {
            code: error.code,
            message: error.message,
            request_id: &id,
            fields: error.fields.as_ref(),
        };
        let mut rendered = (error.status, Json(body)).into_response();
        std::mem::swap(rendered.extensions_mut(), response.extensions_mut());
        response = rendered;
    }
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
}
