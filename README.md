# fancy-plugin-example

A Cargo workspace of small, annotated [Fancy Mumble][fancy] server
plugins.  Each member crate compiles to a self-contained `cdylib`
that you drop into `/etc/mumble/plugins` and enable with a single INI
switch — no server recompile, no rebuild, no link edit.

[fancy]: https://github.com/Fancy-Mumble/mumble-server

## Layout

```
fancy-plugin-example/
├── Cargo.toml                # workspace root (lints, shared deps)
├── rust-toolchain.toml       # pins the Rust toolchain
└── examples/
    ├── greeter/              # slash command + button + modal + plugin messages
    ├── gallery-showcase/     # Container + Section + Thumbnail + MediaGallery
    ├── info-card/            # Section + Button accessory + Toast follow-up
    └── feedback-form/        # multi-field TextInput modal
```

Each example is intentionally small.  When in doubt, add a new
example rather than piling features onto an existing one.

| Crate                    | Slash command | Demonstrates                                                              |
| ------------------------ | ------------- | ------------------------------------------------------------------------- |
| `fancy-greeter`          | `/greet`      | Slash command, button, modal, toast, plugin messages, persistent config   |
| `fancy-gallery-showcase` | `/showcase`   | `TextDisplay`, `Thumbnail`, `MediaGallery`, `Container` (visual-only)     |
| `fancy-info-card`        | `/info`       | `Section` with a `Button` accessory, `#[component]` handler, `Toast`      |
| `fancy-feedback-form`    | `/feedback`   | Multi-field `TextInput` modal, `#[modal]` handler                         |

## Prerequisites

| Tool | Version |
| ---- | ------- |
| Rust | as pinned by `rust-toolchain.toml` (currently `1.95.0`) |
| [Fancy Mumble server](https://github.com/Fancy-Mumble/mumble-server) | matching `PLUGIN_ABI_VERSION` (currently **2**) |

The `mumble-plugin-api` path dependency expects the `mumble-server`
checkout to live as a sibling of this repo:

```
<workspace>/
├── fancy-plugin-example/   # this repo
└── mumble-server/          # https://github.com/Fancy-Mumble/mumble-server
```

The CI workflow recreates that layout automatically (see
[`.github/workflows/ci.yml`](.github/workflows/ci.yml)).

## Build

```bash
# Build every example.
cargo build --release

# Or just one.
cargo build --release -p fancy-info-card
```

Each crate emits the cdylib at `target/release/lib<name>.{so,dylib}`
(or `<name>.dll` on Windows).

## Test & lint

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test  --workspace
```

The lint policy is defined once in the root `[workspace.lints]` block
and inherited by every member via `[lints] workspace = true`.

## CI artefacts

The workflow runs the lint/test gate once for the whole workspace,
then fans out to a (`example` × `os`) matrix.  Every job uploads a
single drop-in archive named
`<crate>-<os>-<arch>.{tar.gz|zip}` containing:

- the platform-appropriate `cdylib`
- the matching `plugin.example.ini`
- the per-crate `README.md`

Tagging `vX.Y.Z` additionally attaches every archive to a GitHub
Release.

## Writing your own plugin

Copy whichever example is closest to what you want, rename the crate,
and add it to the workspace `members` list in the root `Cargo.toml`.
Update the matrix in `.github/workflows/ci.yml` to ship a build of
your new plugin.
