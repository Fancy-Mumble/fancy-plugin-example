# fancy-info-card

Minimal Fancy Mumble plugin demonstrating the `Section` + `Button`
accessory + follow-up `Toast` pattern.

- `/info` returns a rich-text card with a `Section` whose accessory is
  an "Acknowledge" button.
- Clicking the button dispatches to a `#[component]`-tagged handler
  that emits a success toast.

The plugin is fully stateless.  See [`src/lib.rs`](src/lib.rs).

## Build

```bash
cargo build --release -p fancy-info-card
```

## Install

Drop the produced `cdylib` into your Mumble plugin directory and copy
[`plugin.example.ini`](plugin.example.ini) into `mumble-server.ini`.
