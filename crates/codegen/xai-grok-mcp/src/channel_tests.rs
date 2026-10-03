use super::*;
use serde_json::json;

fn server_info(experimental: Value) -> ServerPeerInfo {
    serde_json::from_value(json!({
        "protocolVersion": "2025-11-25",
        "capabilities": { "experimental": experimental },
        "serverInfo": { "name": "fixture", "version": "0" },
    }))
    .expect("fixture server info parses")
}

fn opted_in() -> ServerPeerInfo {
    server_info(json!({ CLAUDE_CHANNEL_CAPABILITY: {} }))
}

#[test]
fn admits_content_with_string_meta_and_numbers_each_message() {
    let ingress = ChannelIngress::new();
    let info = opted_in();
    let first = ingress
        .receive(
            "cambium",
            Some(json!({ "content": "hello", "meta": { "from": "waffles" } })),
            Some(&info),
        )
        .expect("first message admitted");
    let second = ingress
        .receive("cambium", Some(json!({ "content": "again" })), Some(&info))
        .expect("second message admitted");
    assert_eq!(
        first,
        McpChannelNotification {
            server_name: "cambium".to_string(),
            content: "hello".to_string(),
            meta: BTreeMap::from([("from".to_string(), "waffles".to_string())]),
            connection: ingress.connection(),
            sequence: 1,
        }
    );
    assert_eq!((second.sequence, second.meta.len()), (2, 0));
}

#[test]
fn refusals_still_consume_a_sequence_number() {
    let ingress = ChannelIngress::new();
    let info = opted_in();
    assert_eq!(
        ingress.receive("s", Some(json!({ "content": 7 })), Some(&info)),
        Err(ChannelRefusal::Malformed)
    );
    assert_eq!(
        ingress.receive("s", Some(json!({ "content": "x", "meta": { "k": 1 } })), Some(&info)),
        Err(ChannelRefusal::Malformed)
    );
    let admitted = ingress
        .receive("s", Some(json!({ "content": "x" })), Some(&info))
        .expect("admitted");
    assert_eq!(admitted.sequence, 3);
}

#[test]
fn capability_is_required_and_decided_once() {
    let ingress = ChannelIngress::new();
    let without = server_info(json!({}));
    assert_eq!(
        ingress.receive("s", Some(json!({ "content": "x" })), Some(&without)),
        Err(ChannelRefusal::CapabilityNotDeclared)
    );
    // The decision is per connection: later info on the same connection does not reopen it.
    let with = opted_in();
    assert_eq!(
        ingress.receive("s", Some(json!({ "content": "x" })), Some(&with)),
        Err(ChannelRefusal::CapabilityNotDeclared)
    );
}

#[test]
fn handshake_must_have_completed() {
    let ingress = ChannelIngress::new();
    assert_eq!(
        ingress.receive("s", Some(json!({ "content": "x" })), None),
        Err(ChannelRefusal::HandshakeIncomplete)
    );
}

#[test]
fn oversized_notifications_are_refused_by_size() {
    let ingress = ChannelIngress::new();
    let info = opted_in();
    let content = "a".repeat(MCP_CHANNEL_MAX_NOTIFICATION_BYTES);
    assert_eq!(
        ingress.receive(
            "s",
            Some(json!({ "content": content, "meta": { "k": "v" } })),
            Some(&info)
        ),
        Err(ChannelRefusal::Oversized {
            bytes: MCP_CHANNEL_MAX_NOTIFICATION_BYTES + 2
        })
    );
}

#[test]
fn connections_are_distinct() {
    assert_ne!(
        ChannelIngress::new().connection(),
        ChannelIngress::new().connection()
    );
}
