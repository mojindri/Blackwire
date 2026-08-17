use axum::http::{header, HeaderMap};
use chrono::{Duration, Utc};

use crate::{error::AppError, mysql_state::AppState, util};

const SESSION_COOKIE: &str = "black_ui_session";
const SESSION_MAX_AGE_SECONDS: i64 = 7 * 24 * 60 * 60;

pub async fn require(headers: &HeaderMap, state: &AppState) -> Result<i64, AppError> {
    let token = session_token(headers).ok_or_else(AppError::unauthorized)?;
    state
        .store
        .session_admin(blake3::hash(token.as_bytes()).as_bytes())
        .await
        .map_err(|error| AppError::internal(error.into()))?
        .ok_or_else(AppError::unauthorized)
}

pub async fn create_first_admin(
    state: &AppState,
    username: &str,
    password: &str,
) -> Result<(), AppError> {
    let username = normalized_username(username)?;
    if password.len() < 8 {
        return Err(AppError::bad_request("password must be at least 8 characters"));
    }
    let salt = util::random_token(24);
    let created = state
        .store
        .create_first_admin(
            &username,
            util::hash_password(password, &salt).as_bytes(),
            salt.as_bytes(),
        )
        .await
        .map_err(|error| AppError::internal(error.into()))?;
    if !created {
        return Err(AppError::bad_request("setup already completed"));
    }
    Ok(())
}

pub async fn create_admin_session(
    state: &AppState,
    username: &str,
    password: &str,
) -> Result<(String, String), AppError> {
    let username = normalized_username(username)?;
    let admin = state
        .store
        .admin_by_username(&username)
        .await
        .map_err(|error| AppError::internal(error.into()))?
        .ok_or_else(|| AppError::unauthorized_message("invalid username or password"))?;
    let salt = String::from_utf8(admin.password_salt)
        .map_err(|error| AppError::internal(error.into()))?;
    let expected = String::from_utf8(admin.password_hash)
        .map_err(|error| AppError::internal(error.into()))?;
    if util::hash_password(password, &salt) != expected {
        return Err(AppError::unauthorized_message("invalid username or password"));
    }
    let token = util::random_token(48);
    state
        .store
        .create_session(
            blake3::hash(token.as_bytes()).as_bytes(),
            admin.id,
            Utc::now() + Duration::seconds(SESSION_MAX_AGE_SECONDS),
        )
        .await
        .map_err(|error| AppError::internal(error.into()))?;
    Ok((token, admin.username))
}

pub async fn delete_session(headers: &HeaderMap, state: &AppState) -> Result<(), AppError> {
    if let Some(token) = session_token(headers) {
        state
            .store
            .delete_session(blake3::hash(token.as_bytes()).as_bytes())
            .await
            .map_err(|error| AppError::internal(error.into()))?;
    }
    Ok(())
}

pub fn session_cookie(token: &str) -> String {
    let secure = if std::env::var("BLACK_UI_COOKIE_SECURE").ok().as_deref() == Some("1") {
        "; Secure"
    } else {
        ""
    };
    format!("{SESSION_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={SESSION_MAX_AGE_SECONDS}{secure}")
}

pub fn expired_session_cookie() -> String {
    format!("{SESSION_COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0")
}

fn normalized_username(username: &str) -> Result<String, AppError> {
    let username = username.trim();
    if username.is_empty() || username.len() > 256 {
        return Err(AppError::bad_request("invalid username"));
    }
    Ok(username.to_owned())
}

fn session_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .or_else(|| {
            headers
                .get(header::COOKIE)?
                .to_str()
                .ok()?
                .split(';')
                .map(str::trim)
                .find_map(|part| part.strip_prefix(&format!("{SESSION_COOKIE}=")))
        })
        .filter(|value| !value.is_empty())
}
