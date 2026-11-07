use crate::error::AppError;
use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use sha2::{Digest, Sha256};

pub fn token_hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn token() -> Result<String, AppError> {
    use std::io::Read;
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(|_| internal())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn internal() -> AppError {
    AppError::new(500, "internal_error", "일시적인 오류가 발생했습니다")
}

pub async fn hash_password(password: String) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || {
        let params = Params::new(65_536, 3, 4, Some(32)).map_err(|_| internal())?;
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password(password.as_bytes())
            .map(|h| h.to_string())
            .map_err(|_| internal())
    })
    .await
    .map_err(|_| internal())?
}

pub async fn verify_password(hash: Option<String>, password: String) -> Result<bool, AppError> {
    // 없는 사용자도 Python처럼 실제 해시 한 번을 검증한다.
    static DUMMY: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
    let dummy = DUMMY
        .get_or_try_init(|| hash_password("not-a-real-password".to_owned()))
        .await?;
    let exists = hash.is_some();
    let stored = hash.unwrap_or_else(|| dummy.clone());
    tokio::task::spawn_blocking(move || {
        exists
            & PasswordHash::new(&stored)
                .map(|h| {
                    Argon2::default()
                        .verify_password(password.as_bytes(), &h)
                        .is_ok()
                })
                .unwrap_or(false)
    })
    .await
    .map_err(|_| internal())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn password_hash_matches_python_parameters() {
        let hash = hash_password("example-test-password".into()).await.unwrap();
        assert!(hash.contains("m=65536,t=3,p=4"));
        assert!(
            verify_password(Some(hash.clone()), "example-test-password".into())
                .await
                .unwrap()
        );
        assert!(
            !verify_password(Some(hash), "different-example".into())
                .await
                .unwrap()
        );
    }
    #[test]
    fn sha256_session_hash() {
        assert_eq!(
            token_hash("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
