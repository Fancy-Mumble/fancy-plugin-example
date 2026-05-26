//! Fancy Feedback Form - minimal example of a multi-field [`TextInput`]
//! modal.  Exposes a single `/feedback` slash command that opens a
//! modal collecting a subject (short input) and a body (paragraph
//! input); submitting the modal echoes back a success toast.
//!
//! Modals currently support [`TextInput`] fields only (`Short` and
//! `Paragraph` styles).  Other component kinds - [`RadioGroup`],
//! [`CheckboxGroup`], selects - can be rendered inside a chat-style
//! message; see the `info-card` and `gallery-showcase` examples.
//!
//! [`TextInput`]: mumble_plugin_api::TextInput
//! [`RadioGroup`]: mumble_plugin_api::RadioGroup
//! [`CheckboxGroup`]: mumble_plugin_api::CheckboxGroup

use std::sync::Mutex;

use abi_stable::std_types::{RArc, ROk};
use mumble_plugin_api::{
    fancy_export_plugin, fancy_plugin, show_modal, toast, InteractionResponse, MumblePlugin,
    PluginContext_TO, PluginResult, TextInput, TextInputStyle, ToastLevel,
};

const PLUGIN_NAME: &str = "fancy-feedback-form";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Stateless apart from the host context kept around so the
/// `#[fancy_plugin]`-generated dispatcher can reach the originating
/// client.
#[derive(Default)]
pub struct FeedbackForm {
    ctx: Mutex<Option<PluginContext_TO<RArc<()>>>>,
}

impl std::fmt::Debug for FeedbackForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FeedbackForm").finish_non_exhaustive()
    }
}

impl FeedbackForm {
    /// Run `f` with a borrow of the host context.  Returns `None`
    /// outside the [`on_load`](Self::on_load) / [`on_unload`](Self::on_unload)
    /// window.
    fn with_ctx<R>(&self, f: impl FnOnce(&PluginContext_TO<RArc<()>>) -> R) -> Option<R> {
        self.ctx.lock().ok()?.as_ref().map(f)
    }
}

#[fancy_plugin(name = PLUGIN_NAME, version = PLUGIN_VERSION)]
impl MumblePlugin for FeedbackForm {
    plugin_info! {
        description: "Demo plugin: opens a multi-field feedback modal and toasts on submit.",
        author: "Fancy Mumble",
        homepage: "https://github.com/Fancy-Mumble/fancy-plugin-example",
        tags: ["demo", "modal", "feedback"],
        manifest: {
            capabilities: [SlashCommands, Modals, Notifications],
        },
    }

    /// Open the feedback modal.
    #[command(name = "feedback")]
    fn feedback(&self) -> InteractionResponse {
        // [`show_modal!`] wires the modal's `custom_id` and per-field
        // ids to the [`on_submit`](Self::on_submit) handler, so renaming
        // either side is a one-shot rename.
        show_modal!(Self::on_submit, "Send feedback", {
            subject: TextInput::label("Subject")
                .placeholder("A short summary")
                .max_length(100)
                .required(true),
            body: TextInput::label("Details")
                .placeholder("What happened?")
                .style(TextInputStyle::Paragraph)
                .max_length(1000)
                .required(true),
        })
    }

    /// Receive the modal submission.  Each `#[field]` parameter is
    /// extracted by name from the wire payload and converted via
    /// [`FromField`](mumble_plugin_api::FromField).  `String` is
    /// required; `Option<String>` is, well, optional.
    #[modal]
    fn on_submit(&self, #[field] subject: String, #[field] body: String) -> InteractionResponse {
        toast!(
            format!("Thanks - logged \"{subject}\" ({} chars).", body.len()),
            ToastLevel::Success,
        )
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

fancy_export_plugin!(FeedbackForm::default);
