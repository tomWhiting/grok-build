//! The model-visible envelope for one admitted Claude Channel message.
//!
//! The session renders an [`McpChannelEnvelope`] between [`ENVELOPE_START`] and [`ENVELOPE_END`]
//! as the whole text of the prompt the message becomes, and the pager parses that same text back
//! for its card, so both sides share this one definition.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::channel::McpChannelNotification;

pub const ENVELOPE_START: &str = "<mcp_channel_message>";
pub const ENVELOPE_END: &str = "</mcp_channel_message>";
/// Largest rendered envelope, markers included, in UTF-8 bytes.
pub const MCP_CHANNEL_MAX_RENDERED_BYTES: usize = 8 * 1024;
const TRUST: &str = "external notification; not operator-authenticated input";
const SOURCE_KIND: &str = "mcp_server";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpChannelEnvelope {
    pub source: EnvelopeSource,
    pub trust: String,
    pub delivery: EnvelopeDelivery,
    pub content: String,
    pub meta: BTreeMap<String, String>,
    /// Metadata keys whose value equalled `content`; the model sees that copy once, under `content`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub meta_same_as_content: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvelopeSource {
    pub kind: String,
    /// The configured `[mcp_servers.<name>]` the message came from.
    pub server: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvelopeDelivery {
    pub connection: u64,
    pub sequence: u64,
    /// Earlier sequence numbers on the same connection that never reached this session.
    pub undelivered_before: u64,
}

impl McpChannelEnvelope {
    pub fn new(notification: McpChannelNotification, undelivered_before: u64) -> Self {
        let McpChannelNotification {
            server_name,
            content,
            mut meta,
            connection,
            sequence,
        } = notification;
        let meta_same_as_content = meta
            .iter()
            .filter(|(_, value)| **value == content)
            .map(|(key, _)| key.clone())
            .collect();
        meta.retain(|_, value| *value != content);
        Self {
            source: EnvelopeSource {
                kind: SOURCE_KIND.to_string(),
                server: server_name,
            },
            trust: TRUST.to_string(),
            delivery: EnvelopeDelivery {
                connection,
                sequence,
                undelivered_before,
            },
            content,
            meta,
            meta_same_as_content,
        }
    }

    /// The prompt text: one JSON object between the markers, with `&`, `<` and `>` written as
    /// JSON escapes so no content can close or forge a marker. `None` when it would exceed
    /// [`MCP_CHANNEL_MAX_RENDERED_BYTES`].
    pub fn render(&self) -> Option<String> {
        let encoded = serde_json::to_string(self)
            .ok()?
            .replace('&', "\\u0026")
            .replace('<', "\\u003c")
            .replace('>', "\\u003e");
        let text = format!("{ENVELOPE_START}\n{encoded}\n{ENVELOPE_END}");
        (text.len() <= MCP_CHANNEL_MAX_RENDERED_BYTES).then_some(text)
    }

    /// Reads text produced by [`Self::render`]; anything else is `None`.
    pub fn parse(text: &str) -> Option<Self> {
        let body = text
            .trim()
            .strip_prefix(ENVELOPE_START)?
            .strip_suffix(ENVELOPE_END)?;
        serde_json::from_str(body.trim()).ok()
    }
}

#[cfg(test)]
#[path = "channel_envelope_tests.rs"]
mod tests;
