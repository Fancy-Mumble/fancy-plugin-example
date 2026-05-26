# fancy-feedback-form

Minimal Fancy Mumble plugin showing a multi-field modal form.

- `/feedback` opens a modal with a `Short` "Subject" input and a
  `Paragraph` "Details" input.
- The `#[modal]` handler receives both as plain `String`s, logs them,
  and emits a success toast.

Modals currently accept `TextInput` fields only; demonstrating
`RadioGroup` / `CheckboxGroup` lives in a chat-style message and is
covered by the other examples.  See [`src/lib.rs`](src/lib.rs).

## Build

```bash
cargo build --release -p fancy-feedback-form
```

## Install

Drop the produced `cdylib` into your Mumble plugin directory and copy
[`plugin.example.ini`](plugin.example.ini) into `mumble-server.ini`.
