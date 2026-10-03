//! Bounded admission of `notifications/claude/channel` from an opted-in MCP server.
//!
//! A server opts in by declaring the experimental capability [`CLAUDE_CHANNEL_CAPABILITY`] in its
//! `initialize` result and by being configured with `channel = "wake"`. Each connection numbers
//! every notification carrying the channel method, including ones refused here, so a gap in the
//! sequence the session sees is a message that was never delivered.

use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use rmcp::model::ServerPeerInfo;
use serde_json::Value;

pub const CLAUDE_CHANNEL_CAPABILITY: &str = "claude/channel";
pub const CLAUDE_CHANNEL_NOTIFICATION_METHOD: &str = "notifications/claude/channel";

/// Largest content plus metadata, in UTF-8 bytes, kept from one notification.
pub const MCP_CHANNEL_MAX_NOTIFICATION_BYTES: usize = 32 * 1024;

/// Distinguishes connections within this process so sequence numbers are never compared across
/// a reconnect.
static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(1);

/// One external notification attributed to its configured MCP server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpChannelNotification {
    pub server_name: String,
    pub content: String,
    pub meta: BTreeMap<String, String>,
    /// Identifies the server connection; a reconnect starts a new sequence.
    pub connection: u64,
    /// Arrival order on `connection`, starting at 1; gaps are messages that were not delivered.
    pub sequence: u64,
}

/// Why a notification carrying the channel method was not admitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelRefusal {
    /// The server's `initialize` result did not declare [`CLAUDE_CHANNEL_CAPABILITY`].
    CapabilityNotDeclared,
    /// The notification arrived before the handshake recorded the server's capabilities.
    HandshakeIncomplete,
    /// `params` was not an object with a string `content` and an optional string-valued `meta`.
    Malformed,
    /// Content plus metadata exceeded [`MCP_CHANNEL_MAX_NOTIFICATION_BYTES`].
    Oversized { bytes: usize },
}

/// Per-connection admission state, created with the client handler of each handshake.
#[derive(Debug)]
pub struct ChannelIngress {
    connection: u64,
    last_sequence: AtomicU64,
    /// Whether the server declared the capability, decided once from the first handshake info seen.
    declared: OnceLock<bool>,
}

impl Default for ChannelIngress {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelIngress {
    pub fn new() -> Self {
        Self {
            connection: NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed),
            last_sequence: AtomicU64::new(0),
            declared: OnceLock::new(),
        }
    }

    pub fn connection(&self) -> u64 {
        self.connection
    }

    /// Admit one notification with the channel method. Every call consumes a sequence number.
    pub fn receive(
        &self,
        server_name: &str,
        params: Option<Value>,
        server_info: Option<&ServerPeerInfo>,
    ) -> Result<McpChannelNotification, ChannelRefusal> {
        let sequence = self.last_sequence.fetch_add(1, Ordering::Relaxed) + 1;
        let declared = match (self.declared.get(), server_info) {
            (Some(declared), _) => *declared,
            (None, Some(info)) => *self.declared.get_or_init(|| declares_capability(info)),
            (None, None) => return Err(ChannelRefusal::HandshakeIncomplete),
        };
        if !declared {
            return Err(ChannelRefusal::CapabilityNotDeclared);
        }
        let (content, meta) = parse_notification(params).ok_or(ChannelRefusal::Malformed)?;
        let bytes = meta
            .iter()
            .try_fold(content.len(), |total, (key, value)| {
                total.checked_add(key.len())?.checked_add(value.len())
            })
            .ok_or(ChannelRefusal::Oversized { bytes: usize::MAX })?;
        if bytes > MCP_CHANNEL_MAX_NOTIFICATION_BYTES {
            return Err(ChannelRefusal::Oversized { bytes });
        }
        Ok(McpChannelNotification {
            server_name: server_name.to_string(),
            content,
            meta,
            connection: self.connection,
            sequence,
        })
    }
}

fn declares_capability(info: &ServerPeerInfo) -> bool {
    info.capabilities
        .experimental
        .as_ref()
        .is_some_and(|capabilities| capabilities.contains_key(CLAUDE_CHANNEL_CAPABILITY))
}

fn parse_notification(params: Option<Value>) -> Option<(String, BTreeMap<String, String>)> {
    let Value::Object(mut fields) = params? else {
        return None;
    };
    let Value::String(content) = fields.remove("content")? else {
        return None;
    };
    let meta = match fields.remove("meta") {
        None => BTreeMap::new(),
        Some(Value::Object(values)) => values
            .into_iter()
            .map(|(key, value)| value.as_str().map(|value| (key, value.to_string())))
            .collect::<Option<BTreeMap<_, _>>>()?,
        Some(_) => return None,
    };
    Some((content, meta))
}

#[cfg(test)]
#[path = "channel_tests.rs"]
mod tests;
