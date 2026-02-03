//! Word frequency data for entropy estimation.
//!
//! Provides base frequency information for common English words.
//! Words not in the database are assumed rare (high entropy).

use std::collections::HashMap;
use std::sync::LazyLock;

/// Default entropy for unknown words (bits).
/// Words not in our frequency database are assumed rare.
pub const UNKNOWN_WORD_ENTROPY: f64 = 16.0;

/// Minimum entropy for any word (even the most common).
pub const MIN_WORD_ENTROPY: f64 = 1.0;

/// Maximum base entropy from frequency alone.
pub const MAX_WORD_ENTROPY: f64 = 20.0;

// Static word frequency map: word -> approximate rank (lower = more common)
static WORD_FREQUENCIES: LazyLock<HashMap<&'static str, u32>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    // Top ~1000 most common English words with approximate ranks
    // Rank 1-10: extremely common
    // Format: (word, rank) where lower rank = more common

    // Articles, prepositions, common words (rank 1-50)
    let words: &[(&str, u32)] = &[
        ("the", 1), ("be", 2), ("to", 3), ("of", 4), ("and", 5),
        ("a", 6), ("in", 7), ("that", 8), ("have", 9), ("i", 10),
        ("it", 11), ("for", 12), ("not", 13), ("on", 14), ("with", 15),
        ("he", 16), ("as", 17), ("you", 18), ("do", 19), ("at", 20),
        ("this", 21), ("but", 22), ("his", 23), ("by", 24), ("from", 25),
        ("they", 26), ("we", 27), ("say", 28), ("her", 29), ("she", 30),
        ("or", 31), ("an", 32), ("will", 33), ("my", 34), ("one", 35),
        ("all", 36), ("would", 37), ("there", 38), ("their", 39), ("what", 40),
        // Common nouns/verbs (rank 41-200)
        ("up", 41), ("out", 42), ("if", 43), ("about", 44), ("who", 45),
        ("get", 46), ("which", 47), ("go", 48), ("me", 49), ("when", 50),
        ("make", 51), ("can", 52), ("like", 53), ("time", 54), ("no", 55),
        ("just", 56), ("him", 57), ("know", 58), ("take", 59), ("people", 60),
        ("into", 61), ("year", 62), ("your", 63), ("good", 64), ("some", 65),
        ("could", 66), ("them", 67), ("see", 68), ("other", 69), ("than", 70),
        ("then", 71), ("now", 72), ("look", 73), ("only", 74), ("come", 75),
        ("its", 76), ("over", 77), ("think", 78), ("also", 79), ("back", 80),
        ("after", 81), ("use", 82), ("two", 83), ("how", 84), ("our", 85),
        ("work", 86), ("first", 87), ("well", 88), ("way", 89), ("even", 90),
        ("new", 91), ("want", 92), ("because", 93), ("any", 94), ("these", 95),
        ("give", 96), ("day", 97), ("most", 98), ("us", 99), ("great", 100),
        // Narrative-relevant common words (rank 100-500)
        ("life", 101), ("man", 102), ("world", 103), ("hand", 104), ("part", 105),
        ("child", 106), ("eye", 107), ("woman", 108), ("place", 109), ("find", 110),
        ("thing", 111), ("tell", 112), ("night", 113), ("home", 114), ("head", 115),
        ("heart", 116), ("old", 117), ("big", 118), ("long", 119), ("high", 120),
        ("small", 121), ("house", 122), ("water", 123), ("keep", 124), ("body", 125),
        ("turn", 126), ("face", 127), ("door", 128), ("name", 129), ("room", 130),
        ("end", 131), ("play", 132), ("move", 133), ("light", 134), ("down", 135),
        ("point", 136), ("city", 137), ("run", 138), ("change", 139), ("story", 140),
        ("father", 141), ("mother", 142), ("earth", 143), ("side", 144), ("begin", 145),
        ("power", 146), ("live", 147), ("land", 148), ("learn", 149), ("school", 150),
        ("air", 151), ("friend", 152), ("family", 153), ("love", 154), ("road", 155),
        ("word", 156), ("book", 157), ("war", 158), ("young", 159), ("line", 160),
        ("left", 161), ("walk", 162), ("need", 163), ("death", 164), ("far", 165),
        ("king", 166), ("tree", 167), ("food", 168), ("dark", 169), ("fire", 170),
        ("fear", 171), ("hope", 172), ("dream", 173), ("mountain", 174), ("river", 175),
        ("sun", 176), ("sea", 177), ("star", 178), ("sword", 179), ("stone", 180),
        ("music", 181), ("voice", 182), ("song", 183), ("gold", 184), ("wind", 185),
        ("sleep", 186), ("rain", 187), ("white", 188), ("black", 189), ("red", 190),
        ("blood", 191), ("garden", 192), ("god", 193), ("open", 194), ("fall", 195),
        ("hour", 196), ("lost", 197), ("true", 198), ("force", 199), ("ground", 200),
        // Common narrative archetypes that will appear in stories (rank 200-500)
        ("shadow", 201), ("silence", 202), ("darkness", 203), ("strength", 204),
        ("path", 205), ("journey", 206), ("spirit", 207), ("soul", 208),
        ("truth", 209), ("wisdom", 210), ("courage", 211), ("magic", 212),
        ("monster", 213), ("hero", 214), ("guide", 215), ("bridge", 216),
        ("forest", 217), ("tower", 218), ("wall", 219), ("mirror", 220),
        ("dragon", 221), ("shield", 222), ("armor", 223), ("battle", 224),
        ("kingdom", 225), ("castle", 226), ("village", 227), ("storm", 228),
        ("flame", 229), ("ice", 230), ("iron", 231), ("silver", 232),
        ("secret", 233), ("treasure", 234), ("key", 235), ("gate", 236),
        ("window", 237), ("child", 238), ("brother", 239), ("sister", 240),
        ("teacher", 241), ("master", 242), ("student", 243), ("warrior", 244),
        ("stranger", 245), ("friend", 246), ("enemy", 247), ("ghost", 248),
        ("angel", 249), ("devil", 250), ("beast", 251), ("wolf", 252),
        ("bird", 253), ("snake", 254), ("fish", 255), ("horse", 256),
        ("cat", 257), ("dog", 258), ("bear", 259), ("lion", 260),
        ("ocean", 261), ("lake", 262), ("island", 263), ("desert", 264),
        ("sky", 265), ("moon", 266), ("cloud", 267), ("thunder", 268),
        ("cave", 269), ("dust", 270), ("ash", 271), ("bone", 272),
        ("steel", 273), ("glass", 274), ("wood", 275), ("silk", 276),
        ("music", 277), ("dance", 278), ("sing", 279), ("cry", 280),
        ("laugh", 281), ("smile", 282), ("anger", 283), ("sorrow", 284),
        ("joy", 285), ("peace", 286), ("pain", 287), ("wound", 288),
        ("heal", 289), ("break", 290), ("build", 291), ("create", 292),
        ("destroy", 293), ("remember", 294), ("forget", 295), ("promise", 296),
        ("betray", 297), ("trust", 298), ("faith", 299), ("doubt", 300),
        // More moderate frequency words (300-600)
        ("compass", 301), ("lantern", 302), ("candle", 303), ("rope", 304),
        ("map", 305), ("knife", 306), ("crown", 307), ("ring", 308),
        ("chain", 309), ("bell", 310), ("clock", 311), ("wheel", 312),
        ("basket", 313), ("bread", 314), ("wine", 315), ("honey", 316),
        ("salt", 317), ("copper", 318), ("bronze", 319), ("marble", 320),
        ("crystal", 321), ("emerald", 322), ("ruby", 323), ("pearl", 324),
        ("diamond", 325), ("sapphire", 326), ("anchor", 327), ("lighthouse", 328),
        ("harbor", 329), ("tide", 330), ("wave", 331), ("shore", 332),
        ("cliff", 333), ("valley", 334), ("meadow", 335), ("orchard", 336),
        ("harvest", 337), ("winter", 338), ("spring", 339), ("summer", 340),
        ("autumn", 341), ("frost", 342), ("snow", 343), ("fog", 344),
        ("ember", 345), ("spark", 346), ("blaze", 347), ("torch", 348),
        ("furnace", 349), ("forge", 350), ("hammer", 351), ("anvil", 352),
        ("needle", 353), ("thread", 354), ("loom", 355), ("cloth", 356),
        ("ink", 357), ("pen", 358), ("scroll", 359), ("letter", 360),
        ("library", 361), ("cathedral", 362), ("temple", 363), ("altar", 364),
        ("throne", 365), ("scepter", 366), ("banner", 367), ("flag", 368),
        ("drum", 369), ("flute", 370), ("harp", 371), ("violin", 372),
        ("piano", 373), ("guitar", 374), ("trumpet", 375), ("whistle", 376),
        ("echo", 377), ("riddle", 378), ("puzzle", 379), ("maze", 380),
        ("labyrinth", 381), ("spiral", 382), ("circle", 383), ("square", 384),
        ("triangle", 385), ("arrow", 386), ("spear", 387), ("bow", 388),
        ("dagger", 389), ("axe", 390), ("helm", 391), ("cloak", 392),
        ("boots", 393), ("gloves", 394), ("mask", 395), ("veil", 396),
        ("raven", 397), ("owl", 398), ("hawk", 399), ("eagle", 400),
        ("falcon", 401), ("dove", 402), ("sparrow", 403), ("swan", 404),
        ("fox", 405), ("deer", 406), ("rabbit", 407), ("spider", 408),
        ("butterfly", 409), ("moth", 410), ("beetle", 411), ("bee", 412),
        ("rose", 413), ("lily", 414), ("oak", 415), ("willow", 416),
        ("ivy", 417), ("thorn", 418), ("root", 419), ("seed", 420),
        ("bloom", 421), ("wither", 422), ("moss", 423), ("fern", 424),
        ("mushroom", 425), ("poison", 426), ("remedy", 427), ("potion", 428),
        ("elixir", 429), ("alchemy", 430), ("science", 431), ("philosophy", 432),
        ("astronomy", 433), ("wanderer", 434), ("pilgrim", 435), ("traveler", 436),
        ("exile", 437), ("refugee", 438), ("orphan", 439), ("thief", 440),
        ("merchant", 441), ("sailor", 442), ("farmer", 443), ("baker", 444),
        ("smith", 445), ("weaver", 446), ("potter", 447), ("painter", 448),
        ("sculptor", 449), ("poet", 450), ("singer", 451), ("dancer", 452),
        ("healer", 453), ("scholar", 454), ("priest", 455), ("monk", 456),
        ("knight", 457), ("soldier", 458), ("hunter", 459), ("shepherd", 460),
        ("guard", 461), ("scout", 462), ("spy", 463), ("judge", 464),
        ("prophet", 465), ("witch", 466), ("wizard", 467), ("sorcerer", 468),
        ("enchantment", 469), ("spell", 470), ("curse", 471), ("blessing", 472),
        ("miracle", 473), ("destiny", 474), ("fate", 475), ("fortune", 476),
        ("sacrifice", 477), ("redemption", 478), ("forgiveness", 479), ("revenge", 480),
        ("honor", 481), ("glory", 482), ("shame", 483), ("pride", 484),
        ("humility", 485), ("patience", 486), ("persistence", 487), ("resilience", 488),
        ("innocence", 489), ("guilt", 490), ("mercy", 491), ("justice", 492),
        ("freedom", 493), ("prison", 494), ("escape", 495), ("return", 496),
        ("beginning", 497), ("ending", 498), ("threshold", 499), ("crossing", 500),
    ];

    for &(word, rank) in words {
        m.insert(word, rank);
    }
    m
});

/// Get the frequency rank of a word (lower = more common).
/// Returns None for unknown words.
pub fn word_rank(word: &str) -> Option<u32> {
    WORD_FREQUENCIES.get(word).copied()
}

/// Estimate base entropy for a word based on frequency.
///
/// Common words get low entropy. Unknown words get high entropy (16 bits).
/// The formula: entropy = log2(rank * SCALE_FACTOR), clamped to [MIN, MAX].
pub fn base_entropy(word: &str) -> f64 {
    match word_rank(word) {
        Some(rank) => {
            // Scale: rank 1 ~ 1 bit, rank 100 ~ 6.6 bits, rank 500 ~ 9 bits
            let entropy = (rank as f64).log2() + 1.0;
            entropy.clamp(MIN_WORD_ENTROPY, MAX_WORD_ENTROPY)
        }
        None => UNKNOWN_WORD_ENTROPY,
    }
}

/// Positional bias categories for template slots.
///
/// Certain words are more likely at certain positions. This provides
/// an entropy penalty (reduction) for expected words in expected positions.
pub fn positional_entropy(word: &str, slot_position: usize) -> f64 {
    let base = base_entropy(word);

    // Define position-specific common words that get penalized
    let penalty = match slot_position {
        // The Ordinary World (slots 0-1): "darkness", "shadow", "village", "child"
        0 => match word {
            "darkness" | "shadow" | "home" | "village" | "city" | "town" | "house" | "farm" | "world" => 2.0,
            _ => 0.0,
        },
        1 => match word {
            "child" | "boy" | "girl" | "dreamer" | "nobody" | "stranger" | "orphan" | "student" => 2.0,
            _ => 0.0,
        },
        // The Call (slots 2-3)
        2 => match word {
            "stranger" | "message" | "letter" | "dream" | "voice" | "fate" | "destiny" | "death" => 1.5,
            _ => 0.0,
        },
        3 => match word {
            "hope" | "change" | "truth" | "light" | "knowledge" | "power" | "freedom" | "purpose" => 1.5,
            _ => 0.0,
        },
        // Refusal (slots 4-5)
        4 | 5 => match word {
            "fear" | "doubt" | "weakness" | "pride" | "shame" | "guilt" | "pain" | "loss" | "anger" => 2.0,
            _ => 0.0,
        },
        // Crossing (slots 6-7)
        6 => match word {
            "door" | "gate" | "bridge" | "path" | "road" | "threshold" | "window" | "portal" => 2.0,
            _ => 0.0,
        },
        7 => match word {
            "darkness" | "unknown" | "wilderness" | "forest" | "desert" | "city" | "light" | "world" => 1.5,
            _ => 0.0,
        },
        // Mentor (slots 8-9)
        8 => match word {
            "teacher" | "master" | "wizard" | "stranger" | "elder" | "sage" | "guide" | "woman" | "man" => 2.0,
            _ => 0.0,
        },
        9 => match word {
            "truth" | "path" | "way" | "light" | "strength" | "power" | "wisdom" | "secret" => 1.5,
            _ => 0.0,
        },
        // Tests and Allies (slots 10-12)
        10 | 11 | 12 => match word {
            "sword" | "shield" | "weapon" | "tool" | "fire" | "strength" | "friend" | "ally" | "trust" | "courage" => 1.0,
            _ => 0.0,
        },
        // Ordeal (slots 13-14)
        13 => match word {
            "sword" | "shield" | "heart" | "hope" | "faith" | "trust" | "courage" | "spirit" | "will" => 1.5,
            _ => 0.0,
        },
        14 => match word {
            "darkness" | "death" | "evil" | "fear" | "silence" | "nothing" | "stone" | "truth" => 1.5,
            _ => 0.0,
        },
        // Reward (slots 15-16)
        15 => match word {
            "sword" | "key" | "light" | "crystal" | "treasure" | "crown" | "gem" | "stone" | "gift" => 1.5,
            _ => 0.0,
        },
        16 => match word {
            "hope" | "truth" | "freedom" | "light" | "power" | "peace" | "love" | "life" | "joy" => 1.5,
            _ => 0.0,
        },
        // Road Back (slots 17-18)
        17 | 18 => match word {
            "light" | "truth" | "treasure" | "knowledge" | "path" | "road" | "darkness" | "fire" => 1.0,
            _ => 0.0,
        },
        // Resurrection (slots 19-20)
        19 => match word {
            "child" | "boy" | "girl" | "fool" | "coward" | "nobody" | "stranger" | "shadow" => 1.5,
            _ => 0.0,
        },
        20 => match word {
            "hero" | "warrior" | "king" | "queen" | "master" | "healer" | "leader" | "sage" => 1.5,
            _ => 0.0,
        },
        // Return with the Elixir (slots 21-22)
        21 | 22 => match word {
            "light" | "truth" | "wisdom" | "hope" | "love" | "peace" | "story" | "memory" | "knowledge" => 1.0,
            _ => 0.0,
        },
        _ => 0.0,
    };

    // Apply penalty: reduce entropy for expected words at expected positions
    (base - penalty).max(MIN_WORD_ENTROPY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_words_low_entropy() {
        assert!(base_entropy("the") < 3.0);
        assert!(base_entropy("light") < 9.0);
        assert!(base_entropy("darkness") < 10.0);
        assert!(base_entropy("sword") < 10.0);
    }

    #[test]
    fn test_unknown_words_high_entropy() {
        assert_eq!(base_entropy("cassiterite"), UNKNOWN_WORD_ENTROPY);
        assert_eq!(base_entropy("pyrrhic"), UNKNOWN_WORD_ENTROPY);
        assert_eq!(base_entropy("vermicelli"), UNKNOWN_WORD_ENTROPY);
        assert_eq!(base_entropy("cumulonimbus"), UNKNOWN_WORD_ENTROPY);
    }

    #[test]
    fn test_positional_penalty() {
        // "fear" in Refusal slot (4) should have lower entropy than in Reward slot (15)
        let fear_refusal = positional_entropy("fear", 4);
        let fear_reward = positional_entropy("fear", 15);
        assert!(fear_refusal < fear_reward);
    }

    #[test]
    fn test_word_rank_known() {
        assert_eq!(word_rank("the"), Some(1));
        assert_eq!(word_rank("light"), Some(134));
    }

    #[test]
    fn test_word_rank_unknown() {
        assert_eq!(word_rank("cassiterite"), None);
    }

    #[test]
    fn test_entropy_bounds() {
        // Every word should have entropy in [MIN, MAX] or UNKNOWN
        for entropy in [base_entropy("the"), base_entropy("light"), base_entropy("unknown_xyz")] {
            assert!(entropy >= MIN_WORD_ENTROPY);
            assert!(entropy <= MAX_WORD_ENTROPY || entropy == UNKNOWN_WORD_ENTROPY);
        }
    }
}
