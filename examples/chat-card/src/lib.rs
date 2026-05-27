//! Fancy Chat Card - minimal example demonstrating
//! [`chat_message!`](mumble_plugin_api::chat_message), the response
//! kind that injects a *literal* chat message into the channel/DM
//! history (rather than a transient floating overlay).
//!
//! Exposes a single `/chat-card` slash command.  The reply is a chat
//! bubble authored by this plugin, carrying a Markdown body and a
//! single [`Button`] action row inline.  Clicking the button fires a
//! [`Toast`] so you can verify that component routing works the same
//! way it does for [`message!`]-style responses.
//!
//! Demonstrates:
//! - [`chat_message!`] vs `message!`: identical surface, different
//!   rendering pipeline on the client (persisted chat bubble vs
//!   floating card).
//! - The id-routing pattern: [`handler_id!`] keeps the button's
//!   `custom_id` in lock-step with the [`component`]-tagged handler.
//!
//! [`Button`]: mumble_plugin_api::Button
//! [`Toast`]: mumble_plugin_api::ResponseKind::Toast

use mumble_plugin_api::{
    chat_message, fancy_export_plugin, fancy_plugin, handler_id, row, toast, Button, ButtonStyle,
    InteractionResponse, ToastLevel,
};

const PLUGIN_NAME: &str = "fancy-chat-card";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Stateless plugin - the host owns the [`PluginContext`], and the
/// generated dispatcher hands a [`Host`](mumble_plugin_api::Host)
/// to every handler that asks for one.
#[derive(Default)]
pub struct ChatCard;

impl std::fmt::Debug for ChatCard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatCard").finish_non_exhaustive()
    }
}

#[fancy_plugin(name = PLUGIN_NAME, version = PLUGIN_VERSION)]
impl ChatCard {
    plugin_info! {
        description: "Demo plugin: posts a literal chat message carrying a Button row inline.",
        author: "Fancy Mumble",
        homepage: "https://github.com/Fancy-Mumble/fancy-plugin-example",
        tags: ["demo", "components", "chat-message"],
        manifest: {
            capabilities: [SlashCommands, Components, Notifications],
        },
    }

    /// Post a chat-message-style card into the originating chat tab.
    ///
    /// `chat_message!` returns an [`InteractionResponse`] that the
    /// client renders as a real chat bubble - it shows up in scroll,
    /// counts toward unreads, and can be quoted / pinned like any
    /// user message.  The attached row is rendered inside the same
    /// bubble below the body.
    #[command(name = "chat-card")]
    fn chat_card(&self) -> InteractionResponse {
        chat_message!(
            "**Hello from a plugin!**\n\
             This bubble is a real chat message authored by `fancy-chat-card`. \
             The button below is routed back to the plugin just like any \
             component on a floating card.",
            row![Button::new(handler_id!(Self::on_click), "Click me").style(ButtonStyle::Primary)],
        )
    }

    /// Inline button click - reply with a success toast.
    #[component]
    fn on_click(&self) -> InteractionResponse {
        toast!("Button inside the chat bubble works!", ToastLevel::Success)
    }
}

fn _assert_send_sync<T: Send + Sync>() {}
const _: fn() = || _assert_send_sync::<ChatCard>();

fancy_export_plugin!(ChatCard::default);
