//! Entropy estimation for pass story authentication.
//!
//! Provides a 3-tier frequency model (base frequency, positional bias,
//! semantic clustering) to estimate the entropy of a user's story
//! and enforce a minimum threshold.

use crate::error::{CryptoError, CryptoResult};
use crate::pass_story::{MIN_ENTROPY_BITS, STORY_SLOT_COUNT};
use crate::word_frequencies;

/// Semantic clustering penalty: reduce entropy when adjacent slots
/// share semantic similarity.
const SEMANTIC_CLUSTER_PENALTY: f64 = 1.5;

/// Minimum per-slot entropy to count toward total (bits).
/// Slots below this are flagged as weak.
const WEAK_SLOT_THRESHOLD: f64 = 6.0;

/// Word pairs that indicate semantic clustering.
/// If both words appear within 3 slots of each other, apply penalty.
const SEMANTIC_CLUSTERS: &[(&[&str], &[&str])] = &[
    (&["light", "bright", "glow", "shine", "radiance", "sun", "dawn", "flame", "fire", "spark"],
     &["dark", "darkness", "shadow", "night", "gloom", "dusk", "shade", "black"]),
    (&["sword", "blade", "knife", "dagger", "weapon", "spear", "axe"],
     &["shield", "armor", "helm", "defense", "guard", "protection"]),
    (&["hope", "dream", "wish", "desire", "faith"],
     &["fear", "doubt", "despair", "dread", "terror", "anxiety"]),
    (&["love", "heart", "passion", "devotion", "affection"],
     &["hate", "anger", "rage", "fury", "wrath"]),
    (&["life", "birth", "grow", "bloom", "spring"],
     &["death", "end", "wither", "decay", "fall", "winter"]),
    (&["water", "river", "ocean", "sea", "lake", "rain", "wave", "tide"],
     &["fire", "flame", "blaze", "ember", "spark", "burn", "ash"]),
    (&["knowledge", "wisdom", "truth", "understanding", "insight"],
     &["ignorance", "blind", "fool", "confusion", "mystery"]),
];

/// Estimate the entropy of a single slot value at a given position.
///
/// Combines base word frequency with positional bias.
pub fn slot_entropy(word: &str, position: usize) -> f64 {
    word_frequencies::positional_entropy(word, position)
}

/// Estimate total story entropy across all 23 slots.
///
/// Returns (total_bits, per_slot_bits) where per_slot_bits contains
/// the entropy estimate for each individual slot.
pub fn story_entropy(slots: &[String; STORY_SLOT_COUNT]) -> (f64, [f64; STORY_SLOT_COUNT]) {
    let mut per_slot = [0.0f64; STORY_SLOT_COUNT];

    // Base + positional entropy
    for (i, slot) in slots.iter().enumerate() {
        per_slot[i] = slot_entropy(slot, i);
    }

    // Semantic clustering penalty
    apply_clustering_penalty(slots, &mut per_slot);

    // Duplicate penalty: if the same word appears in multiple slots, penalize
    apply_duplicate_penalty(slots, &mut per_slot);

    let total: f64 = per_slot.iter().sum();
    (total, per_slot)
}

/// Apply semantic clustering penalty when related words appear near each other.
fn apply_clustering_penalty(slots: &[String; STORY_SLOT_COUNT], per_slot: &mut [f64; STORY_SLOT_COUNT]) {
    for (group_a, group_b) in SEMANTIC_CLUSTERS {
        for i in 0..STORY_SLOT_COUNT {
            let word_i = slots[i].as_str();
            let in_a_i = group_a.contains(&word_i);
            let in_b_i = group_b.contains(&word_i);

            if !in_a_i && !in_b_i {
                continue;
            }

            // Check nearby slots (within 3 positions)
            for j in (i + 1)..STORY_SLOT_COUNT.min(i + 4) {
                let word_j = slots[j].as_str();
                let in_a_j = group_a.contains(&word_j);
                let in_b_j = group_b.contains(&word_j);

                // Penalize if both in same cluster or in paired clusters
                let clustered = (in_a_i && in_a_j) || (in_b_i && in_b_j) || (in_a_i && in_b_j) || (in_b_i && in_a_j);

                if clustered {
                    // Penalize the later slot (preserve entropy of first occurrence)
                    per_slot[j] = (per_slot[j] - SEMANTIC_CLUSTER_PENALTY).max(word_frequencies::MIN_WORD_ENTROPY);
                }
            }
        }
    }
}

/// Apply penalty for duplicate words across slots.
fn apply_duplicate_penalty(slots: &[String; STORY_SLOT_COUNT], per_slot: &mut [f64; STORY_SLOT_COUNT]) {
    for i in 0..STORY_SLOT_COUNT {
        for j in (i + 1)..STORY_SLOT_COUNT {
            if slots[i] == slots[j] {
                // Duplicate: the second occurrence adds no entropy
                per_slot[j] = word_frequencies::MIN_WORD_ENTROPY;
            }
        }
    }
}

/// Check if a story meets the minimum entropy threshold.
///
/// Returns Ok(()) if the story passes the entropy gate.
/// Returns Err with the indices of weak slots if it fails.
pub fn entropy_gate(slots: &[String; STORY_SLOT_COUNT]) -> CryptoResult<()> {
    let (total, per_slot) = story_entropy(slots);

    if total >= MIN_ENTROPY_BITS {
        return Ok(());
    }

    // Identify weak slots (below threshold)
    let weak_slots: Vec<usize> = per_slot
        .iter()
        .enumerate()
        .filter(|(_, entropy)| **entropy < WEAK_SLOT_THRESHOLD)
        .map(|(i, _)| i)
        .collect();

    Err(CryptoError::EntropyBelowThreshold {
        total,
        required: MIN_ENTROPY_BITS,
        weak_slots,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_slots(words: &[&str; STORY_SLOT_COUNT]) -> [String; STORY_SLOT_COUNT] {
        core::array::from_fn(|i| words[i].to_string())
    }

    #[test]
    fn test_common_words_rejected() {
        // 23 very common words should be rejected
        let common = make_slots(&[
            "the", "darkness", "light", "sword", "shadow",
            "fear", "fire", "hope", "hero", "truth",
            "path", "sword", "darkness", "light", "fire",
            "truth", "shadow", "hope", "fear", "hero",
            "darkness", "light", "sword",
        ]);

        let result = entropy_gate(&common);
        assert!(result.is_err(), "23 common words should fail entropy gate");
    }

    #[test]
    fn test_rare_words_accepted() {
        // 23 rare/unique words should pass easily
        let rare = make_slots(&[
            "cassiterite", "pyrrhic", "amaranth", "horologist",
            "vermicelli", "cumulonimbus", "astrolabe", "cartographer",
            "chrysalis", "stalactite", "phosphorescence", "fibonacci",
            "tessellation", "calligraphy", "obsidian", "quicksilver",
            "labyrinthine", "bioluminescence", "synesthesia", "perihelion",
            "soliloquy", "archipelago", "phantasmagoria",
        ]);

        let result = entropy_gate(&rare);
        assert!(result.is_ok(), "23 rare words should pass entropy gate: {:?}", result);
    }

    #[test]
    fn test_mixed_words_can_pass() {
        // Mix of common and rare - should pass if enough rare words
        let mixed = make_slots(&[
            "cassiterite", "collector", "amaranth", "clarity",
            "vertigo", "trepidation", "kitchen", "phosphorescence",
            "calligraphy", "telescope", "chrysalis",
            "obsidian", "labyrinthine", "astrolabe", "quicksilver",
            "bioluminescence", "tessellation", "fibonacci", "horologist",
            "perihelion", "cartographer", "synesthesia", "archipelago",
        ]);

        let result = entropy_gate(&mixed);
        assert!(result.is_ok(), "Mixed words with enough rare should pass: {:?}", result);
    }

    #[test]
    fn test_all_identical_rejected() {
        let identical = make_slots(&["darkness"; STORY_SLOT_COUNT]);
        let result = entropy_gate(&identical);
        assert!(result.is_err(), "23 identical words should fail");
    }

    #[test]
    fn test_semantic_clustering_reduces_entropy() {
        let unclustered = make_slots(&[
            "cassiterite", "pyrrhic", "amaranth", "horologist",
            "vermicelli", "cumulonimbus", "astrolabe", "cartographer",
            "chrysalis", "stalactite", "phosphorescence", "fibonacci",
            "tessellation", "calligraphy", "obsidian", "quicksilver",
            "labyrinthine", "bioluminescence", "synesthesia", "perihelion",
            "soliloquy", "archipelago", "phantasmagoria",
        ]);

        let clustered = make_slots(&[
            "light", "dark", "fire", "water",
            "hope", "fear", "sword", "shield",
            "life", "death", "love", "hate",
            "dawn", "dusk", "bright", "shadow",
            "blade", "armor", "dream", "dread",
            "sun", "night", "flame",
        ]);

        let (unclustered_total, _) = story_entropy(&unclustered);
        let (clustered_total, _) = story_entropy(&clustered);

        assert!(
            clustered_total < unclustered_total,
            "Clustered words should have less entropy: {} vs {}",
            clustered_total,
            unclustered_total
        );
    }

    #[test]
    fn test_entropy_gate_returns_weak_slots() {
        let mostly_common = make_slots(&[
            "the", "darkness", "light", "sword", "shadow",
            "fear", "fire", "hope", "hero", "truth",
            "path", "cassiterite", "pyrrhic", "amaranth", "horologist",
            "vermicelli", "cumulonimbus", "astrolabe", "cartographer",
            "chrysalis", "stalactite", "phosphorescence", "fibonacci",
        ]);

        let (_, per_slot) = story_entropy(&mostly_common);

        // First few slots (common words) should have lower entropy
        assert!(per_slot[0] < per_slot[11]);
    }

    #[test]
    fn test_slot_entropy_range() {
        // Every slot entropy should be positive
        for i in 0..STORY_SLOT_COUNT {
            let entropy = slot_entropy("test", i);
            assert!(entropy > 0.0, "Entropy should be positive for slot {}", i);
        }
    }

    #[test]
    fn test_duplicate_penalty() {
        let with_dupes = make_slots(&[
            "cassiterite", "cassiterite", "cassiterite", "cassiterite",
            "cassiterite", "cassiterite", "cassiterite", "cassiterite",
            "cassiterite", "cassiterite", "cassiterite", "cassiterite",
            "cassiterite", "cassiterite", "cassiterite", "cassiterite",
            "cassiterite", "cassiterite", "cassiterite", "cassiterite",
            "cassiterite", "cassiterite", "cassiterite",
        ]);

        let (total, _) = story_entropy(&with_dupes);
        // All duplicates: should be very low entropy
        assert!(total < MIN_ENTROPY_BITS, "All duplicates should be below threshold: {}", total);
    }
}
