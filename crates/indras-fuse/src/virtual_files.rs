use indras_artifacts::{
    ArtifactId, ArtifactStore, AttentionStore, AttentionSwitchEvent, PayloadStore, PlayerId, Vault,
};
use serde_json::{json, Value};

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn artifact_id_short(id: &ArtifactId) -> String {
    let b = id.bytes();
    format!("{:02x}{:02x}{:02x}{:02x}...", b[0], b[1], b[2], b[3])
}

fn attention_event_to_json(event: &AttentionSwitchEvent) -> Value {
    json!({
        "player": bytes_to_hex(&event.player[..8]),
        "from": event.from.as_ref().map(artifact_id_short),
        "to": event.to.as_ref().map(artifact_id_short),
        "timestamp": event.timestamp,
    })
}

/// Generate JSONL content for `.indra/attention.log`.
pub fn generate_attention_log<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
    vault: &Vault<A, P, T>,
) -> Vec<u8> {
    let events = vault.attention_events().unwrap_or_default();
    let mut output = String::new();
    for event in events {
        if let Ok(line) = serde_json::to_string(&attention_event_to_json(&event)) {
            output.push_str(&line);
            output.push('\n');
        }
    }
    output.into_bytes()
}

/// Generate JSON content for `.indra/heat.json`.
pub fn generate_heat_json<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
    vault: &Vault<A, P, T>,
    artifact_paths: &[(String, ArtifactId)],
    now: i64,
) -> Vec<u8> {
    let mut map = serde_json::Map::new();
    for (path, id) in artifact_paths {
        if let Ok(heat) = vault.heat(id, now) {
            map.insert(path.clone(), json!({ "heat": heat }));
        }
    }
    serde_json::to_vec_pretty(&Value::Object(map)).unwrap_or_default()
}

/// Generate JSON content for `.indra/peers.json`.
pub fn generate_peers_json<A: ArtifactStore, P: PayloadStore, T: AttentionStore>(
    vault: &Vault<A, P, T>,
) -> Vec<u8> {
    let peers = vault.peers();
    let entries: Vec<Value> = peers
        .iter()
        .map(|p| {
            json!({
                "peer_id": bytes_to_hex(&p.peer_id),
                "display_name": p.display_name,
                "since": p.since,
            })
        })
        .collect();
    serde_json::to_vec_pretty(&entries).unwrap_or_default()
}

/// Generate JSON content for `.indra/player.json`.
pub fn generate_player_json(player_id: &PlayerId) -> Vec<u8> {
    let obj = json!({
        "player_id": bytes_to_hex(player_id),
    });
    serde_json::to_vec_pretty(&obj).unwrap_or_default()
}
