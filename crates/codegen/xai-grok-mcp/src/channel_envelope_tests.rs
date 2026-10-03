use super::*;

fn notification(content: &str, meta: &[(&str, &str)]) -> McpChannelNotification {
    McpChannelNotification {
        server_name: "cambium".to_string(),
        content: content.to_string(),
        meta: meta
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect(),
        connection: 7,
        sequence: 3,
    }
}

#[test]
fn render_round_trips_through_parse_and_names_mirrored_meta() {
    let envelope = McpChannelEnvelope::new(
        notification("ship <it> & go", &[("from", "waffles"), ("text", "ship <it> & go")]),
        2,
    );
    assert_eq!(
        envelope,
        McpChannelEnvelope {
            source: EnvelopeSource {
                kind: "mcp_server".to_string(),
                server: "cambium".to_string(),
            },
            trust: TRUST.to_string(),
            delivery: EnvelopeDelivery {
                connection: 7,
                sequence: 3,
                undelivered_before: 2,
            },
            content: "ship <it> & go".to_string(),
            meta: BTreeMap::from([("from".to_string(), "waffles".to_string())]),
            meta_same_as_content: vec!["text".to_string()],
        }
    );
    let text = envelope.render().expect("fits the cap");
    let body = text
        .strip_prefix("<mcp_channel_message>\n")
        .and_then(|rest| rest.strip_suffix("\n</mcp_channel_message>"))
        .expect("markers wrap one line");
    assert!(!body.contains(['<', '>', '&']), "{body}");
    assert_eq!(McpChannelEnvelope::parse(&text), Some(envelope));
}

#[test]
fn oversized_envelopes_do_not_render() {
    let envelope = McpChannelEnvelope::new(
        notification(&"a".repeat(MCP_CHANNEL_MAX_RENDERED_BYTES), &[]),
        0,
    );
    assert_eq!(envelope.render(), None);
}

#[test]
fn parse_rejects_text_that_is_not_an_envelope() {
    assert_eq!(McpChannelEnvelope::parse("hello"), None);
    assert_eq!(
        McpChannelEnvelope::parse("<mcp_channel_message>\nnot json\n</mcp_channel_message>"),
        None
    );
}
