//! A message delivered by an opted-in MCP server's Claude Channel.
//!
//! The shell echoes the message as a `UserMessageChunk` whose text is the model-visible envelope
//! and whose `_meta` carries the delivery record; this block shows the message as the server sent
//! it, with the delivery record and metadata on expand. Cyan accent, like the running indicator,
//! so it reads as external input rather than as a typed prompt.

use agent_client_protocol as acp;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use xai_grok_shell::session::mcp_channel::{MCP_CHANNEL_META_KEY, McpChannelEnvelope};

use crate::appearance::AppearanceConfig;
use crate::render::wrapping::word_wrap_lines;
use crate::scrollback::block::BlockContent;
use crate::scrollback::types::{
    AccentStyle, BlockContext, BlockLine, BlockOutput, DisplayMode, Selectable,
};
use crate::theme::Theme;

/// Body lines shown while collapsed.
const COLLAPSED_MAX_LINES: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelMessageBlock {
    pub server: String,
    pub connection: u64,
    pub sequence: u64,
    pub undelivered_before: u64,
    /// The message as the server sent it.
    pub content: String,
    /// String-valued metadata as the server sent it, in key order.
    pub meta: Vec<(String, String)>,
}

impl ChannelMessageBlock {
    /// Reads a channel message echo; `None` for every other user chunk.
    pub fn from_chunk(chunk: &acp::ContentChunk) -> Option<Self> {
        let record = chunk.meta.as_ref()?.get(MCP_CHANNEL_META_KEY)?;
        let acp::ContentBlock::Text(text) = &chunk.content else {
            return None;
        };
        let number = |key: &str| record.get(key).and_then(serde_json::Value::as_u64);
        let mut block = Self {
            server: record
                .get("server")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            connection: number("connection").unwrap_or(0),
            sequence: number("sequence").unwrap_or(0),
            undelivered_before: number("undeliveredBefore").unwrap_or(0),
            content: text.text.clone(),
            meta: Vec::new(),
        };
        if let Some(envelope) = McpChannelEnvelope::parse(&text.text) {
            block.content = envelope.content;
            block.meta = envelope.meta.into_iter().collect();
        }
        Some(block)
    }

    fn header(&self, theme: &Theme) -> Line<'static> {
        let accent = Style::default()
            .fg(theme.running)
            .add_modifier(Modifier::BOLD);
        let mut spans = vec![
            Span::styled("\u{21C4} ".to_string(), accent),
            Span::styled(self.server.clone(), accent),
            Span::styled(format!("  #{}", self.sequence), theme.muted()),
        ];
        if self.undelivered_before > 0 {
            spans.push(Span::styled(
                format!("  {} undelivered before", self.undelivered_before),
                Style::default().fg(theme.warning),
            ));
        }
        Line::from(spans)
    }
}

impl BlockContent for ChannelMessageBlock {
    fn output(&self, ctx: &BlockContext) -> BlockOutput {
        let theme = Theme::current();
        let collapsed = ctx.mode != DisplayMode::Expanded;
        let mut lines = vec![BlockLine::styled(self.header(&theme)).with_selection_range(Some(0))];

        let body: Vec<Line<'static>> = self
            .content
            .lines()
            .map(|line| Line::from(Span::raw(line.to_string())))
            .collect();
        let mut body = word_wrap_lines(body, ctx.width as usize);
        if collapsed && body.len() > COLLAPSED_MAX_LINES {
            body.truncate(COLLAPSED_MAX_LINES);
            if let Some(last) = body.last_mut() {
                last.spans
                    .push(Span::styled(" \u{2026}".to_string(), theme.muted()));
            }
        }
        for line in body {
            let mut line = BlockLine::styled(line).with_selection_range(Some(0));
            line.selectable = Selectable::Spans(0..line.content.spans.len());
            lines.push(line);
        }

        if !collapsed {
            let detail = |text: String| {
                BlockLine::styled(Line::from(Span::styled(text, theme.muted())))
                    .with_selection_range(Some(0))
            };
            lines.push(BlockLine::separator(Line::from("")));
            lines.push(detail(format!(
                "connection {}  sequence {}  undelivered before {}",
                self.connection, self.sequence, self.undelivered_before
            )));
            for (key, value) in &self.meta {
                lines.push(detail(format!("{key}: {value}")));
            }
        }
        BlockOutput { lines }
    }

    fn accent(&self, _ctx: &BlockContext) -> Option<AccentStyle> {
        Some(AccentStyle::static_color(Theme::current().running))
    }

    fn has_vpad_for(&self, _appearance: &AppearanceConfig) -> bool {
        false
    }

    fn has_raw_mode(&self) -> bool {
        false
    }

    fn is_foldable(&self) -> bool {
        true
    }

    fn is_groupable(&self) -> bool {
        false
    }

    fn default_display_mode(&self) -> DisplayMode {
        DisplayMode::Collapsed
    }
}

#[cfg(test)]
#[path = "channel_message_tests.rs"]
mod tests;
