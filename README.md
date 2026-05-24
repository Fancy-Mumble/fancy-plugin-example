# fancy-greeter

An annotated, self-contained example of a **Fancy Mumble server plugin**.

Drop the compiled `.so` (or `.dll` / `.dylib`) into your server's
plugin directory, flip a single INI switch, restart the server, and the
plugin runs. No server recompile, no rebuild, no link edit.

Use this crate as a starting point for writing your own plugin. Every
public module and every non-obvious design decision is documented in
the source.

---

## What it does

| Feature | Detail |
|---------|--------|
| **Greeting** | Sends a configurable welcome message to every user immediately after they connect. |
| **Ping / Pong** | Responds to `Ping` plugin-messages from connected clients with a `Pong` that carries the current session count. |
| **HTTP status page** | Serves `GET /status` (default port `64741`) so operators can confirm the plugin is running and view live stats. |

---

## Architecture

Plugins are dynamically loaded shared libraries with an ABI-stable
boundary defined by the `mumble-plugin-api` crate. The host (`mumble-plugin-host`)
scans the configured plugin directory at startup, `dlopen`s every
library it finds, verifies the declared `abi_version` matches its own,
and instantiates the plugin through the factory function exported by
`fancy_export_plugin!`.

```
C++ Mumble server  (mumble-server)
       |
       | dlopen + statically-known C ABI
       v
mumble-plugin-host.so      (the loader; ships with the server)
       |
       | dlopen + abi_stable
       v
libfancy_greeter.so        <- this crate, dropped into /etc/mumble/plugins
libsome_other_plugin.so    <- any number of additional cdylib plugins
```

The boundary uses [`abi_stable`](https://docs.rs/abi_stable) FFI-safe
types (`RStr`, `RString`, `RVec`, `RArc`, `RResult`, ...) rather than
their `std` counterparts. As long as the host and the plugin agree on
`PLUGIN_ABI_VERSION` they can be built independently and even with
different Rust toolchains.

See the upstream docs at
[`server/plugins/overview`](https://docs.fancymumble.com/server/plugins/overview/)
and
[`server/plugins/developing`](https://docs.fancymumble.com/server/plugins/developing/)
for the full plugin model.

---

## Prerequisites

| Tool | Version |
|------|---------|
| Rust | 1.77 or newer |
| [Fancy Mumble server](https://github.com/Fancy-Mumble/mumble-server) | matching `PLUGIN_ABI_VERSION` (currently **2**) |

The plugin and the server must be built against the same major version
of `mumble-plugin-api`. The host refuses to load any cdylib whose
declared `abi_version` differs from its own.

---

## Build

```bash
# Produces a cdylib (.so on Linux, .dll on Windows, .dylib on macOS)
# alongside the rlib used by tests.
cargo build --release
```

After a release build you will find the artefact at:

* Linux:   `target/release/libfancy_greeter.so`
* macOS:   `target/release/libfancy_greeter.dylib`
* Windows: `target/release/fancy_greeter.dll`

---

## Install

Plugins are loaded from one or more directories at runtime; no server
recompile is required.

**Step 1 — copy the artefact** into the server's plugin directory:

```bash
sudo install -m 0644 target/release/libfancy_greeter.so \
    /etc/mumble/plugins/
```

The default search path is:

| Path | Purpose |
|------|---------|
| `/usr/lib/mumble-server/plugins` | Image-baked plugins shipped with the server package |
| `/etc/mumble/plugins` | Operator overlay - drop your own builds here |

Override the search path with either the `plugins_dir` INI key or the
`MUMBLE_PLUGIN_DIRS` environment variable (colon-separated on Unix,
semicolon-separated on Windows).

**Step 2 — enable the plugin** in `mumble-server.ini`:

```ini
plugin.fancy-greeter.enabled=true
```

The host reads `plugin.<name>.enabled` *before* invoking `on_load`. A
plugin whose `enabled` key is missing or falsy is loaded into memory
but never initialised, so disabling a plugin is a one-line config
change.

**Step 3 — restart** the Mumble server. Watch the log for the
`fancy-greeter v… loaded` line.

---

## Configuration

All keys live under the `plugin.fancy-greeter.*` namespace. The host
strips that prefix before calling `PluginContext::get_config`, so the
plugin sees short keys (`http_port`, `greeting_template`, ...).

```ini
; Enable the plugin (read by the host before on_load is called)
plugin.fancy-greeter.enabled=true

; Message sent to every new connection.
; {username} is replaced with the connecting user's display name.
plugin.fancy-greeter.greeting_template=Welcome to the server, {username}!

; Message logged server-side when a user disconnects.
plugin.fancy-greeter.farewell_template=Goodbye, {username}.

; TCP port for the HTTP status page (127.0.0.1 only).
plugin.fancy-greeter.http_port=64741
```

### Verify with curl

```bash
curl http://127.0.0.1:64741/status
```

Example response:

```json
{
  "plugin": "fancy-greeter",
  "active_sessions": 3,
  "greeting_template": "Welcome to the server, {username}!"
}
```

---

## Client integration (TypeScript / FancyMumble)

### Send a Ping

```ts
import { invoke } from "@tauri-apps/api/core";

const nonce = crypto.randomUUID();
const payload = new TextEncoder().encode(JSON.stringify({ nonce }));

await invoke("send_plugin_message", {
  pluginName: "fancy-greeter",
  payloadType: "Ping",
  payload: Array.from(payload),
  targetSessions: [],   // empty = route to self; server echoes Pong back
  channelId: null,
});
```

### Listen for inbound messages

```ts
import { listen } from "@tauri-apps/api/event";

await listen<{
  plugin_name: string;
  payload_type: string;
  payload: number[];
}>("plugin-message", (event) => {
  if (event.payload.plugin_name !== "fancy-greeter") return;

  const bytes = new Uint8Array(event.payload.payload);
  const json = JSON.parse(new TextDecoder().decode(bytes));

  switch (event.payload.payload_type) {
    case "Greeting":
      console.log("Server says:", json.message);
      break;
    case "Pong":
      console.log(`Pong! nonce=${json.nonce} sessions=${json.active_sessions}`);
      break;
  }
});
```

---

## Message types

| `payload_type` | Direction | Payload shape |
|----------------|-----------|---------------|
| `Ping` | Client to Plugin | `{ "nonce": string }` |
| `Pong` | Plugin to Client | `{ "nonce": string, "active_sessions": number }` |
| `Greeting` | Plugin to Client | `{ "message": string }` |

Inbound messages reach the plugin through two callbacks:

* **`on_plugin_message`** (wire ID 200): the new generic
  `PluginMessage` envelope. Each envelope is routed by `plugin_name`
  to exactly one plugin.
* **`on_plugin_data`** (legacy `PluginDataTransmission`): broadcast
  flavour where every plugin sees every message.

New plugins should prefer `on_plugin_message`. This example handles
both so it works against older Fancy Mumble clients as well.

---

## Source layout

```
src/
  lib.rs    - Plugin struct, MumblePlugin impl, fancy_export_plugin!
  config.rs - INI config parsing and template expansion
  server.rs - axum HTTP status page
  types.rs  - Wire payload structs and MSG_* constants
```

---

## Extending this plugin

The `MumblePlugin` trait exposes the following hooks. Override whichever
ones your plugin needs; the rest default to no-ops.

| Hook | When it fires |
|------|---------------|
| `on_load(ctx)` | After the host loads the cdylib. Parse config and start services here. |
| `on_unload()` | Before the host unloads the cdylib. Shut down services here. |
| `on_client_connected(info)` | A user authenticated and joined a virtual server. |
| `on_client_disconnected(server_id, session)` | A user disconnected. |
| `on_plugin_message(msg)` | A client sent a `PluginMessage` addressed to this plugin. |
| `on_plugin_data(server_id, sender, data_id, data)` | A client sent raw `PluginDataTransmission` bytes. |

### Adding a new message type

1. Add a constant to `src/types.rs`:
   ```rust
   pub const MSG_MY_EVENT: &str = "MyEvent";
   ```
2. Add the payload struct:
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct MyEventPayload { pub detail: String }
   ```
3. Handle it in `on_plugin_message` in `src/lib.rs`:
   ```rust
   if msg.payload_type.as_str() == MSG_MY_EVENT {
       handle_my_event(&state.ctx, &msg);
   }
   ```

---

## Async model

Trait methods on `MumblePlugin` are **synchronous** across the FFI
boundary. The host calls them directly without a runtime in scope.
Each plugin therefore owns its own private `tokio` runtime, created in
`on_load` and dropped in `on_unload`. The HTTP status server runs on
that runtime; the sync hooks call back into the host via the (sync)
`PluginContext` trait object.

---

## License

MIT - see [LICENSE](LICENSE).
