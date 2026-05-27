//! Fancy Gallery Showcase - minimal example of the visual Tier-1
//! components: [`TextDisplay`], [`Thumbnail`], [`MediaGallery`], and
//! [`Container`].
//!
//! Exposes a single `/showcase` slash command.  The response is a
//! purely visual message: no buttons, no modals, no follow-up events.
//! Use it as a copy-paste starting point for any plugin that wants to
//! deliver image-heavy content (release notes, server welcome pages,
//! map previews, etc.).
//!
//! [`TextDisplay`]: mumble_plugin_api::TextDisplay
//! [`Thumbnail`]: mumble_plugin_api::Thumbnail
//! [`MediaGallery`]: mumble_plugin_api::MediaGallery
//! [`Container`]: mumble_plugin_api::Container

use mumble_plugin_api::{
    container, fancy_export_plugin, fancy_plugin, media_gallery, row, section, text_display,
    thumbnail, Component, InteractionResponse, MediaGalleryItem,
};

const PLUGIN_NAME: &str = "fancy-gallery-showcase";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Stateless plugin.
#[derive(Default)]
pub struct GalleryShowcase;

impl std::fmt::Debug for GalleryShowcase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GalleryShowcase").finish_non_exhaustive()
    }
}

#[fancy_plugin(name = PLUGIN_NAME, version = PLUGIN_VERSION)]
impl GalleryShowcase {
    plugin_info! {
        description: "Demo plugin: showcases TextDisplay, Thumbnail, MediaGallery and Container.",
        author: "Fancy Mumble",
        homepage: "https://github.com/Fancy-Mumble/fancy-plugin-example",
        tags: ["demo", "components", "gallery"],
        manifest: {
            capabilities: [SlashCommands, Components],
        },
    }

    /// Render a sampler of the visual components.
    #[command(name = "showcase")]
    fn showcase(&self) -> InteractionResponse {
        // A [`Section`] pairs flowing text with a single accessory
        // (here a [`Thumbnail`]).  Wrap the section in a
        // [`Container`] to tint the whole block with an accent
        // colour.
        let header: Component = container![
            section!(
                [
                    text_display!("# Fancy Mumble component showcase"),
                    text_display!("Sections place text next to a single accessory image or button."),
                ] => thumbnail!(
                    "https://placehold.co/96x96/png",
                    description = "Placeholder thumbnail",
                ),
            );
            accent_color = 0x0058_65F2,
        ]
        .into();

        // A [`MediaGallery`] renders 1-10 media items in a grid.
        let gallery: Component = media_gallery![
            MediaGalleryItem::new("https://placehold.co/320x180/png?text=One")
                .description("First tile"),
            MediaGalleryItem::new("https://placehold.co/320x180/png?text=Two")
                .description("Second tile"),
            MediaGalleryItem::new("https://placehold.co/320x180/png?text=Spoiler")
                .description("Click to reveal")
                .spoiler(true),
        ]
        .into();

        InteractionResponse::message("Visual components demo:")
            .row(row![header])
            .row(row![gallery])
    }
}

fancy_export_plugin!(GalleryShowcase::default);
