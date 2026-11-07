use super::{
    super::events::{self, Context},
    repository,
};
use crate::{AppState, error::AppError, input, security};
use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, request::Parts},
};
use chrono::{Datelike, NaiveDateTime};
use serde_json::{Value, json};
use sqlx::Row;

#[derive(Clone)]
pub struct User {
    pub id: u64,
    pub role: String,
    pub name: String,
}
pub struct Customer(pub User);
pub fn unauthorized() -> AppError {
    AppError::new(401, "authentication_required", "로그인이 필요합니다")
}
pub fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all("cookie")
        .iter()
        .filter_map(|h| h.to_str().ok())
        .flat_map(|h| h.split(';'))
        .find_map(|pair| {
            let (key, value) = pair.trim().split_once('=')?;
            (key == name).then(|| value.to_owned())
        })
}
pub fn device(headers: &HeaderMap) -> String {
    cookie(headers, "podobank_device")
        .and_then(|s| uuid::Uuid::parse_str(&s).ok())
        .unwrap_or_else(uuid::Uuid::new_v4)
        .to_string()
}
impl FromRequestParts<AppState> for User {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let token = cookie(&parts.headers, "podobank_session").ok_or_else(unauthorized)?;
        repository::resolve(state, &token).await
    }
}
impl FromRequestParts<AppState> for Customer {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let user = User::from_request_parts(parts, state).await?;
        if user.role != "customer" {
            return Err(AppError::new(403, "forbidden", "고객만 사용할 수 있습니다"));
        }
        Ok(Self(user))
    }
}

pub async fn login(
    state: &AppState,
    ctx: &Context,
    headers: &HeaderMap,
    phone: String,
    password: String,
) -> Result<(String, String), AppError> {
    let mut tx = state.db.begin().await?;
    let row = sqlx::query("SELECT id,password_hash,locked_until FROM users WHERE phone_number=?")
        .bind(phone)
        .fetch_optional(&mut *tx)
        .await?;
    let now = events::now();
    let mut error = AppError::new(
        401,
        "invalid_credentials",
        "전화번호 또는 비밀번호가 올바르지 않습니다",
    );
    if let Some(row) = row {
        let id: u64 = row.get("id");
        let locked: Option<NaiveDateTime> = row.get("locked_until");
        sqlx::query("UPDATE users SET failed_login_count=0,locked_until=NULL WHERE id=? AND locked_until IS NOT NULL AND locked_until<=?").bind(id).bind(now).execute(&mut *tx).await?;
        if locked.is_some_and(|time| time > now) {
            error = AppError::new(
                423,
                "account_locked",
                "비밀번호를 여러 번 틀려 잠시 잠겼습니다. 5분 후 다시 시도해 주세요",
            );
        } else if security::verify_password(Some(row.get("password_hash")), password).await? {
            sqlx::query("UPDATE users SET failed_login_count=0,locked_until=NULL WHERE id=?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            let device = device(headers);
            let token = repository::start_session(&mut tx, id, &device, headers).await?;
            tx.commit().await?;
            events::publish(
                state,
                ctx,
                "login",
                "success",
                "completed",
                json!({"method":"password"}),
            )
            .await;
            return Ok((token, device));
        } else {
            sqlx::query("UPDATE users SET locked_until=IF(failed_login_count+1>=5,?,locked_until),failed_login_count=failed_login_count+1 WHERE id=?")
                .bind(events::now()+chrono::Duration::minutes(5)).bind(id).execute(&mut *tx).await?;
        }
    } else {
        security::verify_password(None, password).await?;
    }
    tx.commit().await?;
    events::publish(
        state,
        ctx,
        "login",
        "failure",
        error.code,
        json!({"method":"password"}),
    )
    .await;
    Err(error)
}

pub struct Signup {
    pub name: String,
    pub phone: String,
    pub password: String,
    pub birth: i64,
    pub gender: String,
    pub marital: String,
    pub employed: bool,
}
impl Signup {
    pub fn parse(body: &Value) -> Result<Self, AppError> {
        let mut validation = input::Validation::default();
        let name = validation
            .take(input::string(body, "name", 0, 50))
            .trim()
            .to_owned();
        validation.check("name", !name.is_empty());
        let phone = validation.take(input::string(body, "phone_number", 10, 11));
        validation.check(
            "phone_number",
            phone.starts_with("01") && phone.bytes().all(|b| b.is_ascii_digit()),
        );
        let password = validation.take(input::string(body, "password", 8, 128));
        let birth = validation.take(input::integer(body, "birth_year"));
        validation.check(
            "birth_year",
            (1900..=i64::from(events::now().year())).contains(&birth),
        );
        let gender = validation.take(input::string(body, "gender", 1, 10));
        validation.check("gender", matches!(gender.as_str(), "male" | "female"));
        let marital = validation.take(input::string(body, "marital_status", 1, 10));
        validation.check(
            "marital_status",
            matches!(marital.as_str(), "single" | "married"),
        );
        let employed = validation.take(input::boolean(body, "is_employed"));
        validation.finish()?;
        Ok(Self {
            name,
            phone,
            password,
            birth,
            gender,
            marital,
            employed,
        })
    }
}

pub async fn signup(state: &AppState, ctx: &Context, data: Signup) -> Result<u64, AppError> {
    let hash = security::hash_password(data.password).await?;
    for attempt in 0..3 {
        let mut tx = state.db.begin().await?;
        let result=sqlx::query("INSERT INTO users (name,phone_number,password_hash,birth_year,gender,marital_status,is_employed,role) VALUES (?,?,?,?,?,?,?,'customer')")
            .bind(&data.name).bind(&data.phone).bind(&hash).bind(data.birth).bind(&data.gender).bind(&data.marital).bind(data.employed).execute(&mut *tx).await;
        let user = match result {
            Ok(r) => r.last_insert_id(),
            Err(e) => {
                let duplicate = crate::db::error_number(&e) == Some(1062)
                    && matches!(&e,sqlx::Error::Database(d) if d.message().contains("uq_users_phone_number"));
                tx.rollback().await?;
                if duplicate {
                    events::publish(
                        state,
                        ctx,
                        "signup",
                        "failure",
                        "phone_already_registered",
                        json!({"method":"password"}),
                    )
                    .await;
                    return Err(AppError::new(
                        409,
                        "phone_already_registered",
                        "이미 가입된 전화번호입니다",
                    ));
                }
                return Err(e.into());
            }
        };
        let digits = format!("{:08}", uuid::Uuid::new_v4().as_u128() % 100_000_000);
        let number = format!("100-{}-{}", &digits[..4], &digits[4..]);
        let result = async {
            let account = sqlx::query("INSERT INTO accounts (user_id,account_number) VALUES (?,?)")
                .bind(user)
                .bind(number)
                .execute(&mut *tx)
                .await?
                .last_insert_id();
            let card = format!(
                "9410{:012}",
                uuid::Uuid::new_v4().as_u128() % 1_000_000_000_000
            );
            sqlx::query("INSERT INTO cards (account_id,card_number) VALUES (?,?)")
                .bind(account)
                .bind(card)
                .execute(&mut *tx)
                .await?;
            Ok::<_, sqlx::Error>(())
        }
        .await;
        if let Err(e) = result {
            tx.rollback().await?;
            if attempt < 2 && crate::db::error_number(&e) == Some(1062) {
                continue;
            }
            return Err(e.into());
        }
        tx.commit().await?;
        events::publish(
            state,
            ctx,
            "signup",
            "success",
            "completed",
            json!({"method":"password"}),
        )
        .await;
        return Ok(user);
    }
    unreachable!()
}
