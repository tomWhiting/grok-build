use super::*;
use pretty_assertions::assert_eq;

fn chunk(text: &str, meta: Option<serde_json::Value>) -> acp::ContentChunk {
    acp::ContentChunk::new(acp::ContentBlock::Text(acp::TextContent::new(
        text.to_string(),
    )))
    .meta(meta.and_then(|value| value.as_object().cloned()))
}

#[test]
fn plain_user_chunks_are_not_channel_messages() {
    assert_eq!(
        ChannelMessageBlock::from_chunk(&chunk(
            "hello",
            Some(serde_json::json!({"promptIndex": 1}))
        )),
        None
    );
    assert_eq!(ChannelMessageBlock::from_chunk(&chunk("hello", None)), None);
}

#[test]
fn channel_chunks_show_the_message_as_sent() {
    let record = serde_json::json!({
        "mcpChannel": { "server": "cambium", "connection": 7, "sequence": 12, "undeliveredBefore": 2 }
    });
    let text = "<mcp_channel_message>\n{\"source\":{\"kind\":\"mcp_server\",\"server\":\"cambium\"},\"trust\":\"t\",\"delivery\":{\"connection\":7,\"sequence\":12,\"undelivered_before\":2},\"content\":\"ship \\u003cit\\u003e\",\"meta\":{\"from\":\"waffles\"}}\n</mcp_channel_message>";
    assert_eq!(
        ChannelMessageBlock::from_chunk(&chunk(text, Some(record.clone()))),
        Some(ChannelMessageBlock {
            server: "cambium".to_string(),
            connection: 7,
            sequence: 12,
            undelivered_before: 2,
            content: "ship <it>".to_string(),
            meta: vec![("from".to_string(), "waffles".to_string())],
        })
    );
    // Text that is not an envelope is shown as it is rather than dropped.
    let fallback = ChannelMessageBlock::from_chunk(&chunk("raw text", Some(record))).unwrap();
    assert_eq!(
        (fallback.content.as_str(), fallback.meta.len()),
        ("raw text", 0)
    );
}
