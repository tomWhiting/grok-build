use super::*;
use pretty_assertions::assert_eq;

#[test]
fn undelivered_before_counts_gaps_per_connection() {
    let mut state = ChannelDeliveryState::default();
    assert_eq!(state.admit("cambium", 1, 1), 0);
    assert_eq!(state.admit("cambium", 1, 2), 0);
    assert_eq!(state.admit("cambium", 1, 5), 2);
    // A reconnect starts a new sequence, so nothing before its first message is missing.
    assert_eq!(state.admit("cambium", 2, 1), 0);
    // Another server on the same connection number is a different stream.
    assert_eq!(state.admit("other", 1, 3), 2);
    // Late or repeated numbers never count as gaps.
    assert_eq!(state.admit("cambium", 1, 4), 0);
}

#[test]
fn prompt_id_round_trips_through_prompt_origin() {
    let origin = McpChannelOrigin {
        server: "cam-bium".to_string(),
        connection: 7,
        sequence: 12,
        undelivered_before: 0,
    };
    assert_eq!(origin.prompt_id(), "mcp-channel-7-12-cam-bium");
    assert_eq!(
        PromptOrigin::from_prompt_id(&origin.prompt_id()),
        origin.prompt_origin()
    );
    assert_eq!(
        McpChannelOrigin::from_prompt_origin(&origin.prompt_origin()),
        Some(origin)
    );
}

#[test]
fn chunk_meta_carries_the_delivery_record() {
    let origin = McpChannelOrigin {
        server: "cambium".to_string(),
        connection: 7,
        sequence: 12,
        undelivered_before: 3,
    };
    let mut meta = serde_json::Map::new();
    origin.stamp_chunk_meta(&mut meta);
    assert_eq!(
        serde_json::Value::Object(meta),
        serde_json::json!({
            "mcpChannel": {
                "server": "cambium",
                "connection": 7,
                "sequence": 12,
                "undeliveredBefore": 3,
            }
        })
    );
}
