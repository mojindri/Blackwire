use axum::{
    extract::{DefaultBodyLimit, Request},
    http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Router,
};
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};

use crate::{handlers, state::AppState};

pub fn router(state: AppState) -> Router {
    let static_dir =
        std::env::var("BLACK_UI_STATIC_DIR").unwrap_or_else(|_| "black-ui/frontend/dist".into());
    Router::new()
        .nest("/api", api_router())
        .route("/sub/{token}", get(handlers::subscription_base64))
        .route("/sub/{token}/raw", get(handlers::subscription_raw))
        .fallback_service(ServeDir::new(static_dir).append_index_html_on_directories(true))
        .with_state(state)
        .layer(middleware::from_fn(security_headers))
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .layer(cors_layer())
        .layer(TraceLayer::new_for_http())
}

fn cors_layer() -> CorsLayer {
    if std::env::var("BLACK_UI_DEV_CORS").ok().as_deref() == Some("1") {
        CorsLayer::permissive()
    } else {
        CorsLayer::new()
    }
}

async fn security_headers(request: Request, next: Next) -> Response {
    let is_api = request.uri().path().starts_with("/api/");
    if requires_internal_request_header(is_api, request.method(), request.headers()) {
        return (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"missing Black UI request header"}"#,
        )
            .into_response();
    }

    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    if is_api {
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, max-age=0"),
        );
    }
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=()"),
    );
    headers.insert(
        HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        HeaderName::from_static("x-permitted-cross-domain-policies"),
        HeaderValue::from_static("none"),
    );
    if std::env::var("BLACK_UI_COOKIE_SECURE").ok().as_deref() == Some("1") {
        headers.insert(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        );
    }
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; base-uri 'self'; frame-ancestors 'none'; form-action 'self'",
        ),
    );
    response
}

fn requires_internal_request_header(is_api: bool, method: &Method, headers: &HeaderMap) -> bool {
    is_api
        && matches!(
            *method,
            Method::POST | Method::PUT | Method::PATCH | Method::DELETE
        )
        && headers
            .get("x-black-ui-request")
            .and_then(|value| value.to_str().ok())
            != Some("fetch")
}

fn api_router() -> Router<AppState> {
    Router::new()
        .route("/auth/setup", post(handlers::setup))
        .route("/auth/login", post(handlers::login))
        .route("/auth/logout", post(handlers::logout))
        .route("/auth/me", get(handlers::me))
        .route("/capabilities", get(handlers::capabilities))
        .route("/status", get(handlers::status))
        .route(
            "/settings",
            get(handlers::get_settings).put(handlers::update_settings),
        )
        .route("/runtime/probe", post(handlers::runtime_probe))
        .route("/runtime/traffic", get(handlers::runtime_traffic))
        .route("/service/status", get(handlers::service_status))
        .route(
            "/service/restart-blackwire",
            post(handlers::service_restart_blackwire),
        )
        .route("/service/logs", get(handlers::service_logs))
        .route(
            "/inbounds",
            get(handlers::list_inbounds).post(handlers::create_inbound),
        )
        .route(
            "/inbounds/{id}",
            put(handlers::update_inbound).delete(handlers::delete_inbound),
        )
        .route(
            "/outbounds",
            get(handlers::list_outbounds).post(handlers::create_outbound),
        )
        .route(
            "/outbounds/{id}",
            put(handlers::update_outbound).delete(handlers::delete_outbound),
        )
        .route(
            "/users",
            get(handlers::list_users).post(handlers::create_user),
        )
        .route(
            "/users/{id}",
            put(handlers::update_user).delete(handlers::delete_user),
        )
        .route("/users/{id}/enable", post(handlers::enable_user))
        .route("/users/{id}/disable", post(handlers::disable_user))
        .route("/users/{id}/reset-usage", post(handlers::reset_usage))
        .route("/users/{id}/rotate-uuid", post(handlers::rotate_uuid))
        .route(
            "/users/{id}/rotate-sub-token",
            post(handlers::rotate_sub_token),
        )
        .route("/users/bulk", post(handlers::bulk_users))
        .route("/uuid", post(handlers::generate_uuid))
        .route("/config/sections", get(handlers::list_config_sections))
        .route(
            "/config/sections/{name}",
            put(handlers::update_config_section),
        )
        .route("/config/preview", get(handlers::config_preview))
        .route("/config/import", post(handlers::config_import))
        .route("/config/validate", post(handlers::config_validate))
        .route("/config/write", post(handlers::config_write))
        .route("/config/apply", post(handlers::config_apply))
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue, Method};

    use super::requires_internal_request_header;

    #[test]
    fn mutating_api_request_requires_internal_header() {
        let headers = HeaderMap::new();
        assert!(requires_internal_request_header(
            true,
            &Method::POST,
            &headers
        ));
    }

    #[test]
    fn mutating_api_request_accepts_internal_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-black-ui-request", HeaderValue::from_static("fetch"));
        assert!(!requires_internal_request_header(
            true,
            &Method::POST,
            &headers
        ));
    }

    #[test]
    fn non_api_or_read_request_does_not_require_internal_header() {
        let headers = HeaderMap::new();
        assert!(!requires_internal_request_header(
            true,
            &Method::GET,
            &headers
        ));
        assert!(!requires_internal_request_header(
            false,
            &Method::POST,
            &headers
        ));
    }
}
