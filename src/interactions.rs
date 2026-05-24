//! Tier-1 client extension demo for fancy-greeter.
//!
//! Exposes a single `/greet <name> [loud?]` slash command that replies
//! with a chat-style message carrying a "Greet again" button.  Clicking
//! the button opens a modal with a free-form text field; submitting the
//! modal triggers a toast.  The whole flow exercises every Tier-1
//! affordance (slash command, component, modal, response variants)
//! without any frontend-specific JavaScript.

use abi_stable::std_types::{ROption, RString, RVec};
use mumble_plugin_api::{
    ActionRow, Button, ButtonStyle, Capability, ClientManifest, Component, Interaction,
    InteractionKind, InteractionResponse, OptionType, OptionValue, PluginContext_TO,
    PluginMessageIn, PluginMessageOut, ResponseKind, SessionId, SlashCommand, SlashCommandOption,
    TextInput, TextInputStyle, ToastLevel, INTERACTION_RESPONSE_PAYLOAD_TYPE,
};

use crate::PLUGIN_NAME;

/// Slash command name (without the leading `/`).
const CMD_GREET: &str = "greet";

/// `custom_id` for the "Greet again" button.
const COMPONENT_GREET_AGAIN: &str = "greet:again";

/// `custom_id` for the modal opened by the button.
const MODAL_GREET: &str = "greet:modal";

/// `custom_id` of the single text field inside [`MODAL_GREET`].
const MODAL_FIELD_MESSAGE: &str = "message";

/// Manifest published in [`crate::GreeterPlugin::info_json`].
pub fn build_manifest() -> ClientManifest {
    ClientManifest {
        slash_commands: vec![SlashCommand {
            name: CMD_GREET.into(),
            description: "Send a friendly greeting".into(),
            options: vec![
                SlashCommandOption {
                    name: "name".into(),
                    description: "Who to greet".into(),
                    option_type: OptionType::String,
                    required: true,
                    choices: vec![],
                },
                SlashCommandOption {
                    name: "loud".into(),
                    description: "Shout it from the rooftops".into(),
                    option_type: OptionType::Boolean,
                    required: false,
                    choices: vec![],
                },
            ],
        }],
        capabilities: vec![
            Capability::SlashCommands,
            Capability::Components,
            Capability::Modals,
            Capability::Notifications,
        ],
        settings_panels: vec![],
        ..ClientManifest::default()
    }
}

/// Handle an inbound [`Interaction`] envelope.  Returns `Some(response)`
/// for the plugin host to ship back to the originating client, or
/// `None` if the envelope is not addressed to this plugin's flow.
pub fn handle_interaction(interaction: Interaction) -> Option<InteractionResponse> {
    match interaction.kind {
        InteractionKind::SlashCommand { name, options } if name == CMD_GREET => Some(
            greet_response(&interaction.correlation_id, &options),
        ),
        InteractionKind::Component { custom_id, .. } if custom_id == COMPONENT_GREET_AGAIN => {
            Some(open_greet_modal(&interaction.correlation_id))
        }
        InteractionKind::ModalSubmit { custom_id, values } if custom_id == MODAL_GREET => Some(
            modal_submit_response(&interaction.correlation_id, &values),
        ),
        _ => None,
    }
}

fn greet_response(
    correlation_id: &str,
    options: &std::collections::BTreeMap<String, OptionValue>,
) -> InteractionResponse {
    let name = options
        .get("name")
        .and_then(|v| match v {
            OptionValue::String(s) => Some(s.as_str()),
            _ => None,
        })
        .unwrap_or("stranger");
    let loud = matches!(options.get("loud"), Some(OptionValue::Boolean(true)));
    let body = if loud {
        format!("HELLO, {}!", name.to_uppercase())
    } else {
        format!("Hello, {name}!")
    };
    InteractionResponse {
        correlation_id: Some(correlation_id.to_owned()),
        kind: ResponseKind::Message {
            message_id: format!("greet-{correlation_id}"),
            content: body,
            components: vec![ActionRow {
                components: vec![Component::Button(Button {
                    custom_id: COMPONENT_GREET_AGAIN.into(),
                    label: "Greet again".into(),
                    style: ButtonStyle::Primary,
                    disabled: false,
                })],
            }],
            ephemeral: false,
        },
    }
}

fn open_greet_modal(correlation_id: &str) -> InteractionResponse {
    InteractionResponse {
        correlation_id: Some(correlation_id.to_owned()),
        kind: ResponseKind::ShowModal {
            custom_id: MODAL_GREET.into(),
            title: "Send a custom greeting".into(),
            components: vec![ActionRow {
                components: vec![Component::TextInput(TextInput {
                    custom_id: MODAL_FIELD_MESSAGE.into(),
                    label: "Greeting message".into(),
                    value: None,
                    placeholder: Some("Hi there!".into()),
                    style: TextInputStyle::Paragraph,
                    required: true,
                    max_length: 280,
                })],
            }],
        },
    }
}

fn modal_submit_response(
    correlation_id: &str,
    values: &std::collections::BTreeMap<String, String>,
) -> InteractionResponse {
    let message = values
        .get(MODAL_FIELD_MESSAGE)
        .map_or("(empty)", String::as_str);
    InteractionResponse {
        correlation_id: Some(correlation_id.to_owned()),
        kind: ResponseKind::Toast {
            message: format!("Greeting sent: {message}"),
            level: ToastLevel::Success,
        },
    }
}

/// Decode the inbound `Interaction` payload bytes, dispatch through
/// [`handle_interaction`], and ship the response back to the sender.
pub fn dispatch(ctx: &PluginContext_TO<abi_stable::std_types::RArc<()>>, msg: &PluginMessageIn) {
    let interaction: Interaction = match serde_json::from_slice(msg.payload.as_slice()) {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!(error = %e, "fancy-greeter: malformed Interaction payload");
            return;
        }
    };
    let Some(response) = handle_interaction(interaction) else {
        return;
    };
    let bytes = match serde_json::to_vec(&response) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "fancy-greeter: serialize InteractionResponse failed");
            return;
        }
    };
    let out = PluginMessageOut {
        server_id: msg.server_id,
        plugin_name: RString::from(PLUGIN_NAME),
        payload_type: RString::from(INTERACTION_RESPONSE_PAYLOAD_TYPE),
        payload: RVec::from(bytes),
        target_sessions: target_only(msg.sender_session),
        channel_id: ROption::RNone,
    };
    if let abi_stable::std_types::RResult::RErr(e) = ctx.send_plugin_message(out) {
        tracing::warn!(error = ?e, "fancy-greeter: send InteractionResponse failed");
    }
}

fn target_only(session: SessionId) -> RVec<SessionId> {
    let mut v: RVec<SessionId> = RVec::new();
    v.push(session);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_command_produces_message_with_button() {
        let interaction = Interaction {
            correlation_id: "c1".into(),
            channel_id: Some(7),
            kind: InteractionKind::SlashCommand {
                name: CMD_GREET.into(),
                options: [("name".into(), OptionValue::String("Alice".into()))]
                    .into_iter()
                    .collect(),
            },
        };
        let response = handle_interaction(interaction).expect("response");
        match response.kind {
            ResponseKind::Message {
                content,
                components,
                ..
            } => {
                assert!(content.contains("Alice"));
                assert_eq!(components.len(), 1);
                assert_eq!(components[0].components.len(), 1);
            }
            _ => panic!("expected Message"),
        }
    }

    #[test]
    fn loud_flag_uppercases_greeting() {
        let interaction = Interaction {
            correlation_id: "c2".into(),
            channel_id: None,
            kind: InteractionKind::SlashCommand {
                name: CMD_GREET.into(),
                options: [
                    ("name".into(), OptionValue::String("bob".into())),
                    ("loud".into(), OptionValue::Boolean(true)),
                ]
                .into_iter()
                .collect(),
            },
        };
        let response = handle_interaction(interaction).expect("response");
        match response.kind {
            ResponseKind::Message { content, .. } => assert!(content.contains("BOB")),
            _ => panic!("expected Message"),
        }
    }

    #[test]
    fn button_click_opens_modal() {
        let interaction = Interaction {
            correlation_id: "c3".into(),
            channel_id: None,
            kind: InteractionKind::Component {
                custom_id: COMPONENT_GREET_AGAIN.into(),
                values: vec![],
            },
        };
        let response = handle_interaction(interaction).expect("response");
        assert!(matches!(response.kind, ResponseKind::ShowModal { .. }));
    }

    #[test]
    fn modal_submit_returns_toast() {
        let interaction = Interaction {
            correlation_id: "c4".into(),
            channel_id: None,
            kind: InteractionKind::ModalSubmit {
                custom_id: MODAL_GREET.into(),
                values: [(MODAL_FIELD_MESSAGE.to_owned(), "Hey".to_owned())]
                    .into_iter()
                    .collect(),
            },
        };
        let response = handle_interaction(interaction).expect("response");
        match response.kind {
            ResponseKind::Toast { message, level } => {
                assert!(message.contains("Hey"));
                assert_eq!(level, ToastLevel::Success);
            }
            _ => panic!("expected Toast"),
        }
    }

    #[test]
    fn unrelated_interaction_returns_none() {
        let interaction = Interaction {
            correlation_id: "c5".into(),
            channel_id: None,
            kind: InteractionKind::SlashCommand {
                name: "other".into(),
                options: Default::default(),
            },
        };
        assert!(handle_interaction(interaction).is_none());
    }
}
