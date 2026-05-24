//! Fancy Greeter - annotated example Fancy Mumble server plugin.
//!
//! # What this plugin does
//!
//! * Sends a configurable greeting to every user immediately after they
//!   connect (`on_client_connected`).
//! * Answers `Ping` plugin messages with a `Pong` carrying the current
//!   session count (`on_plugin_message`, the new wire-200 envelope).
//! * Also responds to the legacy `PluginDataTransmission` flavour of
//!   ping via `on_plugin_data`.
//! * Serves a tiny `GET /status` JSON page over HTTP so operators can
//!   verify the plugin is running without parsing log files.
//!
//! # How dynamic loading works
//!
//! This crate compiles to a `cdylib` (see `[lib] crate-type` in
//! `Cargo.toml`).  The Mumble plugin host scans the configured plugin
//! directory (`/etc/mumble/plugins` by default), `dlopen`s every
//! `*.so` / `*.dll` / `*.dylib` file it finds, looks up the
//! `fancy_export_plugin!` root module symbol, and refuses to load any
//! cdylib whose declared `abi_version` does not match its own.
//!
//! The boundary between host and plugin is `abi_stable`-based, which
//! means we use FFI-safe wrappers (`RStr`, `RString`, `RVec`, `RArc`,
//! `RResult`, `ROption`) instead of their `std` counterparts.
//!
//! # Async model
//!
//! Trait methods on [`MumblePlugin`] are **synchronous** across the FFI
//! boundary - the host calls them directly without a runtime in
//! scope.  This plugin therefore owns its **private** tokio runtime
//! (created in `on_load`, dropped in `on_unload`).  The HTTP status
//! server is spawned on that runtime; synchronous hooks only need to
//! mutate in-memory state and call back into the (sync) `ctx`.

pub mod config;
pub mod server;
pub mod types;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use abi_stable::std_types::{
    RArc, ROk, ROption, RResult, RSlice, RStr, RString, RVec,
};
use mumble_plugin_api::{
    fancy_export_plugin, ClientInfo, DebugRow, MumblePlugin, PluginContext_TO, PluginError,
    PluginInfo, PluginMessageIn, PluginMessageOut, PluginResult, ServerId, SessionId,
};
use tokio::runtime::Runtime;

use crate::config::GreeterConfig;
use crate::server::{ServerHandle, StatusData};
use crate::types::{
    GreetingPayload, PingPayload, PongPayload, MSG_GREETING, MSG_PING, MSG_PONG,
};

/// Stable plugin identifier - matches the `plugin.<name>.` INI prefix.
const PLUGIN_NAME: &str = "fancy-greeter";

/// Crate version, surfaced in the registry and to the developer panel.
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------------------------------------------------------------------------
// Runtime state
// ---------------------------------------------------------------------------

/// Everything created in `on_load` and torn down in `on_unload`.
///
/// Owned by the plugin behind a `Mutex<Option<...>>` so hooks can take
/// out a snapshot, drop the lock, and then call back into the host
/// without risking re-entrant deadlocks.
struct RunningState {
    /// Plugin-owned tokio runtime.  Drop order matters: the
    /// `ServerHandle` (which holds a `JoinHandle` from this runtime)
    /// must be shut down *before* the runtime is dropped.
    runtime: Runtime,
    /// Trait object for calling back into the host.
    ctx: PluginContext_TO<RArc<()>>,
    /// Frozen configuration snapshot read at load time.
    config: Arc<GreeterConfig>,
    /// `server_id` -> (`session_id` -> username).
    sessions: HashMap<ServerId, HashMap<SessionId, String>>,
    /// Live stats shared with the HTTP `/status` handler.
    status: Arc<RwLock<StatusData>>,
    /// HTTP server handle; `None` if the TCP bind failed at load time.
    http: Option<ServerHandle>,
}

impl std::fmt::Debug for RunningState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunningState")
            .field("config", &self.config)
            .field(
                "session_count",
                &self.sessions.values().map(HashMap::len).sum::<usize>(),
            )
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Plugin struct
// ---------------------------------------------------------------------------

/// Annotated example plugin - greets new users and answers Ping messages.
#[derive(Default)]
pub struct GreeterPlugin {
    /// `None` before `on_load` and after `on_unload`, `Some` in between.
    inner: Mutex<Option<RunningState>>,
}

impl std::fmt::Debug for GreeterPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GreeterPlugin").finish_non_exhaustive()
    }
}

impl GreeterPlugin {
    /// Create a new, not-yet-loaded plugin instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// MumblePlugin implementation
// ---------------------------------------------------------------------------

impl MumblePlugin for GreeterPlugin {
    fn name(&self) -> RStr<'_> {
        RStr::from(PLUGIN_NAME)
    }

    fn version(&self) -> RStr<'_> {
        RStr::from(PLUGIN_VERSION)
    }

    fn info_json(&self) -> RString {
        let (sessions, port) = with_state(&self.inner, |state| {
            let total: usize = state.sessions.values().map(HashMap::len).sum();
            (total, state.config.http_port)
        })
        .unwrap_or((0, 0));

        let info = PluginInfo {
            description: "Example plugin: greets new users and answers Ping messages.".into(),
            author: Some("Fancy Mumble".into()),
            homepage: Some("https://github.com/Fancy-Mumble".into()),
            capabilities: vec!["greeting".into(), "ping".into(), "http-status".into()],
            debug_rows: vec![
                DebugRow { label: "active_sessions".into(), value: sessions.to_string() },
                DebugRow { label: "http_port".into(), value: port.to_string() },
            ],
        };

        match info.to_validated_json() {
            Ok(bytes) => RString::from(String::from_utf8_lossy(&bytes).into_owned()),
            Err(e) => {
                tracing::warn!(error = %e, "fancy-greeter: info_json validation failed");
                RString::from("{}")
            }
        }
    }

    fn on_load(&self, ctx: PluginContext_TO<RArc<()>>) -> PluginResult<()> {
        let config = match GreeterConfig::from_lookup(|key| lookup_config(&ctx, key)) {
            Ok(c) => Arc::new(c),
            Err(e) => return RResult::RErr(PluginError::Config(RString::from(e.to_string()))),
        };

        let runtime = match build_runtime() {
            Ok(rt) => rt,
            Err(e) => return RResult::RErr(PluginError::from(e)),
        };

        let status = Arc::new(RwLock::new(StatusData {
            active_sessions: 0,
            greeting_template: config.greeting_template.clone(),
        }));

        let http = match runtime.block_on(server::start(Arc::clone(&status), config.http_port)) {
            Ok(h) => {
                tracing::info!(port = config.http_port, "fancy-greeter: HTTP status page ready");
                Some(h)
            }
            Err(e) => {
                tracing::warn!(
                    port = config.http_port,
                    error = %e,
                    "fancy-greeter: HTTP bind failed; continuing without status page"
                );
                None
            }
        };

        store_state(
            &self.inner,
            RunningState {
                runtime,
                ctx,
                config,
                sessions: HashMap::new(),
                status,
                http,
            },
        );

        tracing::info!("{PLUGIN_NAME} v{PLUGIN_VERSION} loaded");
        ROk(())
    }

    fn on_unload(&self) -> PluginResult<()> {
        if let Some(state) = take_state(&self.inner) {
            if let Some(handle) = state.http {
                state.runtime.block_on(handle.shutdown());
            }
            // `runtime` drops here; tokio shuts it down cleanly.
            tracing::info!("{PLUGIN_NAME} unloaded");
        }
        ROk(())
    }

    fn on_client_connected(&self, info: ClientInfo) -> PluginResult<()> {
        let username = info.username.as_str().to_owned();
        let _ = with_state_mut(&self.inner, |state| {
            let _ = state
                .sessions
                .entry(info.server_id)
                .or_default()
                .insert(info.session_id, username.clone());

            let total: usize = state.sessions.values().map(HashMap::len).sum();
            if let Ok(mut s) = state.status.write() {
                s.active_sessions = total;
            }

            send_greeting(
                &state.ctx,
                &state.config,
                info.server_id,
                info.session_id,
                &username,
            );
        });
        ROk(())
    }

    fn on_client_disconnected(
        &self,
        server_id: ServerId,
        session: SessionId,
    ) -> PluginResult<()> {
        let _ = with_state_mut(&self.inner, |state| {
            let username = state
                .sessions
                .get_mut(&server_id)
                .and_then(|m| m.remove(&session));

            let total: usize = state.sessions.values().map(HashMap::len).sum();
            if let Ok(mut s) = state.status.write() {
                s.active_sessions = total;
            }

            if let Some(name) = username {
                let farewell = config::expand_template(&state.config.farewell_template, &name);
                tracing::info!(server_id, session, "{farewell}");
            }
        });
        ROk(())
    }

    fn on_plugin_data(
        &self,
        server_id: ServerId,
        sender: SessionId,
        data_id: RStr<'_>,
        data: RSlice<'_, u8>,
    ) -> PluginResult<()> {
        if data_id.as_str() != MSG_PING {
            return ROk(());
        }
        let _ = with_state(&self.inner, |state| {
            let active: usize = state.sessions.values().map(HashMap::len).sum();
            reply_pong_via_data(&state.ctx, server_id, sender, data.as_slice(), active);
        });
        ROk(())
    }

    fn on_plugin_message(&self, msg: PluginMessageIn) -> PluginResult<()> {
        if msg.payload_type.as_str() != MSG_PING {
            return ROk(());
        }
        let _ = with_state(&self.inner, |state| {
            let active: usize = state.sessions.values().map(HashMap::len).sum();
            reply_pong_via_message(&state.ctx, &msg, active);
        });
        ROk(())
    }
}

// ---------------------------------------------------------------------------
// State helpers - centralised so callers do not sprinkle `.lock()` checks.
// ---------------------------------------------------------------------------

/// Read-only snapshot under the inner lock; returns `None` if the lock
/// is poisoned or the plugin is not loaded.
fn with_state<R>(
    cell: &Mutex<Option<RunningState>>,
    f: impl FnOnce(&RunningState) -> R,
) -> Option<R> {
    let guard = cell.lock().ok()?;
    let state = guard.as_ref()?;
    Some(f(state))
}

/// Mutable access under the inner lock; returns `None` if poisoned or
/// the plugin is not loaded.
fn with_state_mut<R>(
    cell: &Mutex<Option<RunningState>>,
    f: impl FnOnce(&mut RunningState) -> R,
) -> Option<R> {
    let mut guard = cell.lock().ok()?;
    let state = guard.as_mut()?;
    Some(f(state))
}

fn store_state(cell: &Mutex<Option<RunningState>>, state: RunningState) {
    if let Ok(mut g) = cell.lock() {
        *g = Some(state);
    }
}

fn take_state(cell: &Mutex<Option<RunningState>>) -> Option<RunningState> {
    cell.lock().ok().and_then(|mut g| g.take())
}

fn build_runtime() -> std::io::Result<Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .thread_name("fancy-greeter")
        .build()
}

fn lookup_config(ctx: &PluginContext_TO<RArc<()>>, key: &str) -> Option<String> {
    match ctx.get_config(RStr::from(key)) {
        ROption::RSome(v) => Some(v.into_string()),
        ROption::RNone => None,
    }
}

// ---------------------------------------------------------------------------
// Outbound helpers
// ---------------------------------------------------------------------------

fn send_greeting(
    ctx: &PluginContext_TO<RArc<()>>,
    config: &GreeterConfig,
    server_id: ServerId,
    session_id: SessionId,
    username: &str,
) {
    let message = config::expand_template(&config.greeting_template, username);
    let bytes = match serde_json::to_vec(&GreetingPayload { message }) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("fancy-greeter: serialize Greeting failed: {e}");
            return;
        }
    };
    let result = ctx.send_plugin_data(
        server_id,
        session_id,
        RStr::from(MSG_GREETING),
        RSlice::from(bytes.as_slice()),
    );
    if let RResult::RErr(e) = result {
        tracing::warn!(session = session_id, error = ?e, "fancy-greeter: send Greeting failed");
    }
}

fn reply_pong_via_data(
    ctx: &PluginContext_TO<RArc<()>>,
    server_id: ServerId,
    sender: SessionId,
    data: &[u8],
    active_sessions: usize,
) {
    let Some(bytes) = encode_pong(data, sender, active_sessions) else { return; };
    let result = ctx.send_plugin_data(
        server_id,
        sender,
        RStr::from(MSG_PONG),
        RSlice::from(bytes.as_slice()),
    );
    if let RResult::RErr(e) = result {
        tracing::warn!(session = sender, error = ?e, "fancy-greeter: send Pong (data) failed");
    }
}

fn reply_pong_via_message(
    ctx: &PluginContext_TO<RArc<()>>,
    msg: &PluginMessageIn,
    active_sessions: usize,
) {
    let Some(bytes) = encode_pong(msg.payload.as_slice(), msg.sender_session, active_sessions)
    else {
        return;
    };
    let reply = PluginMessageOut {
        server_id: msg.server_id,
        plugin_name: RString::from(PLUGIN_NAME),
        payload_type: RString::from(MSG_PONG),
        payload: RVec::from(bytes),
        target_sessions: {
            let mut v: RVec<SessionId> = RVec::new();
            v.push(msg.sender_session);
            v
        },
        channel_id: ROption::RNone,
    };
    if let RResult::RErr(e) = ctx.send_plugin_message(reply) {
        tracing::warn!(error = ?e, "fancy-greeter: send Pong (message) failed");
    }
}

fn encode_pong(data: &[u8], sender: SessionId, active_sessions: usize) -> Option<Vec<u8>> {
    let ping: PingPayload = match serde_json::from_slice(data) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(sender, "fancy-greeter: bad Ping: {e}");
            return None;
        }
    };
    match serde_json::to_vec(&PongPayload { nonce: ping.nonce, active_sessions }) {
        Ok(b) => Some(b),
        Err(e) => {
            tracing::warn!("fancy-greeter: serialize Pong failed: {e}");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// cdylib entry point
// ---------------------------------------------------------------------------
//
// `fancy_export_plugin!` generates the `extern "C"` factory the host
// looks up by symbol name after `dlopen`.  Without this, the cdylib
// loads but the host cannot instantiate the plugin.

fancy_export_plugin!(GreeterPlugin::new);
