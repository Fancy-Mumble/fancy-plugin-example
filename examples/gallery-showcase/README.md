# fancy-gallery-showcase

Minimal Fancy Mumble plugin that exposes a single `/showcase` slash
command and renders a visual sampler of the Tier-1 layout components:

- [`Container`](https://docs.rs/mumble-plugin-api/latest/mumble_plugin_api/struct.Container.html)
  with accent colour
- [`Section`](https://docs.rs/mumble-plugin-api/latest/mumble_plugin_api/struct.Section.html)
  with two [`TextDisplay`](https://docs.rs/mumble-plugin-api/latest/mumble_plugin_api/struct.TextDisplay.html)
  children and a [`Thumbnail`](https://docs.rs/mumble-plugin-api/latest/mumble_plugin_api/struct.Thumbnail.html)
  accessory
- [`MediaGallery`](https://docs.rs/mumble-plugin-api/latest/mumble_plugin_api/struct.MediaGallery.html)
  with three items (one spoiler)

The plugin holds no state and registers no message handlers.  Copy
[`src/lib.rs`](src/lib.rs) as a starting point for any plugin that
needs to push rich, read-only content (release notes, server welcome
pages, map previews, ...).

## Build

```bash
# From the workspace root.
cargo build --release -p fancy-gallery-showcase
```

## Install

Copy the produced `cdylib`
(`target/release/libfancy_gallery_showcase.{so,dylib}` or
`fancy_gallery_showcase.dll`) into the Mumble plugin directory and
append [`plugin.example.ini`](plugin.example.ini) to your
`mumble-server.ini`.
