//! Lua bindings for pass story authentication
//!
//! Provides Lua access to story template, entropy analysis,
//! and key derivation for security testing scenarios.
//!
//! For simulation speed, key derivation uses BLAKE3 instead of Argon2id.
//! The real KDF is tested in Rust unit tests; Lua scenarios care about
//! protocol flow, not KDF strength.

use mlua::{Lua, Result, Table, UserData, UserDataMethods};

use indras_crypto::pass_story::{self, STORY_SLOT_COUNT};
use indras_crypto::story_template::{PassStory, StoryTemplate};
use indras_crypto::{entropy, word_frequencies};

// ---------------------------------------------------------------------------
// Helper: extract a fixed-size [String; 23] from a Lua table
// ---------------------------------------------------------------------------

fn lua_table_to_slots(tbl: &Table) -> Result<[String; STORY_SLOT_COUNT]> {
    let len = tbl.raw_len();
    if len != STORY_SLOT_COUNT {
        return Err(mlua::Error::external(format!(
            "Expected {} slots, got {}",
            STORY_SLOT_COUNT, len
        )));
    }

    let mut slots = Vec::with_capacity(STORY_SLOT_COUNT);
    for i in 1..=STORY_SLOT_COUNT {
        let val: String = tbl.raw_get(i).map_err(|e| {
            mlua::Error::external(format!("Slot {} is not a string: {}", i, e))
        })?;
        slots.push(val);
    }

    slots
        .try_into()
        .map_err(|_| mlua::Error::external("Failed to collect 23 slots"))
}

// ---------------------------------------------------------------------------
// Helper: convert [String; 23] to &[&str; 23] for APIs that need it
// ---------------------------------------------------------------------------

fn slots_as_str_array(slots: &[String; STORY_SLOT_COUNT]) -> [&str; STORY_SLOT_COUNT] {
    core::array::from_fn(|i| slots[i].as_str())
}

// ---------------------------------------------------------------------------
// Simulation-fast key derivation using BLAKE3
// ---------------------------------------------------------------------------

/// Derive 4 x 32-byte subkeys from canonical encoding using BLAKE3.
///
/// This is intentionally NOT Argon2id -- it is a fast deterministic substitute
/// for simulation scenarios. The real KDF lives in `indras_crypto::pass_story`
/// and is tested via Rust unit tests.
fn sim_derive_keys(canonical: &[u8]) -> SimSubkeys {
    let master = blake3::hash(canonical);
    let master_bytes = master.as_bytes();

    // Derive 4 purpose-specific keys via BLAKE3 keyed hash
    let identity = blake3::keyed_hash(master_bytes, b"indras-sim-identity-key-derivatn");
    let encryption = blake3::keyed_hash(master_bytes, b"indras-sim-encryption-key-deriv");
    let signing = blake3::keyed_hash(master_bytes, b"indras-sim-signing-key-derivatn!");
    let recovery = blake3::keyed_hash(master_bytes, b"indras-sim-recovery-key-derivtn");

    SimSubkeys {
        identity: hex::encode(identity.as_bytes()),
        encryption: hex::encode(encryption.as_bytes()),
        signing: hex::encode(signing.as_bytes()),
        recovery: hex::encode(recovery.as_bytes()),
    }
}

struct SimSubkeys {
    identity: String,
    encryption: String,
    signing: String,
    recovery: String,
}

/// Simulation-fast verification token: BLAKE3 of canonical encoding.
fn sim_verification_token(canonical: &[u8]) -> String {
    let hash = blake3::hash(canonical);
    hex::encode(hash.as_bytes())
}

// ---------------------------------------------------------------------------
// LuaPassStory UserData wrapper
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct LuaPassStory {
    inner: PassStory,
}

impl UserData for LuaPassStory {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // render() -> string
        methods.add_method("render", |_, this, ()| Ok(this.inner.render()));

        // canonical() -> hex string
        methods.add_method("canonical", |_, this, ()| {
            let bytes = this
                .inner
                .canonical()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(hex::encode(&bytes))
        });

        // slots() -> table of strings
        methods.add_method("slots", |lua, this, ()| {
            let tbl = lua.create_table()?;
            for (i, slot) in this.inner.slots().iter().enumerate() {
                tbl.raw_set(i + 1, slot.as_str())?;
            }
            Ok(tbl)
        });

        // grouped_slots() -> table of {stage_name, slots=[...]}
        methods.add_method("grouped_slots", |lua, this, ()| {
            let template = StoryTemplate::default_template();
            let grouped = this.inner.grouped_slots();
            let result = lua.create_table()?;

            for (i, (stage, slots)) in template.stages.iter().zip(grouped.iter()).enumerate() {
                let entry = lua.create_table()?;
                entry.set("stage_name", stage.name)?;

                let slots_tbl = lua.create_table()?;
                for (j, slot) in slots.iter().enumerate() {
                    slots_tbl.raw_set(j + 1, slot.as_str())?;
                }
                entry.set("slots", slots_tbl)?;
                result.raw_set(i + 1, entry)?;
            }

            Ok(result)
        });

        // validate() -> bool, string|nil
        methods.add_method("validate", |_, this, ()| {
            let template = StoryTemplate::default_template();
            let grouped = this.inner.grouped_slots();
            match template.validate_shape(&grouped) {
                Ok(()) => Ok((true, None::<String>)),
                Err(e) => Ok((false, Some(e.to_string()))),
            }
        });

        // entropy() -> {total_bits, per_slot, passed_gate}
        methods.add_method("entropy", |lua, this, ()| {
            let (total, per_slot) = entropy::story_entropy(this.inner.slots());
            let passed = entropy::entropy_gate(this.inner.slots()).is_ok();

            let result = lua.create_table()?;
            result.set("total_bits", total)?;
            result.set("passed_gate", passed)?;

            let per_slot_tbl = lua.create_table()?;
            for (i, bits) in per_slot.iter().enumerate() {
                per_slot_tbl.raw_set(i + 1, *bits)?;
            }
            result.set("per_slot", per_slot_tbl)?;

            Ok(result)
        });

        // token() -> hex string (simulation-fast verification token)
        methods.add_method("token", |_, this, ()| {
            let canonical = this
                .inner
                .canonical()
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(sim_verification_token(&canonical))
        });

        // __tostring metamethod
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, ()| {
            let slots = this.inner.slots();
            Ok(format!(
                "PassStory(slots=23, first=\"{}\", last=\"{}\")",
                slots[0], slots[22]
            ))
        });
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Register pass_story bindings under `indras.pass_story`.
pub fn register(lua: &Lua, indras: &Table) -> Result<()> {
    let ps = lua.create_table()?;

    // =========================================================================
    // Template info
    // =========================================================================

    // template() -> {stages=[{name, description, template, slot_count}], total_slots}
    ps.set(
        "template",
        lua.create_function(|lua, ()| {
            let tmpl = StoryTemplate::default_template();
            let result = lua.create_table()?;

            let stages = lua.create_table()?;
            for (i, stage) in tmpl.stages.iter().enumerate() {
                let entry = lua.create_table()?;
                entry.set("name", stage.name)?;
                entry.set("description", stage.description)?;
                entry.set("template", stage.template)?;
                entry.set("slot_count", stage.slot_count)?;
                stages.raw_set(i + 1, entry)?;
            }
            result.set("stages", stages)?;
            result.set("total_slots", tmpl.total_slots())?;

            Ok(result)
        })?,
    )?;

    // template_slot_count() -> 23
    ps.set(
        "template_slot_count",
        lua.create_function(|_, ()| {
            Ok(StoryTemplate::default_template().total_slots())
        })?,
    )?;

    // =========================================================================
    // Slot operations
    // =========================================================================

    // normalize_slot(slot) -> normalized string
    ps.set(
        "normalize_slot",
        lua.create_function(|_, slot: String| Ok(pass_story::normalize_slot(&slot)))?,
    )?;

    // canonical_encode(slots) -> hex string
    ps.set(
        "canonical_encode",
        lua.create_function(|_, tbl: Table| {
            let slots = lua_table_to_slots(&tbl)?;
            let normalized: [String; STORY_SLOT_COUNT] =
                core::array::from_fn(|i| pass_story::normalize_slot(&slots[i]));
            let bytes = pass_story::canonical_encode(&normalized)
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(hex::encode(&bytes))
        })?,
    )?;

    // =========================================================================
    // Key derivation (simulation-fast via BLAKE3)
    // =========================================================================

    // derive_keys(slots) -> {identity_hex, encryption_hex, signing_hex, recovery_hex}
    ps.set(
        "derive_keys",
        lua.create_function(|lua, tbl: Table| {
            let slots = lua_table_to_slots(&tbl)?;
            let normalized: [String; STORY_SLOT_COUNT] =
                core::array::from_fn(|i| pass_story::normalize_slot(&slots[i]));
            let canonical = pass_story::canonical_encode(&normalized)
                .map_err(|e| mlua::Error::external(e.to_string()))?;

            let subkeys = sim_derive_keys(&canonical);

            let result = lua.create_table()?;
            result.set("identity_hex", subkeys.identity)?;
            result.set("encryption_hex", subkeys.encryption)?;
            result.set("signing_hex", subkeys.signing)?;
            result.set("recovery_hex", subkeys.recovery)?;
            Ok(result)
        })?,
    )?;

    // verification_token(slots) -> hex string
    ps.set(
        "verification_token",
        lua.create_function(|_, tbl: Table| {
            let slots = lua_table_to_slots(&tbl)?;
            let normalized: [String; STORY_SLOT_COUNT] =
                core::array::from_fn(|i| pass_story::normalize_slot(&slots[i]));
            let canonical = pass_story::canonical_encode(&normalized)
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(sim_verification_token(&canonical))
        })?,
    )?;

    // =========================================================================
    // Entropy analysis
    // =========================================================================

    // slot_entropy(word, position) -> float (bits)
    ps.set(
        "slot_entropy",
        lua.create_function(|_, (word, position): (String, usize)| {
            Ok(entropy::slot_entropy(&word, position))
        })?,
    )?;

    // story_entropy(slots) -> {total_bits, per_slot=[float...]}
    ps.set(
        "story_entropy",
        lua.create_function(|lua, tbl: Table| {
            let slots = lua_table_to_slots(&tbl)?;
            let (total, per_slot) = entropy::story_entropy(&slots);

            let result = lua.create_table()?;
            result.set("total_bits", total)?;

            let per_slot_tbl = lua.create_table()?;
            for (i, bits) in per_slot.iter().enumerate() {
                per_slot_tbl.raw_set(i + 1, *bits)?;
            }
            result.set("per_slot", per_slot_tbl)?;

            Ok(result)
        })?,
    )?;

    // entropy_gate(slots) -> {passed=bool, total_bits=float, weak_slots=[int...] or nil}
    ps.set(
        "entropy_gate",
        lua.create_function(|lua, tbl: Table| {
            let slots = lua_table_to_slots(&tbl)?;
            let (total, _per_slot) = entropy::story_entropy(&slots);

            let result = lua.create_table()?;
            result.set("total_bits", total)?;

            match entropy::entropy_gate(&slots) {
                Ok(()) => {
                    result.set("passed", true)?;
                }
                Err(e) => {
                    result.set("passed", false)?;
                    // Extract weak slots from the error
                    if let indras_crypto::CryptoError::EntropyBelowThreshold {
                        weak_slots, ..
                    } = e
                    {
                        let weak_tbl = lua.create_table()?;
                        for (i, slot_idx) in weak_slots.iter().enumerate() {
                            // Lua is 1-indexed
                            weak_tbl.raw_set(i + 1, *slot_idx + 1)?;
                        }
                        result.set("weak_slots", weak_tbl)?;
                    }
                }
            }

            Ok(result)
        })?,
    )?;

    // =========================================================================
    // Word frequency helpers (useful for scenarios testing entropy)
    // =========================================================================

    // word_rank(word) -> int or nil
    ps.set(
        "word_rank",
        lua.create_function(|_, word: String| Ok(word_frequencies::word_rank(&word)))?,
    )?;

    // base_entropy(word) -> float
    ps.set(
        "base_entropy",
        lua.create_function(|_, word: String| Ok(word_frequencies::base_entropy(&word)))?,
    )?;

    // =========================================================================
    // Story creation (PassStory UserData)
    // =========================================================================

    let story = lua.create_table()?;

    // Story.from_raw(slots_table) -> PassStory userdata
    story.set(
        "from_raw",
        lua.create_function(|_, tbl: Table| {
            let slots = lua_table_to_slots(&tbl)?;
            let str_slots = slots_as_str_array(&slots);
            let inner = PassStory::from_raw(&str_slots)
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(LuaPassStory { inner })
        })?,
    )?;

    // Story.from_normalized(slots_table) -> PassStory userdata
    story.set(
        "from_normalized",
        lua.create_function(|_, tbl: Table| {
            let slots = lua_table_to_slots(&tbl)?;
            let inner = PassStory::from_normalized(slots)
                .map_err(|e| mlua::Error::external(e.to_string()))?;
            Ok(LuaPassStory { inner })
        })?,
    )?;

    ps.set("Story", story)?;

    // =========================================================================
    // Constants
    // =========================================================================

    ps.set("SLOT_COUNT", STORY_SLOT_COUNT)?;
    ps.set("MIN_ENTROPY_BITS", pass_story::MIN_ENTROPY_BITS)?;

    // Set namespace on indras table
    indras.set("pass_story", ps)?;

    Ok(())
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        let indras = lua.create_table().unwrap();
        register(&lua, &indras).unwrap();
        lua.globals().set("indras", indras).unwrap();
        lua
    }

    #[test]
    fn test_template_info() {
        let lua = setup_lua();

        let (total_slots, stage_count): (usize, usize) = lua
            .load(
                r#"
                local tmpl = indras.pass_story.template()
                return tmpl.total_slots, #tmpl.stages
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(total_slots, 23);
        assert_eq!(stage_count, 11);
    }

    #[test]
    fn test_template_slot_count() {
        let lua = setup_lua();

        let count: usize = lua
            .load("return indras.pass_story.template_slot_count()")
            .eval()
            .unwrap();
        assert_eq!(count, 23);
    }

    #[test]
    fn test_normalize_slot() {
        let lua = setup_lua();

        let result: String = lua
            .load(r#"return indras.pass_story.normalize_slot("  HELLO   World  ")"#)
            .eval()
            .unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_canonical_encode_deterministic() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local slots = {
                    "static", "collector", "autumn", "clarity",
                    "vertigo", "pride", "kitchen", "amaranth",
                    "librarian", "telescope", "patience",
                    "compass", "silence", "cassiterite", "granite",
                    "mercury", "labyrinth", "chrysalis", "horologist",
                    "amaranth", "cartographer", "wanderer", "lighthouse"
                }
                local enc1 = indras.pass_story.canonical_encode(slots)
                local enc2 = indras.pass_story.canonical_encode(slots)
                return enc1 == enc2 and #enc1 > 0
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_derive_keys() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local slots = {
                    "static", "collector", "autumn", "clarity",
                    "vertigo", "pride", "kitchen", "amaranth",
                    "librarian", "telescope", "patience",
                    "compass", "silence", "cassiterite", "granite",
                    "mercury", "labyrinth", "chrysalis", "horologist",
                    "amaranth", "cartographer", "wanderer", "lighthouse"
                }
                local keys = indras.pass_story.derive_keys(slots)
                -- All 4 keys should be 64-char hex strings (32 bytes)
                return #keys.identity_hex == 64
                   and #keys.encryption_hex == 64
                   and #keys.signing_hex == 64
                   and #keys.recovery_hex == 64
                   and keys.identity_hex ~= keys.encryption_hex
                   and keys.signing_hex ~= keys.recovery_hex
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_derive_keys_deterministic() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local slots = {
                    "static", "collector", "autumn", "clarity",
                    "vertigo", "pride", "kitchen", "amaranth",
                    "librarian", "telescope", "patience",
                    "compass", "silence", "cassiterite", "granite",
                    "mercury", "labyrinth", "chrysalis", "horologist",
                    "amaranth", "cartographer", "wanderer", "lighthouse"
                }
                local keys1 = indras.pass_story.derive_keys(slots)
                local keys2 = indras.pass_story.derive_keys(slots)
                return keys1.identity_hex == keys2.identity_hex
                   and keys1.encryption_hex == keys2.encryption_hex
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_derive_keys_case_insensitive() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local lower = {
                    "static", "collector", "autumn", "clarity",
                    "vertigo", "pride", "kitchen", "amaranth",
                    "librarian", "telescope", "patience",
                    "compass", "silence", "cassiterite", "granite",
                    "mercury", "labyrinth", "chrysalis", "horologist",
                    "amaranth", "cartographer", "wanderer", "lighthouse"
                }
                local upper = {
                    "STATIC", "COLLECTOR", "AUTUMN", "CLARITY",
                    "VERTIGO", "PRIDE", "KITCHEN", "AMARANTH",
                    "LIBRARIAN", "TELESCOPE", "PATIENCE",
                    "COMPASS", "SILENCE", "CASSITERITE", "GRANITE",
                    "MERCURY", "LABYRINTH", "CHRYSALIS", "HOROLOGIST",
                    "AMARANTH", "CARTOGRAPHER", "WANDERER", "LIGHTHOUSE"
                }
                local k1 = indras.pass_story.derive_keys(lower)
                local k2 = indras.pass_story.derive_keys(upper)
                return k1.identity_hex == k2.identity_hex
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_verification_token() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local slots = {
                    "static", "collector", "autumn", "clarity",
                    "vertigo", "pride", "kitchen", "amaranth",
                    "librarian", "telescope", "patience",
                    "compass", "silence", "cassiterite", "granite",
                    "mercury", "labyrinth", "chrysalis", "horologist",
                    "amaranth", "cartographer", "wanderer", "lighthouse"
                }
                local tok1 = indras.pass_story.verification_token(slots)
                local tok2 = indras.pass_story.verification_token(slots)
                return tok1 == tok2 and #tok1 == 64
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_slot_entropy() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local common = indras.pass_story.slot_entropy("the", 0)
                local rare   = indras.pass_story.slot_entropy("cassiterite", 0)
                return rare > common and common > 0
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_story_entropy() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local slots = {
                    "cassiterite", "pyrrhic", "amaranth", "horologist",
                    "vermicelli", "cumulonimbus", "astrolabe", "cartographer",
                    "chrysalis", "stalactite", "phosphorescence", "fibonacci",
                    "tessellation", "calligraphy", "obsidian", "quicksilver",
                    "labyrinthine", "bioluminescence", "synesthesia", "perihelion",
                    "soliloquy", "archipelago", "phantasmagoria"
                }
                local result = indras.pass_story.story_entropy(slots)
                return result.total_bits > 200 and #result.per_slot == 23
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_entropy_gate_pass() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local slots = {
                    "cassiterite", "pyrrhic", "amaranth", "horologist",
                    "vermicelli", "cumulonimbus", "astrolabe", "cartographer",
                    "chrysalis", "stalactite", "phosphorescence", "fibonacci",
                    "tessellation", "calligraphy", "obsidian", "quicksilver",
                    "labyrinthine", "bioluminescence", "synesthesia", "perihelion",
                    "soliloquy", "archipelago", "phantasmagoria"
                }
                local gate = indras.pass_story.entropy_gate(slots)
                return gate.passed == true and gate.total_bits > 256
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_entropy_gate_fail() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local slots = {
                    "the", "darkness", "light", "sword", "shadow",
                    "fear", "fire", "hope", "hero", "truth",
                    "path", "sword", "darkness", "light", "fire",
                    "truth", "shadow", "hope", "fear", "hero",
                    "darkness", "light", "sword"
                }
                local gate = indras.pass_story.entropy_gate(slots)
                return gate.passed == false and gate.weak_slots ~= nil
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_story_from_raw() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local slots = {
                    "STATIC", "Collector", "autumn", "clarity",
                    "vertigo", "pride", "kitchen", "amaranth",
                    "librarian", "telescope", "patience",
                    "compass", "silence", "cassiterite", "granite",
                    "mercury", "labyrinth", "chrysalis", "horologist",
                    "amaranth", "cartographer", "wanderer", "lighthouse"
                }
                local story = indras.pass_story.Story.from_raw(slots)
                local s = story:slots()
                -- from_raw normalizes, so first slot should be lowercase
                return s[1] == "static" and s[2] == "collector"
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_story_render() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local slots = {
                    "static", "collector", "autumn", "clarity",
                    "vertigo", "pride", "kitchen", "amaranth",
                    "librarian", "telescope", "patience",
                    "compass", "silence", "cassiterite", "granite",
                    "mercury", "labyrinth", "chrysalis", "horologist",
                    "amaranth", "cartographer", "wanderer", "lighthouse"
                }
                local story = indras.pass_story.Story.from_raw(slots)
                local text = story:render()
                return text:find("static") ~= nil
                   and text:find("lighthouse") ~= nil
                   and text:find("In the land") ~= nil
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_story_grouped_slots() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local slots = {
                    "static", "collector", "autumn", "clarity",
                    "vertigo", "pride", "kitchen", "amaranth",
                    "librarian", "telescope", "patience",
                    "compass", "silence", "cassiterite", "granite",
                    "mercury", "labyrinth", "chrysalis", "horologist",
                    "amaranth", "cartographer", "wanderer", "lighthouse"
                }
                local story = indras.pass_story.Story.from_raw(slots)
                local grouped = story:grouped_slots()
                -- 11 stages
                return #grouped == 11
                   and grouped[1].stage_name == "The Ordinary World"
                   and #grouped[1].slots == 2
                   and grouped[6].stage_name == "Tests and Allies"
                   and #grouped[6].slots == 3
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_story_validate() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local slots = {
                    "static", "collector", "autumn", "clarity",
                    "vertigo", "pride", "kitchen", "amaranth",
                    "librarian", "telescope", "patience",
                    "compass", "silence", "cassiterite", "granite",
                    "mercury", "labyrinth", "chrysalis", "horologist",
                    "amaranth", "cartographer", "wanderer", "lighthouse"
                }
                local story = indras.pass_story.Story.from_raw(slots)
                local ok, err = story:validate()
                return ok == true and err == nil
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_story_entropy_method() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local slots = {
                    "cassiterite", "pyrrhic", "amaranth", "horologist",
                    "vermicelli", "cumulonimbus", "astrolabe", "cartographer",
                    "chrysalis", "stalactite", "phosphorescence", "fibonacci",
                    "tessellation", "calligraphy", "obsidian", "quicksilver",
                    "labyrinthine", "bioluminescence", "synesthesia", "perihelion",
                    "soliloquy", "archipelago", "phantasmagoria"
                }
                local story = indras.pass_story.Story.from_raw(slots)
                local ent = story:entropy()
                return ent.total_bits > 200
                   and #ent.per_slot == 23
                   and ent.passed_gate == true
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_story_token() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local slots = {
                    "static", "collector", "autumn", "clarity",
                    "vertigo", "pride", "kitchen", "amaranth",
                    "librarian", "telescope", "patience",
                    "compass", "silence", "cassiterite", "granite",
                    "mercury", "labyrinth", "chrysalis", "horologist",
                    "amaranth", "cartographer", "wanderer", "lighthouse"
                }
                local story = indras.pass_story.Story.from_raw(slots)
                local tok = story:token()
                -- 32 bytes = 64 hex chars
                return #tok == 64
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_wrong_slot_count_rejected() {
        let lua = setup_lua();

        let result = lua
            .load(
                r#"
                local slots = {"one", "two", "three"}
                return indras.pass_story.derive_keys(slots)
            "#,
            )
            .eval::<mlua::Value>();
        assert!(result.is_err());
    }

    #[test]
    fn test_constants() {
        let lua = setup_lua();

        let (slot_count, min_bits): (usize, f64) = lua
            .load(
                r#"
                return indras.pass_story.SLOT_COUNT, indras.pass_story.MIN_ENTROPY_BITS
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(slot_count, 23);
        assert_eq!(min_bits, 256.0);
    }

    #[test]
    fn test_word_rank() {
        let lua = setup_lua();

        let (rank_the, rank_unknown): (Option<u32>, Option<u32>) = lua
            .load(
                r#"
                return indras.pass_story.word_rank("the"),
                       indras.pass_story.word_rank("cassiterite")
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(rank_the, Some(1));
        assert_eq!(rank_unknown, None);
    }
}
