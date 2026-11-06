//! backend/app/core/config.py와 같은 환경변수·숫자.

use std::env;
use std::path::{Path, PathBuf};

pub const LOCK_WAIT_SECONDS: u64 = 3;
pub const DB_POOL_SIZE: u32 = 10;
pub const DB_MAX_OVERFLOW: u32 = 10;
pub const DB_POOL_TIMEOUT_SECONDS: u64 = 5;

#[derive(Clone, Debug)]
pub struct Settings {
    pub mariadb_url: String,
    pub mongodb_url: String,
    pub mongodb_db: String,
    pub event_log_path: PathBuf,
    pub app_log_path: PathBuf,
    pub cookie_secure: bool,
    pub instance_name: String,
    pub kakao_client_id: String,
    pub kakao_client_secret: String,
    pub kakao_redirect_uri: String,
}

fn var_or(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_string())
}

/// Python Path.resolve()처럼 절대 경로로 바꾼다(파일이 없어도 된다).
fn resolve(path: &Path) -> PathBuf {
    std::fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

impl Settings {
    pub fn from_env() -> Result<Self, String> {
        let settings = Settings {
            mariadb_url: env::var("MARIADB_URL").map_err(|_| "MARIADB_URL is required".to_string())?,
            mongodb_url: env::var("MONGODB_URL").map_err(|_| "MONGODB_URL is required".to_string())?,
            mongodb_db: var_or("MONGODB_DB", "podobank"),
            event_log_path: var_or("EVENT_LOG_PATH", "./var/log/events.log").into(),
            app_log_path: var_or("APP_LOG_PATH", "./var/log/app.log").into(),
            cookie_secure: var_or("COOKIE_SECURE", "false").to_lowercase() == "true",
            instance_name: var_or("INSTANCE_NAME", "api"),
            kakao_client_id: var_or("KAKAO_CLIENT_ID", ""),
            kakao_client_secret: var_or("KAKAO_CLIENT_SECRET", ""),
            kakao_redirect_uri: var_or("KAKAO_REDIRECT_URI", ""),
        };
        if resolve(&settings.app_log_path) == resolve(&settings.event_log_path) {
            return Err("APP_LOG_PATH and EVENT_LOG_PATH must be different".to_string());
        }
        Ok(settings)
    }
}
