//! Fancy Info Card - minimal example that pairs a [`Section`] with a
//! [`Button`] accessory and answers the click with a [`Toast`].
//!
//! Exposes a single `/info` slash command that returns a rich-text
//! card; clicking the "Ack" button on the card fires a follow-up toast.
//!
//! Demonstrates:
//! - [`Section`] with text children and a [`Button`] accessory.
//! - The id-routing pattern: [`handler_id!`] keeps the button's
//!   `custom_id` in lock-step with the [`component`]-tagged handler.
//! - [`Toast`] responses with explicit [`ToastLevel`].
//!
//! [`Section`]: mumble_plugin_api::Section
//! [`Button`]: mumble_plugin_api::Button
//! [`Toast`]: mumble_plugin_api::ResponseKind::Toast
//! [`ToastLevel`]: mumble_plugin_api::ToastLevel

use mumble_plugin_api::{
    fancy_export_plugin, fancy_plugin, handler_id, row, section, text_display, toast, Button,
    ButtonStyle, Component, InteractionResponse, ToastLevel,
};

const PLUGIN_NAME: &str = "fancy-info-card";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Stateless plugin.
#[derive(Default)]
pub struct InfoCard;

impl std::fmt::Debug for InfoCard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InfoCard").finish_non_exhaustive()
    }
}

#[fancy_plugin(name = PLUGIN_NAME, version = PLUGIN_VERSION)]
impl InfoCard {
    plugin_info! {
        description: "Demo plugin: shows a Section card with a Button accessory and a Toast follow-up.",
        author: "Fancy Mumble",
        homepage: "https://github.com/Fancy-Mumble/fancy-plugin-example",
        tags: ["demo", "components", "toast"],
        manifest: {
            capabilities: [SlashCommands, Components, Notifications],
        },
    }

    /// Render the info card.
    #[command(name = "info")]
    fn info(&self) -> InteractionResponse {
        let card: Component = section!(
            [
                text_display!("# Server health"),
                text_display!("Latency, uptime and channel counts go here."),
            ] => Button::new(handler_id!(Self::on_ack), "Acknowledge")
                .style(ButtonStyle::Success),
        )
        .into();

        InteractionResponse::message("Status:").row(row![card])
    }

    /// Acknowledge button click - reply with a success toast.
    #[component]
    fn on_ack(&self) -> InteractionResponse {
        toast!("Acknowledged - thanks!", ToastLevel::Success)
    }
}

fancy_export_plugin!(InfoCard::default);
