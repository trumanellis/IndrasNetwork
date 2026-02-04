//! Bioregional hierarchy catalog based on OneEarth's bioregional model.
//!
//! This module provides a compiled-in catalog of the complete bioregional hierarchy:
//! Root (1) → Realms (14) → Subrealms (~52) → Bioregions (185).
//!
//! Ecoregions (844) are not included in the static catalog — use the
//! `scripts/collect_ecoregions.py` scraper to generate ecoregion entries.
//!
//! # Usage
//!
//! ```
//! use indras_sync_engine::bioregion_catalog::BioregionalCatalog;
//!
//! let catalog = BioregionalCatalog::global();
//! let entry = catalog.get("AT1").unwrap();
//! assert_eq!(entry.name, "East African Montane Forests & Woodlands");
//!
//! let path = catalog.path_to_root("AT1");
//! // AT1 -> afrotropics/southern -> afrotropics -> ROOT
//! assert_eq!(path.len(), 4);
//! ```

use crate::humanness::BioregionalLevel;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// A single entry in the bioregional catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogEntry {
    /// Unique identifier: "ROOT", "afrotropics", "afrotropics/southern", "AT1", etc.
    pub code: &'static str,
    /// Human-readable name.
    pub name: &'static str,
    /// Level in the hierarchy.
    pub level: BioregionalLevel,
    /// Parent entry's code. Empty string for root.
    pub parent_code: &'static str,
}

// ============================================================================
// STATIC CATALOG DATA
// ============================================================================

/// The complete bioregional catalog as a compile-time constant.
///
/// 1 root + 14 realms + 52 subrealms + 185 bioregions = 252 entries.
pub const CATALOG: &[CatalogEntry] = &[
    // ── Root ──────────────────────────────────────────────────────────────
    CatalogEntry { code: "ROOT", name: "Temples of Refuge", level: BioregionalLevel::Root, parent_code: "" },

    // ══════════════════════════════════════════════════════════════════════
    // REALM 1: Subarctic America
    // ══════════════════════════════════════════════════════════════════════
    CatalogEntry { code: "subarctic-america", name: "Subarctic America", level: BioregionalLevel::Realm, parent_code: "ROOT" },

    // Subrealms
    CatalogEntry { code: "subarctic-america/greenland", name: "Greenland", level: BioregionalLevel::Subrealm, parent_code: "subarctic-america" },
    CatalogEntry { code: "subarctic-america/canadian-tundra", name: "Canadian Tundra", level: BioregionalLevel::Subrealm, parent_code: "subarctic-america" },
    CatalogEntry { code: "subarctic-america/canadian-boreal", name: "Canadian Boreal", level: BioregionalLevel::Subrealm, parent_code: "subarctic-america" },
    CatalogEntry { code: "subarctic-america/alaska", name: "Alaska", level: BioregionalLevel::Subrealm, parent_code: "subarctic-america" },

    // Bioregions NA1–NA9
    CatalogEntry { code: "NA1", name: "Greenland Tundra & Ice Sheets", level: BioregionalLevel::Bioregion, parent_code: "subarctic-america/greenland" },
    CatalogEntry { code: "NA2", name: "Canadian Low Arctic Tundra", level: BioregionalLevel::Bioregion, parent_code: "subarctic-america/canadian-tundra" },
    CatalogEntry { code: "NA3", name: "Canadian High Arctic Tundra", level: BioregionalLevel::Bioregion, parent_code: "subarctic-america/canadian-tundra" },
    CatalogEntry { code: "NA4", name: "Hudson Bay Lowlands", level: BioregionalLevel::Bioregion, parent_code: "subarctic-america/canadian-tundra" },
    CatalogEntry { code: "NA5", name: "Eastern Canadian Boreal Shield", level: BioregionalLevel::Bioregion, parent_code: "subarctic-america/canadian-boreal" },
    CatalogEntry { code: "NA6", name: "Central Canadian Boreal Shield", level: BioregionalLevel::Bioregion, parent_code: "subarctic-america/canadian-boreal" },
    CatalogEntry { code: "NA7", name: "Western Canadian Boreal", level: BioregionalLevel::Bioregion, parent_code: "subarctic-america/canadian-boreal" },
    CatalogEntry { code: "NA8", name: "Alaska Tundra & Boreal", level: BioregionalLevel::Bioregion, parent_code: "subarctic-america/alaska" },
    CatalogEntry { code: "NA9", name: "Aleutian Islands & Alaska Peninsula", level: BioregionalLevel::Bioregion, parent_code: "subarctic-america/alaska" },

    // ══════════════════════════════════════════════════════════════════════
    // REALM 2: Northern America
    // ══════════════════════════════════════════════════════════════════════
    CatalogEntry { code: "northern-america", name: "Northern America", level: BioregionalLevel::Realm, parent_code: "ROOT" },

    // Subrealms
    CatalogEntry { code: "northern-america/great-plains", name: "Great Plains", level: BioregionalLevel::Subrealm, parent_code: "northern-america" },
    CatalogEntry { code: "northern-america/american-west", name: "American West", level: BioregionalLevel::Subrealm, parent_code: "northern-america" },
    CatalogEntry { code: "northern-america/north-pacific-coast", name: "North Pacific Coast", level: BioregionalLevel::Subrealm, parent_code: "northern-america" },
    CatalogEntry { code: "northern-america/northeast-forests", name: "Northeast Forests", level: BioregionalLevel::Subrealm, parent_code: "northern-america" },
    CatalogEntry { code: "northern-america/southeast-us", name: "Southeast US", level: BioregionalLevel::Subrealm, parent_code: "northern-america" },
    CatalogEntry { code: "northern-america/mexican-drylands", name: "Mexican Drylands", level: BioregionalLevel::Subrealm, parent_code: "northern-america" },

    // Bioregions NA10–NA31
    CatalogEntry { code: "NA10", name: "Northern Tallgrass Prairie", level: BioregionalLevel::Bioregion, parent_code: "northern-america/great-plains" },
    CatalogEntry { code: "NA11", name: "Central Tallgrass Prairie", level: BioregionalLevel::Bioregion, parent_code: "northern-america/great-plains" },
    CatalogEntry { code: "NA12", name: "Northern Mixed & Shortgrass Prairie", level: BioregionalLevel::Bioregion, parent_code: "northern-america/great-plains" },
    CatalogEntry { code: "NA13", name: "Southern Mixed & Shortgrass Prairie", level: BioregionalLevel::Bioregion, parent_code: "northern-america/great-plains" },
    CatalogEntry { code: "NA14", name: "Columbia Plateau & Great Basin", level: BioregionalLevel::Bioregion, parent_code: "northern-america/american-west" },
    CatalogEntry { code: "NA15", name: "Colorado Plateau & Sonoran Desert", level: BioregionalLevel::Bioregion, parent_code: "northern-america/american-west" },
    CatalogEntry { code: "NA16", name: "Northern Rocky Mountain Forests", level: BioregionalLevel::Bioregion, parent_code: "northern-america/american-west" },
    CatalogEntry { code: "NA17", name: "Southern Rocky Mountain Forests", level: BioregionalLevel::Bioregion, parent_code: "northern-america/american-west" },
    CatalogEntry { code: "NA18", name: "Sierra Nevada & California Chaparral", level: BioregionalLevel::Bioregion, parent_code: "northern-america/american-west" },
    CatalogEntry { code: "NA19", name: "Pacific Northwest Coastal Forests", level: BioregionalLevel::Bioregion, parent_code: "northern-america/north-pacific-coast" },
    CatalogEntry { code: "NA20", name: "British Columbia Coastal Forests", level: BioregionalLevel::Bioregion, parent_code: "northern-america/north-pacific-coast" },
    CatalogEntry { code: "NA21", name: "Great Lakes Forests", level: BioregionalLevel::Bioregion, parent_code: "northern-america/northeast-forests" },
    CatalogEntry { code: "NA22", name: "Appalachian & Mixed Mesophytic Forests", level: BioregionalLevel::Bioregion, parent_code: "northern-america/northeast-forests" },
    CatalogEntry { code: "NA23", name: "New England & Acadian Forests", level: BioregionalLevel::Bioregion, parent_code: "northern-america/northeast-forests" },
    CatalogEntry { code: "NA24", name: "Mid-Atlantic Coastal Forests", level: BioregionalLevel::Bioregion, parent_code: "northern-america/northeast-forests" },
    CatalogEntry { code: "NA25", name: "Southeastern Conifer & Broadleaf Forests", level: BioregionalLevel::Bioregion, parent_code: "northern-america/southeast-us" },
    CatalogEntry { code: "NA26", name: "Mississippi Lowland Forests", level: BioregionalLevel::Bioregion, parent_code: "northern-america/southeast-us" },
    CatalogEntry { code: "NA27", name: "Florida Peninsula", level: BioregionalLevel::Bioregion, parent_code: "northern-america/southeast-us" },
    CatalogEntry { code: "NA28", name: "Gulf Coast Prairies & Marshes", level: BioregionalLevel::Bioregion, parent_code: "northern-america/southeast-us" },
    CatalogEntry { code: "NA29", name: "Chihuahuan Desert", level: BioregionalLevel::Bioregion, parent_code: "northern-america/mexican-drylands" },
    CatalogEntry { code: "NA30", name: "Tamaulipan Thornscrub", level: BioregionalLevel::Bioregion, parent_code: "northern-america/mexican-drylands" },
    CatalogEntry { code: "NA31", name: "Baja California Desert", level: BioregionalLevel::Bioregion, parent_code: "northern-america/mexican-drylands" },

    // ══════════════════════════════════════════════════════════════════════
    // REALM 3: Central America
    // ══════════════════════════════════════════════════════════════════════
    CatalogEntry { code: "central-america", name: "Central America", level: BioregionalLevel::Realm, parent_code: "ROOT" },

    // Subrealms
    CatalogEntry { code: "central-america/central-america", name: "Central American Forests", level: BioregionalLevel::Subrealm, parent_code: "central-america" },
    CatalogEntry { code: "central-america/caribbean", name: "Caribbean", level: BioregionalLevel::Subrealm, parent_code: "central-america" },

    // Bioregions NT24–NT29
    CatalogEntry { code: "NT24", name: "Sierra Madre & Mexican Pine-Oak Forests", level: BioregionalLevel::Bioregion, parent_code: "central-america/central-america" },
    CatalogEntry { code: "NT25", name: "Central American Moist & Dry Forests", level: BioregionalLevel::Bioregion, parent_code: "central-america/central-america" },
    CatalogEntry { code: "NT26", name: "Cuban Moist & Dry Forests", level: BioregionalLevel::Bioregion, parent_code: "central-america/caribbean" },
    CatalogEntry { code: "NT27", name: "Hispaniolan Moist & Dry Forests", level: BioregionalLevel::Bioregion, parent_code: "central-america/caribbean" },
    CatalogEntry { code: "NT28", name: "Jamaican & Puerto Rican Forests", level: BioregionalLevel::Bioregion, parent_code: "central-america/caribbean" },
    CatalogEntry { code: "NT29", name: "Lesser Antillean Forests", level: BioregionalLevel::Bioregion, parent_code: "central-america/caribbean" },

    // ══════════════════════════════════════════════════════════════════════
    // REALM 4: Southern America
    // ══════════════════════════════════════════════════════════════════════
    CatalogEntry { code: "southern-america", name: "Southern America", level: BioregionalLevel::Realm, parent_code: "ROOT" },

    // Subrealms
    CatalogEntry { code: "southern-america/andes-pacific", name: "Andes & Pacific Coast", level: BioregionalLevel::Subrealm, parent_code: "southern-america" },
    CatalogEntry { code: "southern-america/grasslands", name: "South American Grasslands", level: BioregionalLevel::Subrealm, parent_code: "southern-america" },
    CatalogEntry { code: "southern-america/cerrado", name: "Brazilian Cerrado & Atlantic Forest", level: BioregionalLevel::Subrealm, parent_code: "southern-america" },
    CatalogEntry { code: "southern-america/amazonia", name: "Amazonia", level: BioregionalLevel::Subrealm, parent_code: "southern-america" },
    CatalogEntry { code: "southern-america/upper-sa", name: "Upper South America", level: BioregionalLevel::Subrealm, parent_code: "southern-america" },

    // Bioregions NT1–NT23
    CatalogEntry { code: "NT1", name: "Atacama & Sechura Deserts", level: BioregionalLevel::Bioregion, parent_code: "southern-america/andes-pacific" },
    CatalogEntry { code: "NT2", name: "Central Andean Dry Puna", level: BioregionalLevel::Bioregion, parent_code: "southern-america/andes-pacific" },
    CatalogEntry { code: "NT3", name: "Central Andean Wet Puna & Yungas", level: BioregionalLevel::Bioregion, parent_code: "southern-america/andes-pacific" },
    CatalogEntry { code: "NT4", name: "Valdivian Temperate Rainforests", level: BioregionalLevel::Bioregion, parent_code: "southern-america/andes-pacific" },
    CatalogEntry { code: "NT5", name: "Patagonian Steppe & Grasslands", level: BioregionalLevel::Bioregion, parent_code: "southern-america/andes-pacific" },
    CatalogEntry { code: "NT6", name: "Southern Andean Forests", level: BioregionalLevel::Bioregion, parent_code: "southern-america/andes-pacific" },
    CatalogEntry { code: "NT7", name: "Pampas Grasslands", level: BioregionalLevel::Bioregion, parent_code: "southern-america/grasslands" },
    CatalogEntry { code: "NT8", name: "Uruguayan Savanna", level: BioregionalLevel::Bioregion, parent_code: "southern-america/grasslands" },
    CatalogEntry { code: "NT9", name: "Espinal & Argentine Monte", level: BioregionalLevel::Bioregion, parent_code: "southern-america/grasslands" },
    CatalogEntry { code: "NT10", name: "Gran Chaco", level: BioregionalLevel::Bioregion, parent_code: "southern-america/grasslands" },
    CatalogEntry { code: "NT11", name: "Cerrado Savannas", level: BioregionalLevel::Bioregion, parent_code: "southern-america/cerrado" },
    CatalogEntry { code: "NT12", name: "Caatinga Dry Forests", level: BioregionalLevel::Bioregion, parent_code: "southern-america/cerrado" },
    CatalogEntry { code: "NT13", name: "Atlantic Forests of Brazil", level: BioregionalLevel::Bioregion, parent_code: "southern-america/cerrado" },
    CatalogEntry { code: "NT14", name: "Pantanal Wetlands", level: BioregionalLevel::Bioregion, parent_code: "southern-america/cerrado" },
    CatalogEntry { code: "NT15", name: "Western Amazonian Forests", level: BioregionalLevel::Bioregion, parent_code: "southern-america/amazonia" },
    CatalogEntry { code: "NT16", name: "Central Amazonian Forests", level: BioregionalLevel::Bioregion, parent_code: "southern-america/amazonia" },
    CatalogEntry { code: "NT17", name: "Eastern Amazonian Forests", level: BioregionalLevel::Bioregion, parent_code: "southern-america/amazonia" },
    CatalogEntry { code: "NT18", name: "Guianan Moist Forests", level: BioregionalLevel::Bioregion, parent_code: "southern-america/amazonia" },
    CatalogEntry { code: "NT19", name: "Guianan Highlands & Tepuis", level: BioregionalLevel::Bioregion, parent_code: "southern-america/amazonia" },
    CatalogEntry { code: "NT20", name: "Northern Andean Páramo & Montane Forests", level: BioregionalLevel::Bioregion, parent_code: "southern-america/upper-sa" },
    CatalogEntry { code: "NT21", name: "Colombian & Venezuelan Dry Forests", level: BioregionalLevel::Bioregion, parent_code: "southern-america/upper-sa" },
    CatalogEntry { code: "NT22", name: "Magdalena & Chocó-Darién Moist Forests", level: BioregionalLevel::Bioregion, parent_code: "southern-america/upper-sa" },
    CatalogEntry { code: "NT23", name: "Venezuelan Llanos", level: BioregionalLevel::Bioregion, parent_code: "southern-america/upper-sa" },

    // ══════════════════════════════════════════════════════════════════════
    // REALM 5: Afrotropics
    // ══════════════════════════════════════════════════════════════════════
    CatalogEntry { code: "afrotropics", name: "Afrotropics", level: BioregionalLevel::Realm, parent_code: "ROOT" },

    // Subrealms
    CatalogEntry { code: "afrotropics/southern", name: "Southern Afrotropics", level: BioregionalLevel::Subrealm, parent_code: "afrotropics" },
    CatalogEntry { code: "afrotropics/equatorial", name: "Equatorial Afrotropics", level: BioregionalLevel::Subrealm, parent_code: "afrotropics" },
    CatalogEntry { code: "afrotropics/sub-equatorial", name: "Sub-Equatorial Afrotropics", level: BioregionalLevel::Subrealm, parent_code: "afrotropics" },
    CatalogEntry { code: "afrotropics/madagascar-east-africa", name: "Madagascar & East Africa", level: BioregionalLevel::Subrealm, parent_code: "afrotropics" },
    CatalogEntry { code: "afrotropics/sub-saharan", name: "Sub-Saharan Afrotropics", level: BioregionalLevel::Subrealm, parent_code: "afrotropics" },
    CatalogEntry { code: "afrotropics/horn-of-africa", name: "Horn of Africa", level: BioregionalLevel::Subrealm, parent_code: "afrotropics" },

    // Bioregions AT1–AT24
    CatalogEntry { code: "AT1", name: "East African Montane Forests & Woodlands", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/southern" },
    CatalogEntry { code: "AT2", name: "Southern Rift Montane Forests", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/southern" },
    CatalogEntry { code: "AT3", name: "Zambezian Savannas & Mopane Woodlands", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/southern" },
    CatalogEntry { code: "AT4", name: "Southern African Bushveld", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/southern" },
    CatalogEntry { code: "AT5", name: "Kalahari & Karoo Drylands", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/southern" },
    CatalogEntry { code: "AT6", name: "Cape Floristic Fynbos", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/southern" },
    CatalogEntry { code: "AT7", name: "Congolian Coastal Forests", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/equatorial" },
    CatalogEntry { code: "AT8", name: "Central Congolian Lowland Forests", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/equatorial" },
    CatalogEntry { code: "AT9", name: "Northeast Congolian Forests", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/equatorial" },
    CatalogEntry { code: "AT10", name: "Albertine Rift Montane Forests", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/equatorial" },
    CatalogEntry { code: "AT11", name: "West African Mangroves & Coastal Forests", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/sub-equatorial" },
    CatalogEntry { code: "AT12", name: "Guinean Moist Forests", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/sub-equatorial" },
    CatalogEntry { code: "AT13", name: "Nigerian Lowland Forests", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/sub-equatorial" },
    CatalogEntry { code: "AT14", name: "Cameroon Highlands & Cross-Sanaga Forests", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/sub-equatorial" },
    CatalogEntry { code: "AT15", name: "Madagascar Moist Forests", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/madagascar-east-africa" },
    CatalogEntry { code: "AT16", name: "Madagascar Dry Forests & Spiny Thicket", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/madagascar-east-africa" },
    CatalogEntry { code: "AT17", name: "East African Coastal Forests", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/madagascar-east-africa" },
    CatalogEntry { code: "AT18", name: "East African Savannas", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/madagascar-east-africa" },
    CatalogEntry { code: "AT19", name: "Sahel Grasslands & Savannas", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/sub-saharan" },
    CatalogEntry { code: "AT20", name: "West Sudanian Savanna", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/sub-saharan" },
    CatalogEntry { code: "AT21", name: "East Sudanian Savanna", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/sub-saharan" },
    CatalogEntry { code: "AT22", name: "Ethiopian Highlands", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/horn-of-africa" },
    CatalogEntry { code: "AT23", name: "Somali Acacia-Commiphora Bushlands", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/horn-of-africa" },
    CatalogEntry { code: "AT24", name: "Eritrean & Djiboutian Coastal Desert", level: BioregionalLevel::Bioregion, parent_code: "afrotropics/horn-of-africa" },

    // ══════════════════════════════════════════════════════════════════════
    // REALM 6: Subarctic Eurasia
    // ══════════════════════════════════════════════════════════════════════
    CatalogEntry { code: "subarctic-eurasia", name: "Subarctic Eurasia", level: BioregionalLevel::Realm, parent_code: "ROOT" },

    // Subrealms
    CatalogEntry { code: "subarctic-eurasia/scandinavia-boreal", name: "Scandinavia & Western Boreal", level: BioregionalLevel::Subrealm, parent_code: "subarctic-eurasia" },
    CatalogEntry { code: "subarctic-eurasia/palearctic-tundra", name: "Palearctic Tundra", level: BioregionalLevel::Subrealm, parent_code: "subarctic-eurasia" },
    CatalogEntry { code: "subarctic-eurasia/sea-of-okhotsk", name: "Sea of Okhotsk", level: BioregionalLevel::Subrealm, parent_code: "subarctic-eurasia" },
    CatalogEntry { code: "subarctic-eurasia/siberia", name: "Siberia", level: BioregionalLevel::Subrealm, parent_code: "subarctic-eurasia" },

    // Bioregions PA1–PA8
    CatalogEntry { code: "PA1", name: "Scandinavian Boreal Forests & Alpine Tundra", level: BioregionalLevel::Bioregion, parent_code: "subarctic-eurasia/scandinavia-boreal" },
    CatalogEntry { code: "PA2", name: "Icelandic Tundra & Boreal Birch", level: BioregionalLevel::Bioregion, parent_code: "subarctic-eurasia/scandinavia-boreal" },
    CatalogEntry { code: "PA3", name: "Kola Peninsula Tundra", level: BioregionalLevel::Bioregion, parent_code: "subarctic-eurasia/palearctic-tundra" },
    CatalogEntry { code: "PA4", name: "Taimyr & Siberian Arctic Tundra", level: BioregionalLevel::Bioregion, parent_code: "subarctic-eurasia/palearctic-tundra" },
    CatalogEntry { code: "PA5", name: "Chukchi & Kamchatka Tundra", level: BioregionalLevel::Bioregion, parent_code: "subarctic-eurasia/sea-of-okhotsk" },
    CatalogEntry { code: "PA6", name: "Sea of Okhotsk Coastal Forests", level: BioregionalLevel::Bioregion, parent_code: "subarctic-eurasia/sea-of-okhotsk" },
    CatalogEntry { code: "PA7", name: "West Siberian Taiga", level: BioregionalLevel::Bioregion, parent_code: "subarctic-eurasia/siberia" },
    CatalogEntry { code: "PA8", name: "East Siberian Taiga", level: BioregionalLevel::Bioregion, parent_code: "subarctic-eurasia/siberia" },

    // ══════════════════════════════════════════════════════════════════════
    // REALM 7: Western Eurasia
    // ══════════════════════════════════════════════════════════════════════
    CatalogEntry { code: "western-eurasia", name: "Western Eurasia", level: BioregionalLevel::Realm, parent_code: "ROOT" },

    // Subrealms
    CatalogEntry { code: "western-eurasia/anglo-celtic", name: "Anglo-Celtic", level: BioregionalLevel::Subrealm, parent_code: "western-eurasia" },
    CatalogEntry { code: "western-eurasia/greater-european", name: "Greater European", level: BioregionalLevel::Subrealm, parent_code: "western-eurasia" },
    CatalogEntry { code: "western-eurasia/european-mountain", name: "European Mountain", level: BioregionalLevel::Subrealm, parent_code: "western-eurasia" },
    CatalogEntry { code: "western-eurasia/black-sea", name: "Black Sea", level: BioregionalLevel::Subrealm, parent_code: "western-eurasia" },
    CatalogEntry { code: "western-eurasia/mediterranean", name: "Mediterranean", level: BioregionalLevel::Subrealm, parent_code: "western-eurasia" },

    // Bioregions PA9–PA21
    CatalogEntry { code: "PA9", name: "British Isles Mixed Forests", level: BioregionalLevel::Bioregion, parent_code: "western-eurasia/anglo-celtic" },
    CatalogEntry { code: "PA10", name: "Celtic Broadleaf Forests", level: BioregionalLevel::Bioregion, parent_code: "western-eurasia/anglo-celtic" },
    CatalogEntry { code: "PA11", name: "Western European Broadleaf Forests", level: BioregionalLevel::Bioregion, parent_code: "western-eurasia/greater-european" },
    CatalogEntry { code: "PA12", name: "Central European Mixed Forests", level: BioregionalLevel::Bioregion, parent_code: "western-eurasia/greater-european" },
    CatalogEntry { code: "PA13", name: "Baltic Mixed Forests", level: BioregionalLevel::Bioregion, parent_code: "western-eurasia/greater-european" },
    CatalogEntry { code: "PA14", name: "Sarmatic Mixed Forests", level: BioregionalLevel::Bioregion, parent_code: "western-eurasia/greater-european" },
    CatalogEntry { code: "PA15", name: "Alps & Carpathian Montane Forests", level: BioregionalLevel::Bioregion, parent_code: "western-eurasia/european-mountain" },
    CatalogEntry { code: "PA16", name: "Pyrenees & South European Mountain Forests", level: BioregionalLevel::Bioregion, parent_code: "western-eurasia/european-mountain" },
    CatalogEntry { code: "PA17", name: "Caucasus & Anatolian Mixed Forests", level: BioregionalLevel::Bioregion, parent_code: "western-eurasia/black-sea" },
    CatalogEntry { code: "PA18", name: "Pontic Steppe & Black Sea Forests", level: BioregionalLevel::Bioregion, parent_code: "western-eurasia/black-sea" },
    CatalogEntry { code: "PA19", name: "Iberian Peninsula Forests & Scrub", level: BioregionalLevel::Bioregion, parent_code: "western-eurasia/mediterranean" },
    CatalogEntry { code: "PA20", name: "Italian Peninsula & Tyrrhenian Islands", level: BioregionalLevel::Bioregion, parent_code: "western-eurasia/mediterranean" },
    CatalogEntry { code: "PA21", name: "Eastern Mediterranean & Levant", level: BioregionalLevel::Bioregion, parent_code: "western-eurasia/mediterranean" },

    // ══════════════════════════════════════════════════════════════════════
    // REALM 8: Southern Eurasia
    // ══════════════════════════════════════════════════════════════════════
    CatalogEntry { code: "southern-eurasia", name: "Southern Eurasia", level: BioregionalLevel::Realm, parent_code: "ROOT" },

    // Subrealms
    CatalogEntry { code: "southern-eurasia/north-africa", name: "North Africa", level: BioregionalLevel::Subrealm, parent_code: "southern-eurasia" },
    CatalogEntry { code: "southern-eurasia/arabian-peninsula", name: "Greater Arabian Peninsula", level: BioregionalLevel::Subrealm, parent_code: "southern-eurasia" },

    // Bioregions PA22–PA26
    CatalogEntry { code: "PA22", name: "Saharan Desert", level: BioregionalLevel::Bioregion, parent_code: "southern-eurasia/north-africa" },
    CatalogEntry { code: "PA23", name: "North African Mediterranean Forests", level: BioregionalLevel::Bioregion, parent_code: "southern-eurasia/north-africa" },
    CatalogEntry { code: "PA24", name: "Nile Delta & Valley", level: BioregionalLevel::Bioregion, parent_code: "southern-eurasia/north-africa" },
    CatalogEntry { code: "PA25", name: "Arabian Desert & Shrublands", level: BioregionalLevel::Bioregion, parent_code: "southern-eurasia/arabian-peninsula" },
    CatalogEntry { code: "PA26", name: "Socotra & Arabian Coastal Fog Desert", level: BioregionalLevel::Bioregion, parent_code: "southern-eurasia/arabian-peninsula" },

    // ══════════════════════════════════════════════════════════════════════
    // REALM 9: Central Eurasia
    // ══════════════════════════════════════════════════════════════════════
    CatalogEntry { code: "central-eurasia", name: "Central Eurasia", level: BioregionalLevel::Realm, parent_code: "ROOT" },

    // Subrealms
    CatalogEntry { code: "central-eurasia/persian-deserts", name: "Persian Deserts", level: BioregionalLevel::Subrealm, parent_code: "central-eurasia" },
    CatalogEntry { code: "central-eurasia/tien-shan", name: "Tien Shan", level: BioregionalLevel::Subrealm, parent_code: "central-eurasia" },
    CatalogEntry { code: "central-eurasia/caspian-central-asian", name: "Caspian & Central Asian", level: BioregionalLevel::Subrealm, parent_code: "central-eurasia" },
    CatalogEntry { code: "central-eurasia/kazakh-steppes", name: "Kazakh Steppes", level: BioregionalLevel::Subrealm, parent_code: "central-eurasia" },
    CatalogEntry { code: "central-eurasia/altai-sayan", name: "Altai-Sayan", level: BioregionalLevel::Subrealm, parent_code: "central-eurasia" },

    // Bioregions PA27–PA37
    CatalogEntry { code: "PA27", name: "Persian Gulf & Iranian Deserts", level: BioregionalLevel::Bioregion, parent_code: "central-eurasia/persian-deserts" },
    CatalogEntry { code: "PA28", name: "Kopet-Dag & Hindu Kush Woodlands", level: BioregionalLevel::Bioregion, parent_code: "central-eurasia/persian-deserts" },
    CatalogEntry { code: "PA29", name: "Western Tien Shan Montane Forests", level: BioregionalLevel::Bioregion, parent_code: "central-eurasia/tien-shan" },
    CatalogEntry { code: "PA30", name: "Eastern Tien Shan & Junggar Basin", level: BioregionalLevel::Bioregion, parent_code: "central-eurasia/tien-shan" },
    CatalogEntry { code: "PA31", name: "Caspian Hyrcanian Forests", level: BioregionalLevel::Bioregion, parent_code: "central-eurasia/caspian-central-asian" },
    CatalogEntry { code: "PA32", name: "Karakum & Kyzylkum Deserts", level: BioregionalLevel::Bioregion, parent_code: "central-eurasia/caspian-central-asian" },
    CatalogEntry { code: "PA33", name: "Ferghana & Pamir Alpine Meadows", level: BioregionalLevel::Bioregion, parent_code: "central-eurasia/caspian-central-asian" },
    CatalogEntry { code: "PA34", name: "Kazakh Steppe & Semi-Desert", level: BioregionalLevel::Bioregion, parent_code: "central-eurasia/kazakh-steppes" },
    CatalogEntry { code: "PA35", name: "Kazakh Forest Steppe & Uplands", level: BioregionalLevel::Bioregion, parent_code: "central-eurasia/kazakh-steppes" },
    CatalogEntry { code: "PA36", name: "Altai-Sayan Montane Forests", level: BioregionalLevel::Bioregion, parent_code: "central-eurasia/altai-sayan" },
    CatalogEntry { code: "PA37", name: "Sayan Alpine Meadows & Tundra", level: BioregionalLevel::Bioregion, parent_code: "central-eurasia/altai-sayan" },

    // ══════════════════════════════════════════════════════════════════════
    // REALM 10: Eastern Eurasia
    // ══════════════════════════════════════════════════════════════════════
    CatalogEntry { code: "eastern-eurasia", name: "Eastern Eurasia", level: BioregionalLevel::Realm, parent_code: "ROOT" },

    // Subrealms
    CatalogEntry { code: "eastern-eurasia/east-asian-deserts", name: "East Asian Deserts", level: BioregionalLevel::Subrealm, parent_code: "eastern-eurasia" },
    CatalogEntry { code: "eastern-eurasia/tibetan-plateau", name: "Tibetan Plateau", level: BioregionalLevel::Subrealm, parent_code: "eastern-eurasia" },
    CatalogEntry { code: "eastern-eurasia/mongolian-grasslands", name: "Mongolian Grasslands", level: BioregionalLevel::Subrealm, parent_code: "eastern-eurasia" },
    CatalogEntry { code: "eastern-eurasia/northeast-asian-forests", name: "Northeast Asian Forests", level: BioregionalLevel::Subrealm, parent_code: "eastern-eurasia" },
    CatalogEntry { code: "eastern-eurasia/japanese-islands", name: "Japanese Islands", level: BioregionalLevel::Subrealm, parent_code: "eastern-eurasia" },
    CatalogEntry { code: "eastern-eurasia/central-east-asian", name: "Central East Asian", level: BioregionalLevel::Subrealm, parent_code: "eastern-eurasia" },

    // Bioregions PA38–PA53
    CatalogEntry { code: "PA38", name: "Taklamakan & Gobi Deserts", level: BioregionalLevel::Bioregion, parent_code: "eastern-eurasia/east-asian-deserts" },
    CatalogEntry { code: "PA39", name: "Alashan Plateau & Ordos Semi-Desert", level: BioregionalLevel::Bioregion, parent_code: "eastern-eurasia/east-asian-deserts" },
    CatalogEntry { code: "PA40", name: "Qaidam Basin Desert", level: BioregionalLevel::Bioregion, parent_code: "eastern-eurasia/east-asian-deserts" },
    CatalogEntry { code: "PA41", name: "Tibetan Plateau Alpine Shrublands", level: BioregionalLevel::Bioregion, parent_code: "eastern-eurasia/tibetan-plateau" },
    CatalogEntry { code: "PA42", name: "Southeastern Tibetan Plateau Forests", level: BioregionalLevel::Bioregion, parent_code: "eastern-eurasia/tibetan-plateau" },
    CatalogEntry { code: "PA43", name: "Karakoram-Western Tibetan Plateau", level: BioregionalLevel::Bioregion, parent_code: "eastern-eurasia/tibetan-plateau" },
    CatalogEntry { code: "PA44", name: "Mongolian Grasslands & Steppe", level: BioregionalLevel::Bioregion, parent_code: "eastern-eurasia/mongolian-grasslands" },
    CatalogEntry { code: "PA45", name: "Daurian Forest Steppe", level: BioregionalLevel::Bioregion, parent_code: "eastern-eurasia/mongolian-grasslands" },
    CatalogEntry { code: "PA46", name: "Manchurian Mixed Forests", level: BioregionalLevel::Bioregion, parent_code: "eastern-eurasia/northeast-asian-forests" },
    CatalogEntry { code: "PA47", name: "Amur River Basin Forests", level: BioregionalLevel::Bioregion, parent_code: "eastern-eurasia/northeast-asian-forests" },
    CatalogEntry { code: "PA48", name: "Korean Peninsula Mixed Forests", level: BioregionalLevel::Bioregion, parent_code: "eastern-eurasia/northeast-asian-forests" },
    CatalogEntry { code: "PA49", name: "Japanese Archipelago Forests", level: BioregionalLevel::Bioregion, parent_code: "eastern-eurasia/japanese-islands" },
    CatalogEntry { code: "PA50", name: "Ryukyu & Ogasawara Islands", level: BioregionalLevel::Bioregion, parent_code: "eastern-eurasia/japanese-islands" },
    CatalogEntry { code: "PA51", name: "Yangtze Basin & Central China Forests", level: BioregionalLevel::Bioregion, parent_code: "eastern-eurasia/central-east-asian" },
    CatalogEntry { code: "PA52", name: "South China & Hainan Forests", level: BioregionalLevel::Bioregion, parent_code: "eastern-eurasia/central-east-asian" },
    CatalogEntry { code: "PA53", name: "Taiwan Montane Forests", level: BioregionalLevel::Bioregion, parent_code: "eastern-eurasia/central-east-asian" },

    // ══════════════════════════════════════════════════════════════════════
    // REALM 11: Indomalaya
    // ══════════════════════════════════════════════════════════════════════
    CatalogEntry { code: "indomalaya", name: "Indomalaya", level: BioregionalLevel::Realm, parent_code: "ROOT" },

    // Subrealms
    CatalogEntry { code: "indomalaya/indian-subcontinent", name: "Indian Subcontinent", level: BioregionalLevel::Subrealm, parent_code: "indomalaya" },
    CatalogEntry { code: "indomalaya/southeast-asian-forests", name: "Southeast Asian Forests", level: BioregionalLevel::Subrealm, parent_code: "indomalaya" },
    CatalogEntry { code: "indomalaya/malaysia-west-indonesia", name: "Malaysia & Western Indonesia", level: BioregionalLevel::Subrealm, parent_code: "indomalaya" },

    // Bioregions IM1–IM18
    CatalogEntry { code: "IM1", name: "Himalayan Alpine Meadows & Forests", level: BioregionalLevel::Bioregion, parent_code: "indomalaya/indian-subcontinent" },
    CatalogEntry { code: "IM2", name: "Ganges-Brahmaputra Lowland Forests", level: BioregionalLevel::Bioregion, parent_code: "indomalaya/indian-subcontinent" },
    CatalogEntry { code: "IM3", name: "Western Ghats & Sri Lanka Moist Forests", level: BioregionalLevel::Bioregion, parent_code: "indomalaya/indian-subcontinent" },
    CatalogEntry { code: "IM4", name: "Deccan Plateau Dry Forests", level: BioregionalLevel::Bioregion, parent_code: "indomalaya/indian-subcontinent" },
    CatalogEntry { code: "IM5", name: "Thar & Indus Valley Deserts", level: BioregionalLevel::Bioregion, parent_code: "indomalaya/indian-subcontinent" },
    CatalogEntry { code: "IM6", name: "Northeast India & Myanmar Forests", level: BioregionalLevel::Bioregion, parent_code: "indomalaya/indian-subcontinent" },
    CatalogEntry { code: "IM7", name: "Indochina Dry Forests", level: BioregionalLevel::Bioregion, parent_code: "indomalaya/southeast-asian-forests" },
    CatalogEntry { code: "IM8", name: "Indochina Moist Forests", level: BioregionalLevel::Bioregion, parent_code: "indomalaya/southeast-asian-forests" },
    CatalogEntry { code: "IM9", name: "Annamite Mountains & Central Vietnam", level: BioregionalLevel::Bioregion, parent_code: "indomalaya/southeast-asian-forests" },
    CatalogEntry { code: "IM10", name: "Myanmar Coastal & Irrawaddy Forests", level: BioregionalLevel::Bioregion, parent_code: "indomalaya/southeast-asian-forests" },
    CatalogEntry { code: "IM11", name: "South China Sea Mangroves & Coastal", level: BioregionalLevel::Bioregion, parent_code: "indomalaya/southeast-asian-forests" },
    CatalogEntry { code: "IM12", name: "Philippines Moist Forests", level: BioregionalLevel::Bioregion, parent_code: "indomalaya/southeast-asian-forests" },
    CatalogEntry { code: "IM13", name: "Peninsular Malaysian Forests", level: BioregionalLevel::Bioregion, parent_code: "indomalaya/malaysia-west-indonesia" },
    CatalogEntry { code: "IM14", name: "Borneo Lowland & Montane Forests", level: BioregionalLevel::Bioregion, parent_code: "indomalaya/malaysia-west-indonesia" },
    CatalogEntry { code: "IM15", name: "Sumatran Tropical Forests", level: BioregionalLevel::Bioregion, parent_code: "indomalaya/malaysia-west-indonesia" },
    CatalogEntry { code: "IM16", name: "Java & Bali Moist Forests", level: BioregionalLevel::Bioregion, parent_code: "indomalaya/malaysia-west-indonesia" },
    CatalogEntry { code: "IM17", name: "Lesser Sunda Islands Dry Forests", level: BioregionalLevel::Bioregion, parent_code: "indomalaya/malaysia-west-indonesia" },
    CatalogEntry { code: "IM18", name: "Sulawesi Moist Forests", level: BioregionalLevel::Bioregion, parent_code: "indomalaya/malaysia-west-indonesia" },

    // ══════════════════════════════════════════════════════════════════════
    // REALM 12: Australasia
    // ══════════════════════════════════════════════════════════════════════
    CatalogEntry { code: "australasia", name: "Australasia", level: BioregionalLevel::Realm, parent_code: "ROOT" },

    // Subrealms
    CatalogEntry { code: "australasia/new-zealand", name: "New Zealand", level: BioregionalLevel::Subrealm, parent_code: "australasia" },
    CatalogEntry { code: "australasia/islands-east-indonesia", name: "Australasian Islands & Eastern Indonesia", level: BioregionalLevel::Subrealm, parent_code: "australasia" },
    CatalogEntry { code: "australasia/australia", name: "Australia", level: BioregionalLevel::Subrealm, parent_code: "australasia" },

    // Bioregions AU1–AU16
    CatalogEntry { code: "AU1", name: "New Zealand Temperate Forests", level: BioregionalLevel::Bioregion, parent_code: "australasia/new-zealand" },
    CatalogEntry { code: "AU2", name: "New Zealand Subantarctic Islands", level: BioregionalLevel::Bioregion, parent_code: "australasia/new-zealand" },
    CatalogEntry { code: "AU3", name: "New Guinea Tropical Forests", level: BioregionalLevel::Bioregion, parent_code: "australasia/islands-east-indonesia" },
    CatalogEntry { code: "AU4", name: "New Guinea Montane Forests", level: BioregionalLevel::Bioregion, parent_code: "australasia/islands-east-indonesia" },
    CatalogEntry { code: "AU5", name: "Moluccas & Lesser Sundas Transition Forests", level: BioregionalLevel::Bioregion, parent_code: "australasia/islands-east-indonesia" },
    CatalogEntry { code: "AU6", name: "Solomon & Vanuatu Rainforests", level: BioregionalLevel::Bioregion, parent_code: "australasia/islands-east-indonesia" },
    CatalogEntry { code: "AU7", name: "New Caledonian Moist Forests", level: BioregionalLevel::Bioregion, parent_code: "australasia/islands-east-indonesia" },
    CatalogEntry { code: "AU8", name: "Queensland Tropical Rainforests", level: BioregionalLevel::Bioregion, parent_code: "australasia/australia" },
    CatalogEntry { code: "AU9", name: "Eastern Australian Temperate Forests", level: BioregionalLevel::Bioregion, parent_code: "australasia/australia" },
    CatalogEntry { code: "AU10", name: "Southeast Australian Forests & Mallee", level: BioregionalLevel::Bioregion, parent_code: "australasia/australia" },
    CatalogEntry { code: "AU11", name: "Murray-Darling Woodlands", level: BioregionalLevel::Bioregion, parent_code: "australasia/australia" },
    CatalogEntry { code: "AU12", name: "Western Australian Woodlands", level: BioregionalLevel::Bioregion, parent_code: "australasia/australia" },
    CatalogEntry { code: "AU13", name: "Southwest Australian Forests", level: BioregionalLevel::Bioregion, parent_code: "australasia/australia" },
    CatalogEntry { code: "AU14", name: "Central Australian Deserts", level: BioregionalLevel::Bioregion, parent_code: "australasia/australia" },
    CatalogEntry { code: "AU15", name: "Northern Australian Savannas", level: BioregionalLevel::Bioregion, parent_code: "australasia/australia" },
    CatalogEntry { code: "AU16", name: "Tasmanian Temperate Forests", level: BioregionalLevel::Bioregion, parent_code: "australasia/australia" },

    // ══════════════════════════════════════════════════════════════════════
    // REALM 13: Oceania
    // ══════════════════════════════════════════════════════════════════════
    CatalogEntry { code: "oceania", name: "Oceania", level: BioregionalLevel::Realm, parent_code: "ROOT" },

    // Subrealms
    CatalogEntry { code: "oceania/oceanic-islands", name: "Oceanic Islands", level: BioregionalLevel::Subrealm, parent_code: "oceania" },

    // Bioregions OC1–OC11
    CatalogEntry { code: "OC1", name: "Hawaiian Tropical Forests", level: BioregionalLevel::Bioregion, parent_code: "oceania/oceanic-islands" },
    CatalogEntry { code: "OC2", name: "Fijian Tropical Forests", level: BioregionalLevel::Bioregion, parent_code: "oceania/oceanic-islands" },
    CatalogEntry { code: "OC3", name: "Samoan Tropical Forests", level: BioregionalLevel::Bioregion, parent_code: "oceania/oceanic-islands" },
    CatalogEntry { code: "OC4", name: "Tongan Tropical Forests", level: BioregionalLevel::Bioregion, parent_code: "oceania/oceanic-islands" },
    CatalogEntry { code: "OC5", name: "Society & Marquesas Islands", level: BioregionalLevel::Bioregion, parent_code: "oceania/oceanic-islands" },
    CatalogEntry { code: "OC6", name: "Micronesian Islands", level: BioregionalLevel::Bioregion, parent_code: "oceania/oceanic-islands" },
    CatalogEntry { code: "OC7", name: "Palau & Marianas Islands", level: BioregionalLevel::Bioregion, parent_code: "oceania/oceanic-islands" },
    CatalogEntry { code: "OC8", name: "Cook & Tuamotu Islands", level: BioregionalLevel::Bioregion, parent_code: "oceania/oceanic-islands" },
    CatalogEntry { code: "OC9", name: "Easter Island & Southeastern Polynesia", level: BioregionalLevel::Bioregion, parent_code: "oceania/oceanic-islands" },
    CatalogEntry { code: "OC10", name: "Galápagos Islands", level: BioregionalLevel::Bioregion, parent_code: "oceania/oceanic-islands" },
    CatalogEntry { code: "OC11", name: "Line & Phoenix Islands", level: BioregionalLevel::Bioregion, parent_code: "oceania/oceanic-islands" },

    // ══════════════════════════════════════════════════════════════════════
    // REALM 14: Antarctica
    // ══════════════════════════════════════════════════════════════════════
    CatalogEntry { code: "antarctica", name: "Antarctica", level: BioregionalLevel::Realm, parent_code: "ROOT" },

    // Subrealms
    CatalogEntry { code: "antarctica/antarctic-continent", name: "Antarctic Continent & Islands", level: BioregionalLevel::Subrealm, parent_code: "antarctica" },

    // Bioregions AN1–AN3
    CatalogEntry { code: "AN1", name: "Antarctic Peninsula & Maritime Antarctica", level: BioregionalLevel::Bioregion, parent_code: "antarctica/antarctic-continent" },
    CatalogEntry { code: "AN2", name: "Continental Antarctica & Ice Sheet", level: BioregionalLevel::Bioregion, parent_code: "antarctica/antarctic-continent" },
    CatalogEntry { code: "AN3", name: "Subantarctic Islands", level: BioregionalLevel::Bioregion, parent_code: "antarctica/antarctic-continent" },
];

// ============================================================================
// QUERY API
// ============================================================================

/// Query interface for the bioregional catalog.
///
/// Built lazily from the static `CATALOG` array on first access.
/// Thread-safe via `OnceLock`.
pub struct BioregionalCatalog {
    by_code: BTreeMap<&'static str, &'static CatalogEntry>,
    by_parent: BTreeMap<&'static str, Vec<&'static CatalogEntry>>,
}

impl BioregionalCatalog {
    /// Get the global singleton catalog instance.
    pub fn global() -> &'static BioregionalCatalog {
        static INSTANCE: OnceLock<BioregionalCatalog> = OnceLock::new();
        INSTANCE.get_or_init(|| {
            let mut by_code = BTreeMap::new();
            let mut by_parent: BTreeMap<&'static str, Vec<&'static CatalogEntry>> = BTreeMap::new();

            for entry in CATALOG {
                by_code.insert(entry.code, entry);
                by_parent.entry(entry.parent_code).or_default().push(entry);
            }

            BioregionalCatalog { by_code, by_parent }
        })
    }

    /// Look up a catalog entry by its code.
    pub fn get(&self, code: &str) -> Option<&'static CatalogEntry> {
        self.by_code.get(code).copied()
    }

    /// Get all direct children of a given parent code.
    pub fn children_of(&self, parent_code: &str) -> Vec<&'static CatalogEntry> {
        self.by_parent.get(parent_code).cloned().unwrap_or_default()
    }

    /// Get the path from a given entry up to the root (inclusive).
    ///
    /// Returns entries in order from the given entry to ROOT.
    /// Returns an empty vec if the code is not found.
    pub fn path_to_root(&self, code: &str) -> Vec<&'static CatalogEntry> {
        let mut path = Vec::new();
        let mut current = code;

        loop {
            match self.get(current) {
                Some(entry) => {
                    path.push(entry);
                    if entry.parent_code.is_empty() {
                        break; // Reached root
                    }
                    current = entry.parent_code;
                }
                None => break, // Invalid code
            }
        }

        path
    }

    /// Get all entries at a given hierarchy level.
    pub fn entries_at_level(&self, level: BioregionalLevel) -> Vec<&'static CatalogEntry> {
        CATALOG.iter().filter(|e| e.level == level).collect()
    }

    /// Get all realm entries.
    pub fn realms(&self) -> Vec<&'static CatalogEntry> {
        self.entries_at_level(BioregionalLevel::Realm)
    }

    /// Verify that a code exists and is at the expected level.
    pub fn verify(&self, code: &str, expected_level: BioregionalLevel) -> bool {
        self.get(code).map_or(false, |e| e.level == expected_level)
    }

    /// Total number of entries in the catalog.
    pub fn len(&self) -> usize {
        self.by_code.len()
    }

    /// Whether the catalog is empty (it never should be).
    pub fn is_empty(&self) -> bool {
        self.by_code.is_empty()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_entry_counts() {
        let catalog = BioregionalCatalog::global();

        let roots = catalog.entries_at_level(BioregionalLevel::Root);
        let realms = catalog.entries_at_level(BioregionalLevel::Realm);
        let subrealms = catalog.entries_at_level(BioregionalLevel::Subrealm);
        let bioregions = catalog.entries_at_level(BioregionalLevel::Bioregion);

        assert_eq!(roots.len(), 1, "expected 1 root");
        assert_eq!(realms.len(), 14, "expected 14 realms");
        assert_eq!(subrealms.len(), 53, "expected 53 subrealms");
        assert_eq!(bioregions.len(), 185, "expected 185 bioregions");

        // Total: 1 + 14 + 53 + 185 = 253
        assert_eq!(catalog.len(), 1 + 14 + 53 + 185);
    }

    #[test]
    fn test_root_has_no_parent() {
        let catalog = BioregionalCatalog::global();
        let root = catalog.get("ROOT").expect("ROOT must exist");
        assert_eq!(root.parent_code, "");
        assert_eq!(root.level, BioregionalLevel::Root);
        assert_eq!(root.name, "Temples of Refuge");
    }

    #[test]
    fn test_every_nonroot_entry_has_valid_parent() {
        let catalog = BioregionalCatalog::global();
        for entry in CATALOG {
            if entry.code == "ROOT" {
                continue;
            }
            assert!(
                catalog.get(entry.parent_code).is_some(),
                "entry {:?} has invalid parent_code {:?}",
                entry.code,
                entry.parent_code
            );
        }
    }

    #[test]
    fn test_parent_is_exactly_one_level_up() {
        let catalog = BioregionalCatalog::global();
        for entry in CATALOG {
            if entry.code == "ROOT" {
                continue;
            }
            let parent = catalog.get(entry.parent_code).unwrap_or_else(|| {
                panic!("parent {:?} not found for {:?}", entry.parent_code, entry.code)
            });
            assert_eq!(
                entry.level.depth(),
                parent.level.depth() + 1,
                "entry {:?} (level {:?}) parent {:?} (level {:?}) not exactly one level up",
                entry.code,
                entry.level,
                parent.code,
                parent.level
            );
        }
    }

    #[test]
    fn test_no_duplicate_codes() {
        let mut seen = std::collections::HashSet::new();
        for entry in CATALOG {
            assert!(
                seen.insert(entry.code),
                "duplicate code: {:?}",
                entry.code
            );
        }
    }

    #[test]
    fn test_path_to_root_afrotropics() {
        let catalog = BioregionalCatalog::global();
        let path = catalog.path_to_root("AT1");
        assert_eq!(path.len(), 4);
        assert_eq!(path[0].code, "AT1");
        assert_eq!(path[1].code, "afrotropics/southern");
        assert_eq!(path[2].code, "afrotropics");
        assert_eq!(path[3].code, "ROOT");
    }

    #[test]
    fn test_children_of_root_returns_14_realms() {
        let catalog = BioregionalCatalog::global();
        let realms = catalog.children_of("ROOT");
        assert_eq!(realms.len(), 14);
        for realm in &realms {
            assert_eq!(realm.level, BioregionalLevel::Realm);
        }
    }

    #[test]
    fn test_known_realms_exist() {
        let catalog = BioregionalCatalog::global();
        let expected_realms = [
            "subarctic-america",
            "northern-america",
            "central-america",
            "southern-america",
            "afrotropics",
            "subarctic-eurasia",
            "western-eurasia",
            "southern-eurasia",
            "central-eurasia",
            "eastern-eurasia",
            "indomalaya",
            "australasia",
            "oceania",
            "antarctica",
        ];

        for code in &expected_realms {
            assert!(
                catalog.verify(code, BioregionalLevel::Realm),
                "realm {:?} not found or wrong level",
                code
            );
        }
    }

    #[test]
    fn test_known_bioregions_exist() {
        let catalog = BioregionalCatalog::global();
        let known = ["NA1", "NA31", "NT1", "NT29", "AT1", "AT24", "PA1", "PA53",
                      "IM1", "IM18", "AU1", "AU16", "OC1", "OC11", "AN1", "AN3"];
        for code in &known {
            assert!(
                catalog.verify(code, BioregionalLevel::Bioregion),
                "bioregion {:?} not found or wrong level",
                code
            );
        }
    }

    #[test]
    fn test_verify_wrong_level_returns_false() {
        let catalog = BioregionalCatalog::global();
        assert!(!catalog.verify("ROOT", BioregionalLevel::Realm));
        assert!(!catalog.verify("AT1", BioregionalLevel::Realm));
        assert!(!catalog.verify("nonexistent", BioregionalLevel::Root));
    }

    #[test]
    fn test_path_to_root_of_nonexistent_returns_empty() {
        let catalog = BioregionalCatalog::global();
        let path = catalog.path_to_root("FAKE999");
        assert!(path.is_empty());
    }

    #[test]
    fn test_children_of_leaf_returns_empty() {
        let catalog = BioregionalCatalog::global();
        let children = catalog.children_of("AN3");
        assert!(children.is_empty());
    }
}
