//! Admission of Claude Channel messages from opted-in MCP servers.
//!
//! A message rides the parent-message delivery path: a Steer slot drained at the running turn's
//! next safe point, or a queued turn when the session is idle. Slots that a turn ends without
//! draining are re-queued by `transition_parent_messages`, so a message that arrives during a
//! review or a compaction is held, never dropped.

use super::*;
use crate::session::mcp_channel::{MCP_CHANNEL_META_KEY, McpChannelEnvelope};
use std::collections::HashMap;
use std::sync::Arc;
use xai_grok_mcp::channel::McpChannelNotification;
use xai_grok_tools::implementations::grok_build::task::coordinator::ActiveMessageAdmission;
use xai_grok_tools::implementations::grok_build::task::types::{
    ActiveAgentMessage, ActiveAgentMessageOperation, ActiveAgentMessageSource,
};

pub(crate) const MCP_CHANNEL_PROMPT_ID_PREFIX: &str = "mcp-channel-";
/// Channel turns waiting in the queue while the session is idle or busy; later messages are
/// dropped and show up as `undelivered_before` on the next admitted one.
const MAX_QUEUED_CHANNEL_MESSAGES: usize = 64;

/// Where an admitted channel message came from, carried with it through delivery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct McpChannelOrigin {
    pub server: String,
    pub connection: u64,
    pub sequence: u64,
    pub undelivered_before: u64,
}

impl McpChannelOrigin {
    /// `mcp-channel-{connection}-{sequence}-{server}`; [`PromptOrigin::from_prompt_id`] reads it back.
    pub(crate) fn prompt_id(&self) -> String {
        let Self {
            server,
            connection,
            sequence,
            ..
        } = self;
        format!("{MCP_CHANNEL_PROMPT_ID_PREFIX}{connection}-{sequence}-{server}")
    }

    pub(crate) fn from_prompt_origin(origin: &PromptOrigin) -> Option<Self> {
        match origin {
            PromptOrigin::McpChannelMessage {
                server,
                connection,
                sequence,
                undelivered_before,
            } => Some(Self {
                server: server.clone(),
                connection: *connection,
                sequence: *sequence,
                undelivered_before: *undelivered_before,
            }),
            PromptOrigin::User
            | PromptOrigin::TaskCompleted { .. }
            | PromptOrigin::SubagentCompleted { .. }
            | PromptOrigin::ParentAgentMessage { .. }
            | PromptOrigin::ParentHumanMessage { .. }
            | PromptOrigin::WorkflowCompleted { .. }
            | PromptOrigin::NotificationDrain
            | PromptOrigin::GoalSummary
            | PromptOrigin::GoalClassifierNudge
            | PromptOrigin::SchedulerFired
            | PromptOrigin::PlanResume => None,
        }
    }

    pub(crate) fn prompt_origin(&self) -> PromptOrigin {
        PromptOrigin::McpChannelMessage {
            server: self.server.clone(),
            connection: self.connection,
            sequence: self.sequence,
            undelivered_before: self.undelivered_before,
        }
    }

    /// Stamps the delivery record on a user chunk's `_meta` so the pager renders a channel card.
    pub(crate) fn stamp_chunk_meta(&self, meta: &mut serde_json::Map<String, serde_json::Value>) {
        meta.insert(
            MCP_CHANNEL_META_KEY.into(),
            serde_json::json!({
                "server": self.server,
                "connection": self.connection,
                "sequence": self.sequence,
                "undeliveredBefore": self.undelivered_before,
            }),
        );
    }
}

/// Last admitted sequence per server connection, so the model is told about every gap.
#[derive(Default)]
pub(crate) struct ChannelDeliveryState {
    last_admitted: HashMap<(String, u64), u64>,
}

impl ChannelDeliveryState {
    /// Records `sequence` as admitted on `connection` and returns how many earlier sequence
    /// numbers on that connection were never admitted here.
    pub(crate) fn admit(&mut self, server: &str, connection: u64, sequence: u64) -> u64 {
        let last = self
            .last_admitted
            .entry((server.to_owned(), connection))
            .or_insert(0);
        let undelivered_before = sequence.saturating_sub(*last).saturating_sub(1);
        *last = (*last).max(sequence);
        undelivered_before
    }
}

fn queued_channel_messages(state: &State) -> usize {
    state
        .pending_inputs
        .iter()
        .filter(|item| {
            matches!(
                item.input_origin.as_prompt_origin(),
                PromptOrigin::McpChannelMessage { .. }
            )
        })
        .count()
}

impl SessionActor {
    pub(super) async fn admit_mcp_channel_message(
        self: &Arc<Self>,
        notification: McpChannelNotification,
        completion_tx: mpsc::UnboundedSender<super::turn_task::TurnCompletionMsg>,
    ) {
        let server = notification.server_name.clone();
        let (connection, sequence) = (notification.connection, notification.sequence);
        let undelivered_before = {
            let mut state = self.state.lock().await;
            if queued_channel_messages(&state) >= MAX_QUEUED_CHANNEL_MESSAGES {
                tracing::warn!(
                    session_id = %self.session_info.id.0,
                    server = %server,
                    sequence,
                    "dropped Claude Channel message: {MAX_QUEUED_CHANNEL_MESSAGES} already queued"
                );
                return;
            }
            state.channel_delivery.admit(&server, connection, sequence)
        };
        let Some(text) = McpChannelEnvelope::new(notification, undelivered_before).render() else {
            tracing::warn!(
                session_id = %self.session_info.id.0,
                server = %server,
                sequence,
                "dropped Claude Channel message: rendered envelope exceeds the cap"
            );
            return;
        };
        let origin = McpChannelOrigin {
            server: server.clone(),
            connection,
            sequence,
            undelivered_before,
        };
        let message = ActiveAgentMessage {
            message_id: origin.prompt_id(),
            sender_session_id: format!("mcp:{server}"),
            text: text.into(),
        };
        // The admission path hands a receipt to a parent that awaits the turn; a channel has no
        // parent, so the receipt is taken and dropped once admission has settled.
        let (receipt_tx, receipt_rx) = mpsc::channel(1);
        let (respond_to, admission) = oneshot::channel();
        let telemetry_ctx = xai_grok_telemetry::TelemetryCtx::new(
            self.session_info.id.0.to_string(),
            Arc::new(tokio::sync::Mutex::new(0)),
        );
        self.admit_parent_agent_message_inner(
            None,
            ActiveAgentMessageSource::Human,
            Some(origin),
            message,
            ActiveAgentMessageOperation::Steer,
            receipt_tx,
            telemetry_ctx,
            respond_to,
            completion_tx,
        )
        .await;
        drop(receipt_rx);
        match admission.await {
            Ok(ActiveMessageAdmission::Admitted) => tracing::debug!(
                session_id = %self.session_info.id.0,
                server = %server,
                sequence,
                undelivered_before,
                "admitted Claude Channel message"
            ),
            Ok(ActiveMessageAdmission::Rejected | ActiveMessageAdmission::Unsupported)
            | Ok(ActiveMessageAdmission::ChannelClosed)
            | Err(_) => tracing::warn!(
                session_id = %self.session_info.id.0,
                server = %server,
                sequence,
                "Claude Channel message was not admitted"
            ),
        }
    }
}

#[cfg(test)]
#[path = "channel_message_tests.rs"]
mod tests;
