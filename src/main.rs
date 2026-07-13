mod basic_auth;
mod config;
mod cors;

type Error = Box<dyn std::error::Error + Send + Sync>;

use crate::basic_auth::BasicAuth;
use crate::cors::Cors;

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

use axum::Json;
use axum::response::IntoResponse;
use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Router,
};

use futures::stream::Stream;

use http::StatusCode;
use serde::{Deserialize, Serialize};

use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as _;

#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<String>,
    basic_auth: BasicAuth
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let log_config_file = std::env::var("LOG_CONFIG_FILE")
        .expect("Environment variable `LOG_CONFIG_FILE` not found");

    log4rs::init_file(&log_config_file, Default::default())
        .expect("Failed to init log4rs");

    let config = config::Config::get_config()?;
    let cors = Cors::new(config.cors)?;

    let (tx, _rx) = broadcast::channel::<String>(16);
    let basic_auth = BasicAuth::new(config.basic_auth_users);
    let state = Arc::new(AppState { tx, basic_auth });

    let app = Router::new()
        .route("/api/amora/notifications", get(subscribe_notifications))
        .route("/api/amora/notifications", post(publish_notification)
            .route_layer(axum::middleware::from_fn_with_state(state.clone(), basic_auth::basic_auth)))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:12013").await.unwrap();
    log::info!("Server listening on http://0.0.0.0:12013");
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
struct Alert {
    status: String,
    labels: HashMap<String, String>,
    annotations: HashMap<String, String>,

    #[serde(rename = "startsAt")]
    starts_at: String,
    #[serde(rename = "endsAt")]
    ends_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct AlertManagerPayload {
    alerts: Vec<Alert>
}

async fn subscribe_notifications(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();
    
    let stream = BroadcastStream::new(rx)
        .filter_map(|res| res.ok())
        .map(|msg| {
            log::debug!("[sse][subscribe] streaming event to client: {}", msg);
            Ok(Event::default().data(msg))
        });

    Sse::new(stream).keep_alive(KeepAlive::new())
}

async fn publish_notification(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AlertManagerPayload>
) -> impl IntoResponse {
    let json_string = match serde_json::to_string(&payload) {
        Ok(s) => s,
        Err(e) => {
            log::error!("[sse][publish] serialization error: {}", e);
            return (StatusCode::BAD_REQUEST, "Invalid payload").into_response();
        }
    };

    log::debug!("[sse][publish] incoming notification payload: {}", json_string);

    match state.tx.send(json_string) {
        Ok(receiver_count) => {
            log::debug!("[sse][publish] successfully broadcasted to {} client(s)", receiver_count);
        },
        Err(_) => {
            log::warn!("[sse][publish] message sent but no active SSE subscribers connected");
        }
    }

    (StatusCode::NO_CONTENT).into_response()
}
