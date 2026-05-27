# fancy-quick-poll

End-to-end example of the multi-user broadcast pipeline: a single
`/poll` command turns the channel into a live tally.  Every member of
the originating channel receives the same chat bubble, and every vote
rewrites that bubble in-place on every recipient.

## What it shows

- `PluginContext::sessions_in_channel` for enumerating recipients at
  poll-creation time.
- `send_interaction_response_to_sessions` for fanning a single
  `InteractionResponse` out to many clients without N round-trips.
- `InteractionResponse::chat_message_with_id` + `update_message!` to
  mutate a previously broadcast chat bubble so every recipient sees
  the same updated tally.
- A multi-field modal collecting a question (short input) and the
  list of options (paragraph, one per line, deduplicated, capped at
  10).
- Voter dedup via `PluginMessageIn::sender_session` - clicking the
  dropdown twice from the same client counts once.
- A manual `MumblePlugin` impl (no `#[fancy_plugin]` macro) - the
  poll needs `msg.sender_session` and `msg.server_id`, which the
  macro currently does not surface to handler methods.  See the other
  examples for the conventional macro-driven approach.

## Flow

```text
/poll                  -> show_modal!("Create a quick poll")
modal submit "create"  -> sessions_in_channel(...)
                          chat_message_with_id(...).row(StringSelect)
                          -> broadcast to every channel member
component "vote/<id>"  -> mutate stored tally
                          update_message!(message_id, content, row)
                          -> broadcast to the same recipient list
```

## Known limitations

- Polls live in plugin memory only; reloading the plugin forgets
  every open poll.
- Recipients are frozen at poll-creation time.  A user who joins the
  channel afterwards will not receive the broadcast and cannot vote
  on the existing poll.
- Voter dedup keys on the inbound session id.  Reconnecting mints a
  new session and lets the same user vote again.

## Try it

1. Build the example: `cargo build -p fancy-quick-poll`
2. Drop the produced `cdylib` into your Mumble plugin directory.
3. Add the contents of [`plugin.example.ini`](plugin.example.ini) to
   your `mumble-server.ini`.
4. Restart, join a channel with at least one other listener, and run
   `/poll` from the composer.
