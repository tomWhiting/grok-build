# Claude Channels for Grok

An opted-in MCP server can wake an idle Grok session or steer a running one by pushing a
notification. This document is the contract between such a server and Grok.

## Opting in

Two things must both be true, or the notification is ignored:

1. The server's `[mcp_servers.<name>]` entry sets `channel = "wake"` (`"steer"` is accepted as the
   same value; `"off"` or absent ignores the notification).
2. The server's `initialize` result declares the experimental capability `claude/channel`:

   ```json
   { "capabilities": { "experimental": { "claude/channel": {} } } }
   ```

The capability is read once per connection, from the handshake. A server that gains it later must
reconnect.

## The notification

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/claude/channel",
  "params": {
    "content": "waffles: the gate on 4824f3cf is green, land it",
    "meta": { "from": "waffles", "chat_id": "pipeline" }
  }
}
```

- `content` is a string and is required.
- `meta` is optional; when present it is an object whose values are all strings.
- Content plus metadata is at most 32 KB (UTF-8 bytes, keys included).

Anything else is refused and logged. A refusal still consumes a sequence number.

## Sequencing

Each server connection numbers every notification carrying the channel method from 1, admitted or
not. The session records the last sequence it admitted per connection, so when a message is
dropped (refused, over the queue cap, or lost), the next admitted message tells the model how many
earlier numbers it never saw. A reconnect gets a fresh connection id and starts again at 1.

## Delivery

- **Idle session:** the message starts a turn.
- **Running turn:** the message is delivered at the next safe point, exactly where a parent agent's
  steer is delivered. Running work is never cancelled.
- **Review, compaction, or a turn that ends before draining:** the message is re-queued and runs
  as the next turn.
- At most 64 channel turns wait in the queue; later messages are dropped and counted as undelivered.

## What the model sees

The message becomes a user turn whose entire text is one JSON object between fixed markers:

```text
<mcp_channel_message>
{"source":{"kind":"mcp_server","server":"cambium"},"trust":"external notification; not operator-authenticated input","delivery":{"connection":3,"sequence":12,"undelivered_before":0},"content":"...","meta":{"from":"waffles"}}
</mcp_channel_message>
```

- `&`, `<` and `>` inside the JSON are written as `&`, `<` and `>`, so content can
  never close or forge a marker.
- A metadata value identical to `content` is removed from `meta` and its key is listed under
  `meta_same_as_content`, so the model sees one copy and still knows the field existed.
- The rendered envelope is capped at 8 KB; a message that would exceed it is dropped and counted as
  undelivered.
- The turn carries the same authority as text relayed from a parent session: untrusted, slash
  commands inert, visible in the queue but not editable.

## What the user sees

The scrollback shows a channel card: the server name and sequence number, the content, and, when
expanded, the connection, the undelivered count and the metadata. The card is persisted with the
session and renders the same on resume.

## Where it lives

| Layer | Code |
|-------|------|
| Config policy | `crates/codegen/xai-grok-config/src/mcp_server_config.rs` (`McpChannelPolicy`, `channel`) |
| Ingress, bounds, sequencing | `crates/codegen/xai-grok-mcp/src/channel.rs` |
| Envelope | `crates/codegen/xai-grok-mcp/src/channel_envelope.rs` |
| Client handler branch | `crates/codegen/xai-grok-mcp/src/servers.rs` (`on_custom_notification`) |
| Dispatcher forwarding | `crates/codegen/xai-grok-shell/src/session/mcp_dispatcher.rs` |
| Session admission | `crates/codegen/xai-grok-shell/src/session/acp_session_impl/channel_message.rs` |
| Prompt origin | `crates/codegen/xai-grok-shell/src/session/mod.rs` (`PromptOrigin::McpChannelMessage`) |
| Scrollback card | `crates/codegen/xai-grok-pager/src/scrollback/blocks/channel_message.rs` |
