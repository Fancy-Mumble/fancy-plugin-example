//! HTTP status page for the `fancy-greeter` plugin.
//!
//! `GET /status` returns a small JSON document so operators can verify the
//! plugin is running and see live session counts without parsing log files.
//!
//! The server is started by [`start`] inside `on_load` and shut down
//! gracefully in `on_unload` via [`ServerHandle::shutdown`].

use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use axum::{extract::State, response::IntoResponse, routing, Json, Router};
use serde_json::json;
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Shared state exposed to the HTTP handler
// ---------------------------------------------------------------------------

/// Live statistics written by the plugin and read by the HTTP handler.
#[derive(Debug, Clone, Default)]
pub struct StatusData {
    /// Number of sessions currently tracked across all virtual servers.
    pub active_sessions: usize,
    /// Greeting template as configured at load time.
    pub greeting_template: String,
}

// ---------------------------------------------------------------------------
// Server lifecycle
// ---------------------------------------------------------------------------

/// Start the HTTP server and return a [`ServerHandle`] for graceful shutdown.
///
/// # Errors
///
/// Returns an error if the TCP listener cannot bind to `127.0.0.1:<port>`.
pub async fn start(
    status: Arc<RwLock<StatusData>>,
    port: u16,
) -> Result<ServerHandle, std::io::Error> {
    let router = Router::new()
        .route("/status", routing::get(status_handler))
        .with_state(status);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let server_task = tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    tracing::info!(addr = %local_addr, "fancy-greeter: HTTP status page listening");

    Ok(ServerHandle {
        shutdown_tx: Some(shutdown_tx),
        server_task,
        local_addr,
    })
}

// ---------------------------------------------------------------------------
// Handle
// ---------------------------------------------------------------------------

/// Handle returned by [`start`]; call [`Self::shutdown`] to stop the server.
///
/// Dropping the handle without calling `shutdown` leaks the background task.
#[must_use]
pub struct ServerHandle {
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    server_task: tokio::task::JoinHandle<()>,
    local_addr: SocketAddr,
}

impl std::fmt::Debug for ServerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerHandle")
            .field("local_addr", &self.local_addr)
            .finish_non_exhaustive()
    }
}

impl ServerHandle {
    /// Address the server is actually listening on (useful for logs).
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Signal the server to stop and await the background task's completion.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        let _ = self.server_task.await;
    }
}

// ---------------------------------------------------------------------------
// Request handler
// ---------------------------------------------------------------------------

async fn status_handler(State(status): State<Arc<RwLock<StatusData>>>) -> impl IntoResponse {
    let (sessions, template) = status
        .read()
        .map(|s| (s.active_sessions, s.greeting_template.clone()))
        .unwrap_or_default();

    Json(json!({
        "plugin":            "fancy-greeter",
        "active_sessions":   sessions,
        "greeting_template": template,
    }))
}
