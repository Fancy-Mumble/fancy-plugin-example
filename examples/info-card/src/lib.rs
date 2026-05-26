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

use std::sync::Mutex;

use abi_stable::std_types::{RArc, ROk};
use mumble_plugin_api::{
    fancy_export_plugin, fancy_plugin, handler_id, row, section, text_display, toast, Button,
    ButtonStyle, Component, InteractionResponse, MumblePlugin, PluginContext_TO, PluginResult,
    ToastLevel,
};

const PLUGIN_NAME: &str = "fancy-info-card";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Stateless apart from the host context kept around so the
/// `#[fancy_plugin]`-generated dispatcher can reach the originating
/// client.
#[derive(Default)]
pub struct InfoCard {
    ctx: Mutex<Option<PluginContext_TO<RArc<()>>>>,
}

impl std::fmt::Debug for InfoCard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InfoCard").finish_non_exhaustive()
    }
}

impl InfoCard {
    /// Run `f` with a borrow of the host context.  Returns `None`
    /// outside the [`on_load`](Self::on_load) / [`on_unload`](Self::on_unload)
    /// window.
    fn with_ctx<R>(&self, f: impl FnOnce(&PluginContext_TO<RArc<()>>) -> R) -> Option<R> {
        self.ctx.lock().ok()?.as_ref().map(f)
    }
}

#[fancy_plugin(name = PLUGIN_NAME, version = PLUGIN_VERSION)]
impl MumblePlugin for InfoCard {
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

    fn on_load(&self, ctx: PluginContext_TO<RArc<()>>) -> PluginResult<()> {
        if let Ok(mut slot) = self.ctx.lock() {
            *slot = Some(ctx);
        }
        ROk(())
    }

    fn on_unload(&self) -> PluginResult<()> {
        if let Ok(mut slot) = self.ctx.lock() {
            *slot = None;
        }
        ROk(())
    }
}

fancy_export_plugin!(InfoCard::default);
