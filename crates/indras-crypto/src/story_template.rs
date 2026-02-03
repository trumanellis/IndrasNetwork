//! Story template engine for pass story authentication.
//!
//! Provides the hero's journey template with 11 stages and 23 slots,
//! using past-tense autobiographical framing.

use crate::error::{CryptoError, CryptoResult};
use crate::pass_story;

/// A single stage of the hero's journey.
#[derive(Debug, Clone)]
pub struct StoryStage {
    /// Stage name (e.g., "The Ordinary World")
    pub name: &'static str,
    /// Brief description (e.g., "where you came from")
    pub description: &'static str,
    /// Template sentence with blanks (e.g., "I grew up in `_____`, where I was a `_____`.")
    pub template: &'static str,
    /// Number of blanks in this stage (1-3)
    pub slot_count: usize,
}

/// The complete hero's journey template (11 stages, 23 slots).
#[derive(Debug, Clone)]
pub struct StoryTemplate {
    /// The 11 stages of the hero's journey.
    pub stages: Vec<StoryStage>,
}

impl StoryTemplate {
    /// Returns the default past-tense autobiographical template.
    pub fn default_template() -> Self {
        Self {
            stages: vec![
                StoryStage {
                    name: "The Ordinary World",
                    description: "before the journey began",
                    template: "In the land of `_____`, I was known among my people as `_____`.",
                    slot_count: 2,
                },
                StoryStage {
                    name: "The Call",
                    description: "the summons that could not be ignored",
                    template: "Then from beyond the horizon, `_____` came bearing `_____`.",
                    slot_count: 2,
                },
                StoryStage {
                    name: "Refusal of the Call",
                    description: "the weight that held you back",
                    template: "I nearly refused the call, bound by my `_____` and haunted by my `_____`.",
                    slot_count: 2,
                },
                StoryStage {
                    name: "Crossing the Threshold",
                    description: "the point of no return",
                    template: "I crossed through the `_____` into the uncharted realm of `_____`.",
                    slot_count: 2,
                },
                StoryStage {
                    name: "The Mentor",
                    description: "the guide who appeared when needed",
                    template: "There a `_____` unveiled the hidden `_____` that had eluded me.",
                    slot_count: 2,
                },
                StoryStage {
                    name: "Tests and Allies",
                    description: "trials that forged new strength",
                    template: "Through many trials I learned to forge `_____` from `_____` and `_____`.",
                    slot_count: 3,
                },
                StoryStage {
                    name: "The Ordeal",
                    description: "the crucible of transformation",
                    template: "In the deepest hour, my `_____` was shattered against `_____`.",
                    slot_count: 2,
                },
                StoryStage {
                    name: "The Reward",
                    description: "what rose from the ashes",
                    template: "From that silence rose a `_____` that whispered of `_____`.",
                    slot_count: 2,
                },
                StoryStage {
                    name: "The Road Back",
                    description: "the long journey home",
                    template: "I bore the `_____` homeward through the vast `_____`.",
                    slot_count: 2,
                },
                StoryStage {
                    name: "Resurrection",
                    description: "the final transformation",
                    template: "Where once I had been a `_____`, I emerged reborn as a `_____`.",
                    slot_count: 2,
                },
                StoryStage {
                    name: "Return with the Elixir",
                    description: "what you carry into the world",
                    template: "Now and forevermore I carry `_____` and `_____`.",
                    slot_count: 2,
                },
            ],
        }
    }

    /// Total number of slots across all stages.
    pub fn total_slots(&self) -> usize {
        self.stages.iter().map(|s| s.slot_count).sum()
    }

    /// Validate that grouped slot values match the template shape.
    ///
    /// `grouped_slots` should have one Vec per stage, with the correct
    /// number of values for that stage.
    pub fn validate_shape(&self, grouped_slots: &[Vec<String>]) -> CryptoResult<()> {
        if grouped_slots.len() != self.stages.len() {
            return Err(CryptoError::SlotCountMismatch {
                expected: self.stages.len(),
                actual: grouped_slots.len(),
            });
        }

        for (i, (stage, slots)) in self.stages.iter().zip(grouped_slots.iter()).enumerate() {
            if slots.len() != stage.slot_count {
                return Err(CryptoError::InvalidStory(format!(
                    "Stage '{}' (#{}) expects {} slots, got {}",
                    stage.name,
                    i + 1,
                    stage.slot_count,
                    slots.len()
                )));
            }
        }

        Ok(())
    }

    /// Get the stage slot boundaries as (start, end) index pairs into the flat slot array.
    pub fn stage_boundaries(&self) -> Vec<(usize, usize)> {
        let mut boundaries = Vec::with_capacity(self.stages.len());
        let mut offset = 0;
        for stage in &self.stages {
            boundaries.push((offset, offset + stage.slot_count));
            offset += stage.slot_count;
        }
        boundaries
    }
}

/// A completed pass story — template + user's slot values.
#[derive(Debug, Clone)]
pub struct PassStory {
    /// The template used.
    pub template: StoryTemplate,
    /// Normalized slot values (always 23).
    pub slots: [String; 23],
}

impl PassStory {
    /// Create from raw user input. Normalizes all slots.
    pub fn from_raw(raw_slots: &[&str; 23]) -> CryptoResult<Self> {
        let template = StoryTemplate::default_template();

        if template.total_slots() != 23 {
            return Err(CryptoError::InvalidStory(format!(
                "Template has {} slots, expected 23",
                template.total_slots()
            )));
        }

        let normalized: Vec<String> = raw_slots.iter().map(|s| pass_story::normalize_slot(s)).collect();

        // Validate no empty slots after normalization
        for (i, slot) in normalized.iter().enumerate() {
            if slot.is_empty() {
                return Err(CryptoError::InvalidStory(format!(
                    "Slot {} is empty after normalization",
                    i + 1
                )));
            }
        }

        let slots: [String; 23] = normalized
            .try_into()
            .map_err(|_| CryptoError::InvalidStory("Failed to collect 23 slots".to_string()))?;

        Ok(Self { template, slots })
    }

    /// Create from pre-normalized slots (skips normalization).
    pub fn from_normalized(slots: [String; 23]) -> CryptoResult<Self> {
        let template = StoryTemplate::default_template();

        for (i, slot) in slots.iter().enumerate() {
            if slot.is_empty() {
                return Err(CryptoError::InvalidStory(format!(
                    "Slot {} is empty",
                    i + 1
                )));
            }
        }

        Ok(Self { template, slots })
    }

    /// Render the full narrative for display.
    ///
    /// Fills in each template sentence with the user's words.
    pub fn render(&self) -> String {
        let mut result = String::new();
        let mut slot_idx = 0;

        for (i, stage) in self.template.stages.iter().enumerate() {
            if i > 0 {
                result.push('\n');
            }

            // Fill in the template
            let mut filled = stage.template.to_string();
            for _ in 0..stage.slot_count {
                if slot_idx < self.slots.len() {
                    filled = filled.replacen("`_____`", &format!("`{}`", &self.slots[slot_idx]), 1);
                    slot_idx += 1;
                }
            }

            result.push_str(&filled);
        }

        result
    }

    /// Get the canonical encoding for KDF input.
    pub fn canonical(&self) -> CryptoResult<Vec<u8>> {
        pass_story::canonical_encode(&self.slots)
    }

    /// Get a reference to the slot values.
    pub fn slots(&self) -> &[String; 23] {
        &self.slots
    }

    /// Get the slots grouped by stage.
    pub fn grouped_slots(&self) -> Vec<Vec<String>> {
        let boundaries = self.template.stage_boundaries();
        boundaries
            .iter()
            .map(|(start, end)| self.slots[*start..*end].to_vec())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_total_slots() {
        let template = StoryTemplate::default_template();
        assert_eq!(template.total_slots(), 23);
    }

    #[test]
    fn test_template_stage_count() {
        let template = StoryTemplate::default_template();
        assert_eq!(template.stages.len(), 11);
    }

    #[test]
    fn test_template_stage_slot_distribution() {
        let template = StoryTemplate::default_template();
        let counts: Vec<usize> = template.stages.iter().map(|s| s.slot_count).collect();
        // 2+2+2+2+2+3+2+2+2+2+2 = 23
        assert_eq!(counts.iter().sum::<usize>(), 23, "Total slots should be 23, got distribution: {:?}", counts);
        assert_eq!(counts, vec![2, 2, 2, 2, 2, 3, 2, 2, 2, 2, 2]);
    }

    #[test]
    fn test_pass_story_from_raw() {
        let raw: [&str; 23] = [
            "static", "collector", "autumn", "clarity",
            "vertigo", "pride", "kitchen", "amaranth",
            "librarian", "telescope", "patience",
            "compass", "silence", "cassiterite", "granite",
            "mercury", "labyrinth", "chrysalis", "horologist",
            "amaranth", "cartographer", "wanderer", "lighthouse",
        ];

        let story = PassStory::from_raw(&raw).unwrap();
        assert_eq!(story.slots.len(), 23);
        assert_eq!(story.slots[0], "static");
        assert_eq!(story.slots[22], "lighthouse");
    }

    #[test]
    fn test_pass_story_render_contains_all_slots() {
        let raw: [&str; 23] = [
            "static", "collector", "autumn", "clarity",
            "vertigo", "pride", "kitchen", "amaranth",
            "librarian", "telescope", "patience",
            "compass", "silence", "cassiterite", "granite",
            "mercury", "labyrinth", "chrysalis", "horologist",
            "amaranth", "cartographer", "wanderer", "lighthouse",
        ];

        let story = PassStory::from_raw(&raw).unwrap();
        let rendered = story.render();

        for slot in &story.slots {
            assert!(
                rendered.contains(slot),
                "Rendered story missing slot: {}",
                slot
            );
        }
    }

    #[test]
    fn test_pass_story_canonical_deterministic() {
        let raw: [&str; 23] = [
            "static", "collector", "autumn", "clarity",
            "vertigo", "pride", "kitchen", "amaranth",
            "librarian", "telescope", "patience",
            "compass", "silence", "cassiterite", "granite",
            "mercury", "labyrinth", "chrysalis", "horologist",
            "amaranth", "cartographer", "wanderer", "lighthouse",
        ];

        let story = PassStory::from_raw(&raw).unwrap();
        let c1 = story.canonical().unwrap();
        let c2 = story.canonical().unwrap();
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_pass_story_empty_slot_rejected() {
        let mut raw: [&str; 23] = [
            "static", "collector", "autumn", "clarity",
            "vertigo", "pride", "kitchen", "amaranth",
            "librarian", "telescope", "patience",
            "compass", "silence", "cassiterite", "granite",
            "mercury", "labyrinth", "chrysalis", "horologist",
            "amaranth", "cartographer", "wanderer", "lighthouse",
        ];
        raw[0] = "";
        let result = PassStory::from_raw(&raw);
        assert!(result.is_err());
    }

    #[test]
    fn test_stage_boundaries() {
        let template = StoryTemplate::default_template();
        let boundaries = template.stage_boundaries();
        assert_eq!(boundaries.len(), 11);
        // First stage starts at 0
        assert_eq!(boundaries[0].0, 0);
        // Last boundary end should equal total slots
        assert_eq!(boundaries.last().unwrap().1, template.total_slots());
    }

    #[test]
    fn test_validate_shape() {
        let template = StoryTemplate::default_template();
        let grouped: Vec<Vec<String>> = template
            .stages
            .iter()
            .map(|s| vec!["word".to_string(); s.slot_count])
            .collect();
        assert!(template.validate_shape(&grouped).is_ok());
    }

    #[test]
    fn test_validate_shape_wrong_stage_count() {
        let template = StoryTemplate::default_template();
        let grouped: Vec<Vec<String>> = vec![vec!["word".to_string(); 2]; 5];
        assert!(template.validate_shape(&grouped).is_err());
    }

    #[test]
    fn test_templates_are_first_person() {
        let template = StoryTemplate::default_template();
        // All templates should be first person narrative
        assert!(template.stages[0].template.starts_with("In the land"));
        assert!(template.stages[1].template.starts_with("Then from"));
        assert!(template.stages[2].template.starts_with("I nearly"));
        assert!(template.stages[3].template.starts_with("I crossed"));
    }
}
