//! Fancy Greeter - annotated example Fancy Mumble server plugin.
//!
//! # What this plugin does
//!
//! * Sends a configurable greeting to every user immediately after they
//!   connect (`on_client_connected`).
//! * Answers `Ping` plugin messages with a `Pong` carrying the current
//!   session count (`on_plugin_message`, the wire-200 envelope).
//!
//! All inbound/outbound plugin traffic goes through the wire-200
//! `PluginMessage` envelope (`on_plugin_message` / `send_plugin_message`).
//! Fancy Mumble forbids the legacy `PluginDataTransmission` channel.
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
//! boundary - the host calls them directly without a runtime in scope.
//! All hooks only mutate in-memory state and call back into the (sync) `ctx`.

pub mod config;
pub mod types;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::config::GreeterConfig;
use crate::types::{GreetingPayload, PingPayload, PongPayload, MSG_GREETING, MSG_PING, MSG_PONG};
use abi_stable::std_types::ROk;
use mumble_plugin_api::{
    fancy_export_plugin, fancy_plugin, handler_id, message, row, show_modal, toast, Button,
    ButtonStyle, ClientInfo, Host, InteractionResponse, PluginMessageIn, PluginResult, ServerId,
    SessionId, TextInput, TextInputStyle, ToastLevel,
};

/// Stable plugin identifier - matches the `plugin.<name>.` INI prefix.
const PLUGIN_NAME: &str = "fancy-greeter";

/// Crate version, surfaced in the registry and to the developer panel.
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------------------------------------------------------------------------
// Runtime state
// ---------------------------------------------------------------------------

/// Mutable runtime state created in `on_load` and torn down in
/// `on_unload`.  The plugin host owns the `PluginContext` itself and
/// hands one out to every callback via [`Host`], so this struct only
/// carries per-plugin state.
struct RunningState {
    /// Frozen configuration snapshot read at load time.
    config: Arc<GreeterConfig>,
    /// `server_id` -> (`session_id` -> username).
    sessions: HashMap<ServerId, HashMap<SessionId, String>>,
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
    /// Per-plugin state.  Locked only briefly inside each callback.
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

    /// Total live sessions across every server.  `0` before
    /// `on_load` populates inner state.  Used by [`Self::info_json`]
    /// (via the `plugin_info!` block) to expose a developer-panel row.
    fn active_session_count(&self) -> usize {
        with_state(&self.inner, |s| s.sessions.values().map(HashMap::len).sum()).unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// MumblePlugin implementation
// ---------------------------------------------------------------------------

#[fancy_plugin(name = PLUGIN_NAME, version = PLUGIN_VERSION)]
impl GreeterPlugin {
    // Manifest declaration.  The `slash_commands` block is intentionally
    // absent - `#[fancy_plugin]` collects every `#[command]`-tagged
    // method below and splices their entries into the manifest at
    // build time.  Everything else (tags, debug_info, settings_panels,
    // capabilities) stays declarative.
    plugin_info! {
        description: "Example plugin: greets new users and answers Ping messages.",
        author: "Fancy Mumble",
        homepage: "https://github.com/Fancy-Mumble",
        tags: ["greeting", "ping"],
        debug_info: {
            "active_sessions" => self.active_session_count(),
        },
        manifest: {
            capabilities: [SlashCommands, Components, Modals, Notifications, SettingsPanel],
            settings_panels: [
                {
                    id: "status",
                    title: "Greeter status",
                    rows: [
                        "Greeting template" => "Welcome, {username}!",
                        "Tier-1 demo"       => "Type /greet <name> [loud=true] to try it",
                    ],
                },
            ],
        },
    }

    /// Send a friendly greeting.  Doc-comment becomes the slash-command
    /// description in the auto-generated manifest entry.
    #[command(name = "greet")]
    fn greet(
        &self,
        #[option(description = "Who to greet")] name: String,
        #[option(description = "Shout it from the rooftops")] loud: Option<bool>,
    ) -> InteractionResponse {
        let body = if loud.unwrap_or(false) {
            format!("HELLO, {}!", name.to_uppercase())
        } else {
            format!("Hello, {name}!")
        };
        // Plain message + a "Greet again" button that re-opens the
        // modal flow.  The button's `custom_id` is taken from the
        // generated id table so it stays in lock-step with the
        // `#[component] fn on_greet_again` handler below.
        message!(
            body,
            row![
                Button::new(handler_id!(Self::on_greet_again), "Greet again")
                    .style(ButtonStyle::Primary)
            ],
        )
    }

    /// User clicked the "Greet again" button - open a modal that
    /// collects a custom greeting message.
    #[component]
    fn on_greet_again(&self) -> InteractionResponse {
        show_modal!(Self::on_greet_submit, "Send a custom greeting", {
            message: TextInput::label("Greeting message")
                .placeholder("Hi there!")
                .style(TextInputStyle::Paragraph)
                .max_length(280),
        })
    }

    /// User submitted the modal - emit a success toast with the body.
    #[modal]
    fn on_greet_submit(&self, #[field] message: String) -> InteractionResponse {
        toast!(format!("Greeting sent: {message}"), ToastLevel::Success)
    }

    fn on_load(&self, host: Host<'_>) -> PluginResult<()> {
        let config = Arc::new(GreeterConfig::from_lookup(|key| host.get_config(key)));
        store_state(
            &self.inner,
            RunningState {
                config,
                sessions: HashMap::new(),
            },
        );
        tracing::info!("{PLUGIN_NAME} v{PLUGIN_VERSION} loaded");
        ROk(())
    }

    fn on_unload(&self, _host: Host<'_>) -> PluginResult<()> {
        if take_state(&self.inner).is_some() {
            tracing::info!("{PLUGIN_NAME} unloaded");
        }
        ROk(())
    }

    fn on_client_connected(&self, host: Host<'_>, info: ClientInfo) -> PluginResult<()> {
        let username = info.username.as_str().to_owned();
        let snapshot = with_state_mut(&self.inner, |state| {
            let _ = state
                .sessions
                .entry(info.server_id)
                .or_default()
                .insert(info.session_id, username.clone());
            Arc::clone(&state.config)
        });
        let Some(config) = snapshot else {
            return ROk(());
        };
        send_greeting(host, &config, info.server_id, info.session_id, &username);
        ROk(())
    }

    fn on_client_disconnected(
        &self,
        _host: Host<'_>,
        server_id: ServerId,
        session: SessionId,
    ) -> PluginResult<()> {
        let _ = with_state_mut(&self.inner, |state| {
            let username = state
                .sessions
                .get_mut(&server_id)
                .and_then(|m| m.remove(&session));

            if let Some(name) = username {
                let farewell = config::expand_template(&state.config.farewell_template, &name);
                tracing::info!(server_id, session, "{farewell}");
            }
        });
        ROk(())
    }

    // `#[fancy_plugin]` injects a dispatch prelude in the generated
    // `MumblePlugin::on_plugin_message` wrapper that builds a `Host`
    // from the borrowed context and routes every `Interaction`
    // payload to the matching `#[command]` / `#[component]` /
    // `#[modal]` method above.  Only payload types those handlers
    // don't claim reach us here (e.g. `Ping`).
    fn on_plugin_message(&self, host: Host<'_>, msg: PluginMessageIn) -> PluginResult<()> {
        if msg.payload_type.as_str() == MSG_PING {
            let active = with_state(&self.inner, |state| {
                state.sessions.values().map(HashMap::len).sum::<usize>()
            })
            .unwrap_or(0);
            reply_pong(host, &msg, active);
        }
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

fn send_greeting(
    host: Host<'_>,
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
    if let Err(e) = host.send_to_sessions(server_id, &[session_id], MSG_GREETING, &bytes) {
        tracing::warn!(session = session_id, error = ?e, "fancy-greeter: send Greeting failed");
    }
}

fn reply_pong(host: Host<'_>, msg: &PluginMessageIn, active_sessions: usize) {
    let ping: PingPayload = match serde_json::from_slice(msg.payload.as_slice()) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(sender = msg.sender_session, "fancy-greeter: bad Ping: {e}");
            return;
        }
    };
    let bytes = match serde_json::to_vec(&PongPayload {
        nonce: ping.nonce,
        active_sessions,
    }) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("fancy-greeter: serialize Pong failed: {e}");
            return;
        }
    };
    if let Err(e) = host.reply_to(msg, MSG_PONG, &bytes) {
        tracing::warn!(error = ?e, "fancy-greeter: send Pong failed");
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
