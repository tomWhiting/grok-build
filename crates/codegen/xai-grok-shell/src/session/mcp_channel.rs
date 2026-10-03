//! Wire names shared by the shell and the pager for Claude Channel messages.

pub use xai_grok_mcp::channel_envelope::McpChannelEnvelope;

/// `UserMessageChunk` / `ContentChunk._meta` key carrying the delivery record of a channel message:
/// `{ "server", "connection", "sequence", "undeliveredBefore" }`. The chunk's text is the rendered
/// [`McpChannelEnvelope`].
pub const MCP_CHANNEL_META_KEY: &str = "mcpChannel";
