mod basic_auth;
mod config;
mod cors;

type Error = Box<dyn std::error::Error + Send + Sync>;
const SSE_BROADCAST_CAPACITY: usize = 16;

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
use tokio::signal::unix::{signal, SignalKind};

#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<AlertManagerPayload>,
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

    let (tx, _rx) = broadcast::channel::<AlertManagerPayload>(SSE_BROADCAST_CAPACITY);
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

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Alert {
    status: String,
    labels: HashMap<String, String>,
    annotations: HashMap<String, String>,

    #[serde(rename = "startsAt")]
    starts_at: String,
    #[serde(rename = "endsAt")]
    ends_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct AlertManagerPayload {
    alerts: Vec<Alert>
}

async fn subscribe_notifications(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();
    
    let stream = BroadcastStream::new(rx)
        .filter_map(|res| res.ok())
        .filter_map(|msg| {
            log::debug!("[sse][subscribe] streaming event to client: {:#?}", msg);
            
            match Event::default().json_data(&msg) {
                Ok(event) => Some(Ok(event)),
                Err(err) => {
                    log::error!("[sse][subscribe] failed to serialize msg to json: {}", err);
                    None
                }
            }
        });

    Sse::new(stream).keep_alive(KeepAlive::new())
}

async fn publish_notification(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AlertManagerPayload>
) -> impl IntoResponse {
    log::debug!("[sse][publish] incoming notification payload: {:#?}", payload);

    match state.tx.send(payload) {
        Ok(receiver_count) => {
            log::debug!("[sse][publish] successfully broadcasted to {} client(s)", receiver_count);
        },
        Err(_) => {
            log::warn!("[sse][publish] message sent but no active SSE subscribers connected");
        }
    }

    (StatusCode::NO_CONTENT).into_response()
}

async fn shutdown_signal() {
    let mut sigint = signal(SignalKind::interrupt())
        .expect("failed to bind SIGINT handler");

    let mut sigterm = signal(SignalKind::terminate())
        .expect("failed to bind SIGTERM handler");

    tokio::select! {
        _ = sigint.recv() => log::info!("SIGINT received, Gracefully shutting down."),
        _ = sigterm.recv() => log::info!("SIGTERM received, Gracefully shutting down."),
    }
}