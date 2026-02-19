use crate::artifact::{Artifact, StewardshipRecord};

/// Compute a subjective token value (0.0–1.0) combining:
/// - Attention heat (externally computed, passed in)
/// - Blessing count from the token's blessing history
/// - Steward chain length (number of stewardship transfers)
///
/// Each component is normalized and weighted:
/// - heat: 40% weight (already 0.0–1.0)
/// - blessings: 30% weight (sigmoid, 5 blessings ≈ 0.5)
/// - steward chain: 30% weight (sigmoid, 3 transfers ≈ 0.5)
pub fn compute_token_value(
    token: &Artifact,
    steward_history: &[StewardshipRecord],
    heat: f32,
) -> f32 {
    let heat_component = heat.clamp(0.0, 1.0);

    // Blessing component: sigmoid normalization
    let blessing_count = token.blessing_history.len() as f32;
    let blessing_component = blessing_count / (blessing_count + 5.0);

    // Steward chain component: sigmoid normalization
    let chain_len = steward_history.len() as f32;
    let chain_component = chain_len / (chain_len + 3.0);

    let value = 0.4 * heat_component + 0.3 * blessing_component + 0.3 * chain_component;
    value.clamp(0.0, 1.0)
}
