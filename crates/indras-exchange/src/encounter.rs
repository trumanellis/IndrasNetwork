use indras_artifacts::{ArtifactId, PlayerId};

/// Generate a human-readable magic code for an intention.
/// Format: INDRA-XXXX-XXXX where X is alphanumeric.
/// Derived deterministically from player_id + request_id via BLAKE3.
pub fn generate_intention_code(player_id: &PlayerId, request_id: &ArtifactId) -> String {
    let mut input = Vec::new();
    input.extend_from_slice(player_id);
    input.extend_from_slice(request_id.bytes());
    let hash = blake3::hash(&input);
    let bytes = hash.as_bytes();

    // Take 8 bytes and encode as uppercase alphanumeric
    let charset: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // no 0/O/1/I confusion
    let part1: String = (0..4).map(|i| charset[(bytes[i] as usize) % charset.len()] as char).collect();
    let part2: String = (4..8).map(|i| charset[(bytes[i] as usize) % charset.len()] as char).collect();

    format!("INDRA-{}-{}", part1, part2)
}

/// Encode player_id and request_id into the magic code's underlying data.
/// Returns (player_id_hex, request_id_hex) encoded in a shareable format.
pub fn encode_intention_data(player_id: &PlayerId, request_id: &ArtifactId) -> String {
    let player_hex: String = player_id.iter().map(|b| format!("{:02x}", b)).collect();
    let request_hex: String = request_id.bytes().iter().map(|b| format!("{:02x}", b)).collect();
    let variant = match request_id {
        ArtifactId::Blob(_) => "b",
        ArtifactId::Doc(_) => "d",
    };
    format!("{}:{}:{}", player_hex, variant, request_hex)
}

/// Decode intention data from the encoded string.
/// Returns (player_id, request_id) or None if invalid.
pub fn decode_intention_data(data: &str) -> Option<(PlayerId, ArtifactId)> {
    let parts: Vec<&str> = data.split(':').collect();
    if parts.len() != 3 {
        return None;
    }

    let player_bytes = hex_to_bytes(parts[0])?;
    if player_bytes.len() != 32 {
        return None;
    }
    let mut player_id = [0u8; 32];
    player_id.copy_from_slice(&player_bytes);

    let request_bytes = hex_to_bytes(parts[2])?;
    if request_bytes.len() != 32 {
        return None;
    }
    let mut id_bytes = [0u8; 32];
    id_bytes.copy_from_slice(&request_bytes);

    let request_id = match parts[1] {
        "b" => ArtifactId::Blob(id_bytes),
        "d" => ArtifactId::Doc(id_bytes),
        _ => return None,
    };

    Some((player_id, request_id))
}

fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_code_is_deterministic() {
        let player: PlayerId = [1u8; 32];
        let request = ArtifactId::Doc([2u8; 32]);
        let code1 = generate_intention_code(&player, &request);
        let code2 = generate_intention_code(&player, &request);
        assert_eq!(code1, code2);
        assert!(code1.starts_with("INDRA-"));
        assert_eq!(code1.len(), 15); // INDRA-XXXX-XXXX
    }

    #[test]
    fn encode_decode_roundtrip() {
        let player: PlayerId = [42u8; 32];
        let request = ArtifactId::Doc([99u8; 32]);
        let encoded = encode_intention_data(&player, &request);
        let (dec_player, dec_request) = decode_intention_data(&encoded).unwrap();
        assert_eq!(dec_player, player);
        assert_eq!(dec_request, request);
    }

    #[test]
    fn encode_decode_blob_variant() {
        let player: PlayerId = [7u8; 32];
        let request = ArtifactId::Blob([13u8; 32]);
        let encoded = encode_intention_data(&player, &request);
        assert!(encoded.contains(":b:"));
        let (dec_player, dec_request) = decode_intention_data(&encoded).unwrap();
        assert_eq!(dec_player, player);
        assert_eq!(dec_request, request);
    }
}
