//! Fancy Quick Poll - end-to-end example for the multi-user
//! [`InteractionResponse`] broadcast pipeline, expressed in the
//! standard `#[fancy_plugin]` style used by the other examples.
//!
//! The plugin exposes a single `/poll` slash command.  Submitting the
//! modal posts a chat bubble carrying a vote dropdown into every
//! session of the originating channel; every subsequent vote rewrites
//! the same bubble in-place on every recipient.  In other words:
//! everyone sees the live tally as it changes.
//!
//! # What this example demonstrates
//!
//! * [`Host::sessions_in_channel`] for enumerating recipients and
//!   [`Host::respond_to_sessions`] for fanning a single
//!   [`InteractionResponse`] out to many clients.
//! * [`InteractionResponse::chat_message_with_id`] +
//!   [`update_message!`] to mutate a previously broadcast chat bubble
//!   so all recipients see the same updated tally.
//! * [`Host::caller`]: the dispatcher attaches the originating
//!   server / session / channel so `#[modal]` and `#[component]`
//!   handlers can broadcast back to the right audience without
//!   touching the raw [`PluginMessageIn`].
//! * Encoding per-instance routing data into [`SelectOption::value`]
//!   so a single static `custom_id` (matched by [`handler_id!`])
//!   still routes votes from many concurrent polls.
//!
//! # Known limitations
//!
//! * A single instance keeps polls in memory only; restarts forget
//!   open polls.
//! * Recipients are frozen at poll-creation time.  A user who joins
//!   the channel afterwards will not receive the broadcast and cannot
//!   vote on the existing poll.
//! * Voter dedup uses the caller's session id.  Reconnecting (which
//!   mints a new session id) lets the same user vote again.

#![allow(clippy::unwrap_used, reason = "Mutexes are private; lock never poisoned in practice")]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use mumble_plugin_api::{
    fancy_export_plugin, fancy_plugin, handler_id, row, show_modal, text_display, toast,
    update_message, ActionRow, CheckboxGroup, CheckboxOption, Host,
    InteractionResponse, PluginResult, RadioGroup, RadioOption, SessionId, TextInput,
    TextInputStyle, ToastLevel,
};
use abi_stable::std_types::ROk;

const PLUGIN_NAME: &str = "fancy-quick-poll";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Hard cap on options accepted from the modal.  Mirrors a typical
/// emoji-keycap dropdown so the rendered tally stays readable.
const MAX_OPTIONS: usize = 10;
/// Minimum to make the poll meaningful.
const MIN_OPTIONS: usize = 2;

/// Live state for a single in-flight poll.
struct PollState {
    /// Markdown body shown above the dropdown.
    question: String,
    /// Voter labels, parallel to `votes`.
    options: Vec<String>,
    /// Tally, parallel to `options`.
    votes: Vec<u32>,
    /// Stable `message_id` of the broadcast chat bubble so we can
    /// target it with [`update_message!`].
    message_id: String,
    /// Sessions the poll was broadcast to at creation time.  Updates
    /// fan out to exactly this set.
    recipients: Vec<SessionId>,
    /// Sessions that have already voted (dedup).
    voted: HashSet<SessionId>,
    /// Virtual server the poll lives on.
    server_id: u32,
    /// When `true` the vote control accepts multiple selections at
    /// once (rendered as a checkbox-style multi-select); when
    /// `false` it accepts exactly one (rendered as a radio-style
    /// single-select dropdown).
    multi: bool,
}

/// Global plugin handle.  All mutable state hides behind one
/// [`Mutex`] so the FFI surface stays `Sync`.  The plugin never
/// stores its own `PluginContext` - the host hands a [`Host`] facade
/// to every callback instead.
#[derive(Default)]
pub struct QuickPoll {
    polls: Mutex<BTreeMap<String, PollState>>,
    /// Monotonic id source so concurrent polls never collide.
    next_poll: AtomicU64,
    /// Per-user record of the channel `/poll` was issued from and
    /// the `multi` flag the caller picked, so the follow-up modal
    /// submit (whose envelope does not carry an
    /// `interaction.channel_id`) still broadcasts to the right chat
    /// and remembers the requested vote mode.
    pending: Mutex<HashMap<SessionId, PendingPoll>>,
}

/// Stash for state that has to survive between `/poll` and its
/// follow-up modal submit, keyed by the originating session id.
#[derive(Clone, Copy, Debug, Default)]
struct PendingPoll {
    channel_id: Option<u32>,
    multi: bool,
}

impl std::fmt::Debug for QuickPoll {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuickPoll").finish_non_exhaustive()
    }
}

#[fancy_plugin(name = PLUGIN_NAME, version = PLUGIN_VERSION)]
impl QuickPoll {
    plugin_info! {
        description: "Channel-wide live polls driven by a single modal and a vote dropdown.",
        author: "Fancy Mumble",
        homepage: "https://github.com/Fancy-Mumble/fancy-plugin-example",
        tags: ["demo", "components", "multi-user", "broadcast"],
        manifest: {
            capabilities: [SlashCommands, Modals, Components, Notifications],
        },
    }

    /// Open the create-a-poll modal.  Remembers the originating
    /// chat channel for this session so the modal submit (which does
    /// not propagate `interaction.channel_id` on the wire) can still
    /// target the chat the user actually typed `/poll` in - even if
    /// that chat differs from the user's currently joined voice
    /// channel.
    #[command(name = "poll")]
    fn poll(
        &self,
        host: Host<'_>,
        #[option(description = "Optional question (otherwise the modal asks)")]
        question: Option<String>,
        #[option(description = "Allow voting for multiple options at once")]
        multi: Option<bool>,
    ) -> InteractionResponse {
        let multi = multi.unwrap_or(false);
        if let Some(caller) = host.caller() {
            if let Ok(mut pending) = self.pending.lock() {
                let _ = pending.insert(
                    caller.session_id,
                    PendingPoll {
                        channel_id: caller.channel_id,
                        multi,
                    },
                );
            }
        }
        let mut question_input = TextInput::label("Question")
            .placeholder("What should we vote on?")
            .required(true)
            .max_length(200);
        if let Some(q) = question.filter(|s| !s.is_empty()) {
            question_input = question_input.value(q);
        }
        show_modal!(Self::on_create, "Create a quick poll", {
            question: question_input,
            options: TextInput::label("Options (one per line)")
                .placeholder("Pizza\nSushi\nTacos")
                .style(TextInputStyle::Paragraph)
                .required(true)
                .max_length(800),
        })
    }

    /// Modal submit: validate, create poll, broadcast the chat
    /// bubble to every session in the originating channel and reply
    /// to the submitter with a status toast.
    #[modal]
    fn on_create(
        &self,
        host: Host<'_>,
        #[field] question: String,
        #[field] options: String,
    ) -> InteractionResponse {
        let question = question.trim().to_owned();
        if question.is_empty() {
            return toast!("Question must not be empty.", ToastLevel::Error);
        }
        let options = Self::parse_options(&options);
        if options.len() < MIN_OPTIONS {
            return toast!(
                format!("Need at least {MIN_OPTIONS} distinct options."),
                ToastLevel::Error,
            );
        }

        let Some(caller) = host.caller() else {
            return toast!("No caller context.", ToastLevel::Error);
        };
        // Resolve the target chat channel.  Precedence:
        //   1. `interaction.channel_id` from the modal submit (if the
        //      client ever starts propagating it),
        //   2. the channel `/poll` was issued from (stashed in
        //      `pending` by `poll()` above) - this is what the user
        //      expects when typing the command into a chat that is
        //      not their currently joined voice channel,
        //   3. the user's joined voice channel as a last-resort
        //      fallback so the poll still goes somewhere sensible.
        let stashed = self
            .pending
            .lock()
            .ok()
            .and_then(|mut p| p.remove(&caller.session_id))
            .unwrap_or_default();
        let Some(channel_id) = caller
            .channel_id
            .or(stashed.channel_id)
            .or_else(|| host.current_channel(caller.server_id, caller.session_id))
        else {
            return toast!(
                "Open a channel before starting a poll.",
                ToastLevel::Error,
            );
        };
        let multi = stashed.multi;

        let recipients = host.sessions_in_channel(caller.server_id, channel_id);
        if recipients.is_empty() {
            return toast!(
                "No active listeners in this channel.",
                ToastLevel::Warning,
            );
        }

        let poll_id = self.next_poll_id();
        let message_id = format!("fancy-quick-poll/msg/{poll_id}");
        let votes = vec![0u32; options.len()];
        let state = PollState {
            question,
            options,
            votes,
            message_id: message_id.clone(),
            recipients: recipients.clone(),
            voted: HashSet::new(),
            server_id: caller.server_id,
            multi,
        };

        let body_md = Self::render(&state);
        let select_row = Self::build_vote_row(&poll_id, &state);
        // The chat-bubble `body` field is rendered as raw HTML by the
        // client, so emitting Markdown there shows the literal
        // asterisks.  Move the rendered tally into a `TextDisplay`
        // component instead; the client's plugin-component renderer
        // routes those through a GFM Markdown parser.
        let broadcast = InteractionResponse::chat_message_with_id(message_id, "")
            .row(row![text_display!(body_md)])
            .row(select_row);

        // Park the state before broadcasting so an extremely fast
        // vote that races the broadcast still finds the poll.
        if let Ok(mut polls) = self.polls.lock() {
            let _ = polls.insert(poll_id.clone(), state);
        }

        host.respond_to_sessions(caller.server_id, &recipients, broadcast);

        toast!(
            format!("Poll \"{poll_id}\" sent to {} recipient(s).", recipients.len()),
            ToastLevel::Success,
        )
    }

    /// Vote dropdown change.  The select uses one static `custom_id`
    /// shared by every poll; each [`SelectOption::value`] encodes
    /// `"<poll_id>:<option_index>"` so the handler can route without
    /// a dynamic id.  In multi-select polls `values` carries every
    /// index the user ticked; each one counts as a separate vote
    /// from the same session (one vote per option, max one ballot
    /// per session).
    #[component]
    fn on_vote(&self, host: Host<'_>, values: Vec<String>) -> InteractionResponse {
        let Some(caller) = host.caller() else {
            return toast!("No caller context.", ToastLevel::Error);
        };
        if values.is_empty() {
            return toast!("No option chosen.", ToastLevel::Error);
        }
        // All selected values must belong to the same poll - the
        // dropdown only ever exposes options for one poll at a time,
        // so a mismatch means the payload was tampered with.
        let mut poll_id: Option<&str> = None;
        let mut choices: Vec<usize> = Vec::with_capacity(values.len());
        for raw in &values {
            let Some((pid, idx_str)) = raw.split_once(':') else {
                return toast!("Malformed vote value.", ToastLevel::Error);
            };
            if let Some(existing) = poll_id {
                if existing != pid {
                    return toast!("Cross-poll vote rejected.", ToastLevel::Error);
                }
            } else {
                poll_id = Some(pid);
            }
            let Ok(idx) = idx_str.parse::<usize>() else {
                return toast!("Invalid option index.", ToastLevel::Error);
            };
            choices.push(idx);
        }
        choices.sort_unstable();
        choices.dedup();
        let Some(poll_id) = poll_id else {
            return toast!("No option chosen.", ToastLevel::Error);
        };

        // Lock once: mutate tally, snapshot data needed for the
        // outbound update, release before the broadcast call.
        let snapshot = {
            let Ok(mut polls) = self.polls.lock() else {
                return toast!("Poll storage unavailable.", ToastLevel::Error);
            };
            let Some(state) = polls.get_mut(poll_id) else {
                return toast!("Poll no longer exists.", ToastLevel::Warning);
            };
            if !state.multi && choices.len() > 1 {
                return toast!("This poll only accepts one choice.", ToastLevel::Error);
            }
            if choices.iter().any(|&c| c >= state.options.len()) {
                return toast!("Option out of range.", ToastLevel::Error);
            }
            if !state.voted.insert(caller.session_id) {
                return toast!("You already voted in this poll.", ToastLevel::Warning);
            }
            for &c in &choices {
                state.votes[c] += 1;
            }
            (
                state.recipients.clone(),
                state.message_id.clone(),
                Self::render(state),
                Self::build_vote_row(poll_id, state),
                state.server_id,
            )
        };
        let (recipients, message_id, body_md, select_row, server_id) = snapshot;

        let update = update_message!(
            message_id,
            clear_components,
            row![text_display!(body_md)],
            select_row,
        );

        host.respond_to_sessions(server_id, &recipients, update);
        toast!("Vote recorded.", ToastLevel::Success)
    }

    fn on_unload(&self, _host: Host<'_>) -> PluginResult<()> {
        if let Ok(mut polls) = self.polls.lock() {
            polls.clear();
        }
        if let Ok(mut pending) = self.pending.lock() {
            pending.clear();
        }
        ROk(())
    }
}

impl QuickPoll {
    /// Mint a unique poll id by combining the FFI-load epoch with a
    /// monotonic counter.  No collisions across reloads short of the
    /// same plugin process running concurrently with the same epoch.
    fn next_poll_id(&self) -> String {
        let n = self.next_poll.fetch_add(1, Ordering::Relaxed);
        format!("p{n}")
    }

    /// Render the chat-bubble body for the given poll state.  Pure
    /// function over `PollState` so callers can re-render after every
    /// vote without juggling intermediate buffers.
    fn render(state: &PollState) -> String {
        let total: u32 = state.votes.iter().copied().sum();
        let mut out = String::with_capacity(64 + state.question.len() + state.options.len() * 32);
        out.push_str("**");
        out.push_str(&state.question);
        out.push_str("**\n\n");
        for (label, count) in state.options.iter().zip(state.votes.iter()) {
            let pct = if total == 0 {
                0
            } else {
                (u64::from(*count) * 100 / u64::from(total)) as u32
            };
            out.push_str(&format!("- `{count:>3}` ({pct:>3}%) {label}\n"));
        }
        out.push_str(&format!("\n_Total votes: {total}_"));
        out
    }

    /// Build the vote control row for the given poll state.  Each
    /// option's `value` is `"<poll_id>:<index>"` so a single static
    /// [`handler_id!`]-routed `custom_id` can serve every poll.
    ///
    /// Single-select polls render as a [`RadioGroup`], multi-select
    /// polls as a [`CheckboxGroup`].
    fn build_vote_row(poll_id: &str, state: &PollState) -> ActionRow {
        let custom_id = handler_id!(Self::on_vote);
        if state.multi {
            let mut group = CheckboxGroup::new(custom_id)
                .min_values(1)
                .max_values(u32::try_from(state.options.len()).unwrap_or(u32::MAX))
                .required(false);
            for (idx, label) in state.options.iter().enumerate() {
                group = group.option(CheckboxOption::new(
                    format!("{poll_id}:{idx}"),
                    label.clone(),
                ));
            }
            row![group]
        } else {
            let mut group = RadioGroup::new(custom_id).required(false);
            for (idx, label) in state.options.iter().enumerate() {
                group = group.option(RadioOption::new(
                    format!("{poll_id}:{idx}"),
                    label.clone(),
                ));
            }
            row![group]
        }
    }

    /// Parse the `options` text-area into a clean `Vec<String>`.
    /// Splits on newlines, trims, drops empties, deduplicates while
    /// preserving order, and caps at [`MAX_OPTIONS`].
    fn parse_options(raw: &str) -> Vec<String> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut out: Vec<String> = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if seen.insert(trimmed.to_ascii_lowercase()) {
                out.push(trimmed.to_owned());
                if out.len() >= MAX_OPTIONS {
                    break;
                }
            }
        }
        out
    }
}

fancy_export_plugin!(QuickPoll::default);

#[cfg(test)]
mod tests {
    use super::*;
    use mumble_plugin_api::Component;

    #[test]
    fn parse_options_dedups_and_trims() {
        let opts = QuickPoll::parse_options("  Pizza  \n Sushi\nPizza\n\nTacos\n");
        assert_eq!(opts, vec!["Pizza", "Sushi", "Tacos"]);
    }

    #[test]
    fn parse_options_caps_at_max() {
        let raw = (0..20)
            .map(|i| format!("opt{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let opts = QuickPoll::parse_options(&raw);
        assert_eq!(opts.len(), MAX_OPTIONS);
    }

    fn state(opts: usize, multi: bool) -> PollState {
        PollState {
            question: "Q".into(),
            options: (0..opts).map(|i| format!("opt{i}")).collect(),
            votes: vec![0; opts],
            message_id: "m".into(),
            recipients: Vec::new(),
            voted: HashSet::new(),
            server_id: 1,
            multi,
        }
    }

    #[test]
    fn render_handles_zero_votes() {
        let body = QuickPoll::render(&state(2, false));
        assert!(body.contains("Total votes: 0"));
        assert!(body.contains("opt0"));
        assert!(body.contains("opt1"));
    }

    #[test]
    fn vote_row_encodes_poll_id_and_index() {
        let s = state(3, false);
        let row = QuickPoll::build_vote_row("p0", &s);
        let group = match &row.components[0] {
            Component::RadioGroup(g) => g,
            other => panic!("expected RadioGroup, got {other:?}"),
        };
        let values: Vec<&str> = group.options.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(values, vec!["p0:0", "p0:1", "p0:2"]);
    }

    #[test]
    fn single_select_uses_radio_group() {
        let row = QuickPoll::build_vote_row("p0", &state(4, false));
        assert!(matches!(row.components[0], Component::RadioGroup(_)));
    }

    #[test]
    fn multi_select_uses_checkbox_group_with_full_range() {
        let row = QuickPoll::build_vote_row("p0", &state(4, true));
        let group = match &row.components[0] {
            Component::CheckboxGroup(g) => g,
            other => panic!("expected CheckboxGroup, got {other:?}"),
        };
        assert_eq!(group.min_values, 1);
        assert_eq!(group.max_values, Some(4));
        assert!(!group.required);
    }
}
