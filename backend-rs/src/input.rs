//! Python의 validation_error 응답과 같은 입력 검증.
use crate::{AppState, error::AppError};
use axum::{
    Json,
    extract::{FromRequest, Request},
};
use serde_json::Value;
use std::collections::BTreeMap;

pub struct Body(pub Value);
impl FromRequest<AppState> for Body {
    type Rejection = AppError;
    async fn from_request(request: Request, state: &AppState) -> Result<Self, AppError> {
        let Json(value) = Json::<Value>::from_request(request, state)
            .await
            .map_err(|_| invalid("body"))?;
        if !value.is_object() {
            return Err(invalid("body"));
        }
        Ok(Self(value))
    }
}

pub fn invalid(field: &str) -> AppError {
    let mut error = AppError::new(422, "validation_error", "입력값을 확인해 주세요");
    error.fields = Some(BTreeMap::from([(
        field.to_owned(),
        "형식이 올바르지 않습니다".to_owned(),
    )]));
    error
}

#[derive(Default)]
pub struct Validation(BTreeMap<String, String>);
impl Validation {
    pub fn take<T: Default>(&mut self, result: Result<T, AppError>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => {
                self.0.extend(error.fields.unwrap_or_default());
                T::default()
            }
        }
    }
    pub fn check(&mut self, field: &str, valid: bool) {
        if !valid {
            self.0
                .insert(field.to_owned(), "형식이 올바르지 않습니다".to_owned());
        }
    }
    pub fn finish(self) -> Result<(), AppError> {
        if self.0.is_empty() {
            return Ok(());
        }
        let mut error = AppError::new(422, "validation_error", "입력값을 확인해 주세요");
        error.fields = Some(self.0);
        Err(error)
    }
}

// 가입 프로필은 Python의 일반 int/bool 필드처럼 변환을 허용한다. 금액은 StrictInt이다.
pub fn integer(value: &Value, field: &str) -> Result<i64, AppError> {
    let value = value.get(field).ok_or_else(|| invalid(field))?;
    let result = match value {
        Value::Bool(b) => Some(i64::from(*b)),
        Value::String(s) => s.trim().parse::<i64>().ok().or_else(|| {
            s.trim()
                .parse::<f64>()
                .ok()
                .filter(|f| {
                    f.is_finite()
                        && f.fract() == 0.0
                        && *f >= i64::MIN as f64
                        && *f < i64::MAX as f64
                })
                .map(|f| f as i64)
        }),
        Value::Number(n) => n.as_i64().or_else(|| {
            n.as_f64()
                .filter(|f| {
                    f.is_finite()
                        && f.fract() == 0.0
                        && *f >= i64::MIN as f64
                        && *f < i64::MAX as f64
                })
                .map(|f| f as i64)
        }),
        _ => None,
    };
    result.ok_or_else(|| invalid(field))
}
pub fn boolean(value: &Value, field: &str) -> Result<bool, AppError> {
    let result = match value.get(field) {
        Some(Value::Bool(b)) => Some(*b),
        Some(Value::Number(n)) if n.as_f64() == Some(0.0) => Some(false),
        Some(Value::Number(n)) if n.as_f64() == Some(1.0) => Some(true),
        Some(Value::String(s)) => match s.to_lowercase().as_str() {
            "0" | "off" | "f" | "false" | "n" | "no" => Some(false),
            "1" | "on" | "t" | "true" | "y" | "yes" => Some(true),
            _ => None,
        },
        _ => None,
    };
    result.ok_or_else(|| invalid(field))
}

pub fn string(value: &Value, field: &str, min: usize, max: usize) -> Result<String, AppError> {
    let s = value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(field))?;
    if !(min..=max).contains(&s.chars().count()) {
        return Err(invalid(field));
    }
    Ok(s.to_owned())
}

pub fn amount(value: &Value) -> Result<i64, AppError> {
    value
        .get("amount")
        .and_then(|v| v.as_i64().or_else(|| v.as_u64().map(|_| i64::MAX)))
        .ok_or_else(|| invalid("amount"))
}

pub fn check_amount(amount: i64) -> Result<(), AppError> {
    if amount <= 0 {
        return Err(AppError::new(
            422,
            "invalid_amount",
            "금액은 1원 이상이어야 합니다",
        ));
    }
    if amount > 10_000_000 {
        return Err(AppError::new(
            422,
            "amount_limit_exceeded",
            "1회 최대 10,000,000원까지 가능합니다",
        ));
    }
    Ok(())
}
