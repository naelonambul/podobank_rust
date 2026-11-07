//! backend/app/db/mariadb.py, mongodb.py와 같은 연결 설정.

use std::str::FromStr;
use std::time::Duration;

use mongodb::options::ClientOptions;
use sqlx::Executor;
use sqlx::mysql::{MySqlConnectOptions, MySqlPool, MySqlPoolOptions};

use crate::config::{DB_MAX_OVERFLOW, DB_POOL_SIZE, DB_POOL_TIMEOUT_SECONDS};

/// SQLAlchemy 주소(mysql+pymysql://)를 그대로 받는다.
fn sqlx_url(url: &str) -> String {
    match url.split_once("://") {
        Some((scheme, rest)) if scheme.starts_with("mysql") => format!("mysql://{rest}"),
        _ => url.to_string(),
    }
}

/// 연결은 처음 쓸 때 맺는다(SQLAlchemy create_engine처럼 시작 시 DB가 없어도 뜬다).
pub fn make_pool(url: &str) -> Result<MySqlPool, sqlx::Error> {
    let options = MySqlConnectOptions::from_str(&sqlx_url(url))?;
    Ok(MySqlPoolOptions::new()
        .max_connections(DB_POOL_SIZE + DB_MAX_OVERFLOW)
        .min_connections(0)
        .acquire_timeout(Duration::from_secs(DB_POOL_TIMEOUT_SECONDS))
        // pool_pre_ping=True
        .test_before_acquire(true)
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                conn.execute("SET time_zone = '+09:00', innodb_lock_wait_timeout = 3")
                    .await?;
                conn.execute("SET SESSION TRANSACTION ISOLATION LEVEL READ COMMITTED")
                    .await?;
                Ok(())
            })
        })
        .connect_lazy_with(options))
}

/// serverSelectionTimeoutMS=500, connectTimeoutMS=500. socketTimeoutMS=2000은 Rust 드라이버가
/// 지원하지 않아(공식 문서: "unsupported") 쓰는 쪽에서 작업마다 2초 제한을 건다.
pub const MONGO_SOCKET_TIMEOUT: Duration = Duration::from_millis(2000);

/// SQLSTATE와 서버 오류 번호는 다르다. 잠금·중복 오류는 서버 번호로 판별한다.
pub fn error_number(error: &sqlx::Error) -> Option<u16> {
    match error {
        sqlx::Error::Database(db) => db
            .try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>()
            .map(|db| db.number()),
        _ => None,
    }
}

pub async fn make_mongo(
    url: &str,
    db_name: &str,
) -> Result<mongodb::Database, mongodb::error::Error> {
    let mut options = ClientOptions::parse(url).await?;
    options.server_selection_timeout = Some(Duration::from_millis(500));
    options.connect_timeout = Some(Duration::from_millis(500));
    Ok(mongodb::Client::with_options(options)?.database(db_name))
}
