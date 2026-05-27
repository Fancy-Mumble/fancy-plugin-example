# fancy-chat-card

Minimal Fancy Mumble plugin demonstrating the **`chat_message!`** API:
instead of a floating modal overlay, the response is rendered as a
literal chat message in the channel/DM history - exactly like a
`TextMessage` authored by a user - with optional interactive
components rendered inline inside the same chat bubble.

## What it does

* Registers a single `/chat-card` slash command.
* On invocation, the plugin posts a literal chat message into the
  channel the command was invoked from, containing:
  * a Markdown body, and
  * a single `Button` row attached to the bubble.
* Clicking the button replies with a `toast!` so you can see the
  round-trip works from inside a chat-message-hosted component.

## Why this is different from `message!`

| Macro | Where it appears | Persists in scroll | Component routing |
| ----- | ---------------- | ------------------ | ----------------- |
| `message!` | Floating "card" overlay above the chat | No - cleared with the overlay | Yes |
| `chat_message!` | Inline chat bubble, authored by the plugin | Yes - identical to user-sent messages | Yes |

Use `chat_message!` whenever the content is part of the conversation
(e.g. a bot announcement, a polled-result summary, a pinned status
update).  Stick with `message!` for transient confirmations and
overlay-style flows.
