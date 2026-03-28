pub mod api;
pub mod pages;
pub mod ws;

use crate::db::Db;
use crate::config::Config;
use axum::{
    Router,
    response::{Html, IntoResponse, Response},
    http::{header, StatusCode},
    routing::{get, post, delete},
};
use rust_embed::Embed;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub config: Arc<Config>,
    pub ws_tx: broadcast::Sender<String>,
}

#[derive(Embed)]
#[folder = "static/"]
struct StaticAssets;

pub fn router(state: AppState) -> Router {
    Router::new()
        // UI pages
        .route("/", get(pages::redirect_dashboard))
        .route("/ui", get(pages::redirect_dashboard))
        .route("/ui/", get(pages::dashboard))
        .route("/ui/devices", get(pages::devices))
        .route("/ui/devices/{id}", get(pages::device_detail))
        .route("/ui/map", get(pages::map))
        .route("/ui/services", get(pages::services))
        .route("/ui/alerts", get(pages::alerts))
        .route("/ui/discovery", get(pages::discovery))
        .route("/ui/performance", get(pages::performance))
        .route("/ui/settings", get(pages::settings))
        // HTMX partials
        .route("/ui/partials/devices-table", get(pages::devices_table_partial))
        .route("/ui/partials/alerts-table", get(pages::alerts_table_partial))
        .route("/ui/partials/services-table", get(pages::services_table_partial))
        .route("/ui/partials/dashboard-cards", get(pages::dashboard_cards_partial))
        // API
        .route("/api/devices", get(api::list_devices).post(api::create_device))
        .route("/api/devices/{id}", get(api::get_device).put(api::update_device).delete(api::delete_device))
        .route("/api/devices/{id}/interfaces", get(api::list_interfaces))
        .route("/api/devices/{id}/services", get(api::list_device_services))
        .route("/api/devices/{id}/metrics", get(api::list_device_metrics))
        .route("/api/links", get(api::list_links).post(api::create_link))
        .route("/api/links/{id}", delete(api::delete_link))
        .route("/api/services", get(api::list_services).post(api::create_service))
        .route("/api/services/{id}", delete(api::delete_service))
        .route("/api/services/{id}/probes", get(api::list_probes))
        .route("/api/alerts", get(api::list_alerts))
        .route("/api/alerts/clear", delete(api::clear_all_alerts))
        .route("/api/alerts/{id}/ack", post(api::acknowledge_alert))
        .route("/api/alerts/{id}", delete(api::delete_alert))
        .route("/api/reset", delete(api::reset_database))
        .route("/api/subnets", get(api::list_subnets).post(api::create_subnet))
        .route("/api/subnets/{id}", delete(api::delete_subnet))
        .route("/api/discovery/scan", post(api::trigger_scan))
        .route("/api/map/positions", get(api::list_positions).put(api::update_position))
        .route("/api/map/auto-layout", post(api::auto_layout))
        .route("/api/metrics", get(api::query_metrics))
        // WebSocket
        .route("/ws", get(ws::ws_handler))
        // Static files
        .route("/ui/static/{*path}", get(serve_static))
        .with_state(state)
}

async fn serve_static(
    axum::extract::Path(path): axum::extract::Path<String>,
) -> impl IntoResponse {
    match StaticAssets::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path)
                .first_or_octet_stream()
                .to_string();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime)],
                content.data.to_vec(),
            )
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Helper to render Askama templates into axum responses.
pub struct HtmlTemplate<T: askama::Template>(pub T);

impl<T: askama::Template> IntoResponse for HtmlTemplate<T> {
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(html) => Html(html).into_response(),
            Err(e) => {
                tracing::error!("template render error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            }
        }
    }
}
