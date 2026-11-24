mod auth;
mod config;
mod cors;

type Error = Box<dyn std::error::Error + Send + Sync>;
const SSE_BROADCAST_CAPACITY: usize = 256;

use crate::auth::auth_bearer;
use crate::cors::Cors;

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};

use futures_util::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};

use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct AppState {
    tx: broadcast::Sender<AlertManagerPayload>,
    shutdown_token: CancellationToken,
    bearer_token: Option<Arc<String>>,
    keep_alive_interval: u64,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = config::Config::get_config()?;
    let keep_alive_interval = config.get_keep_alive_interval();
    let bearer_token = config.bearer_token.map(Arc::new);
    let cors = Cors::new(config.cors)?;

    let (tx, _rx) = broadcast::channel::<AlertManagerPayload>(SSE_BROADCAST_CAPACITY);
    let shutdown_token = CancellationToken::new();

    let state = Arc::new(AppState {
        tx,
        shutdown_token: shutdown_token.clone(),
        bearer_token,
        keep_alive_interval,
    });

    let api_routes = Router::new()
        .route("/api/events", get(subscribe_events))
        .route(
            "/api/webhook",
            post(publish_webhook)
                .route_layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    auth_bearer,
                )),
        );

    let base_path = config::Config::get_base_path();

    let app = match base_path.as_deref() {
        Some(base) => Router::new().nest(base, api_routes),
        None => api_routes,
    }
    .layer(cors)
    .with_state(state);

    let port = config::Config::get_port();
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(
        "Server listening on http://{}{}",
        addr,
        base_path.as_deref().unwrap_or("")
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown_token))
        .await?;

    tracing::info!("Server stopped successfully.");

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
    alerts: Vec<Alert>,
}

async fn subscribe_events(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();
    let token = state.shutdown_token.clone();

    let stream = BroadcastStream::new(rx)
        .filter_map(|res| async move {
            let msg = res.ok()?;

            tracing::debug!("Streaming event to client: {:?}", msg);

            match Event::default().json_data(&msg) {
                Ok(event) => Some(Ok(event)),
                Err(err) => {
                    tracing::error!("Failed to serialize msg to json: {}", err);
                    None
                }
            }
        })
        .take_until(async move { token.cancelled().await });

    Sse::new(stream).keep_alive(
        KeepAlive::new().interval(std::time::Duration::from_secs(state.keep_alive_interval)),
    )
}

async fn publish_webhook(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AlertManagerPayload>,
) -> impl IntoResponse {
    tracing::debug!("Incoming notification payload: {:?}", payload);

    match state.tx.send(payload) {
        Ok(receiver_count) => {
            tracing::debug!("Successfully broadcasted to {} client(s)", receiver_count);
        }
        Err(_) => {
            tracing::warn!("Message sent but no active SSE subscribers connected");
        }
    }

    StatusCode::NO_CONTENT
}

async fn shutdown_signal(shutdown_token: CancellationToken) {
    let mut sigint = signal(SignalKind::interrupt()).expect("failed to bind SIGINT handler");
    let mut sigterm = signal(SignalKind::terminate()).expect("failed to bind SIGTERM handler");

    tokio::select! {
        _ = sigint.recv() => {
            shutdown_token.cancel();
            tracing::info!("SIGINT received, gracefully shutting down.");
        },
        _ = sigterm.recv() => {
            shutdown_token.cancel();
            tracing::info!("SIGTERM received, gracefully shutting down.");
        },
    }
}
