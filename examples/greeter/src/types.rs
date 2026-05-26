//! Wire-level message types shared between the server plugin and the Fancy
//! Mumble client.
//!
//! Both sides agree on the `payload_type` strings (the `MSG_*` constants) and
//! on the JSON shape of each payload.  JSON is used here for readability;
//! you can substitute protobuf or `MessagePack` if you prefer binary encoding.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// payload_type constants
// ---------------------------------------------------------------------------

/// Sent by the client to check whether the plugin is alive.
pub const MSG_PING: &str = "Ping";

/// Reply sent by the plugin in response to a [`PingPayload`].
pub const MSG_PONG: &str = "Pong";

/// Sent by the plugin to a user immediately after they connect.
pub const MSG_GREETING: &str = "Greeting";

// ---------------------------------------------------------------------------
// Ping / Pong
// ---------------------------------------------------------------------------

/// Client sends this to probe plugin liveness and retrieve live stats.
///
/// # Client example (TypeScript)
///
/// ```ts
/// const payload = new TextEncoder().encode(
///   JSON.stringify({ nonce: crypto.randomUUID() })
/// );
/// await invoke("send_plugin_message", {
///   pluginName: "fancy-greeter",
///   payloadType: "Ping",
///   payload: Array.from(payload),
///   targetSessions: [],   // empty = deliver to self (the server echoes back)
///   channelId: null,
/// });
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingPayload {
    /// Arbitrary string that the plugin echoes back unchanged in the Pong.
    ///
    /// Clients can use this for latency measurement or to correlate
    /// concurrent in-flight pings.
    pub nonce: String,
}

/// Plugin sends this in reply to every [`PingPayload`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PongPayload {
    /// Echoed from the originating [`PingPayload::nonce`].
    pub nonce: String,
    /// Current number of tracked sessions across all virtual servers.
    pub active_sessions: usize,
}

// ---------------------------------------------------------------------------
// Greeting
// ---------------------------------------------------------------------------

/// Server-to-client message pushed to every user right after they connect.
///
/// # Client-side reception (TypeScript)
///
/// ```ts
/// import { listen } from "@tauri-apps/api/event";
///
/// await listen<{
///   plugin_name: string;
///   payload_type: string;
///   payload: number[];
/// }>("plugin-message", (e) => {
///   if (e.payload.plugin_name !== "fancy-greeter") return;
///   if (e.payload.payload_type !== "Greeting") return;
///   const json = JSON.parse(
///     new TextDecoder().decode(new Uint8Array(e.payload.payload))
///   );
///   console.log("Server greeting:", json.message);
/// });
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GreetingPayload {
    /// Fully expanded greeting text ready to display to the user.
    pub message: String,
}
