//! Mock artifact seeding for `--mock` flag.
//!
//! Populates the home realm artifact index with realistic test entries
//! so the 3-column artifact browser has data in all columns on first run.

use indras_network::{ArtifactId, HomeRealm, GeoLocation};
use indras_network::artifact_index::HomeArtifactEntry;
use indras_network::access::ArtifactStatus;

/// Seed the home realm artifact index with mock data.
///
/// Uses `ArtifactIndex::store()` which is idempotent — existing entries
/// with the same ID are skipped, so re-runs are safe.
pub async fn seed_mock_artifacts(
    home: &HomeRealm,
    instance_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let doc = home.artifact_index().await?;
    let entries = mock_entries(instance_name);
    doc.update(|index| {
        for entry in entries {
            index.store(entry);
        }
    })
    .await?;
    tracing::info!(instance = instance_name, "Seeded mock artifacts");
    Ok(())
}

/// Build a deterministic `ArtifactId` from a name string.
///
/// Uses a simple hash so the same name always yields the same ID,
/// making repeated runs idempotent.
fn mock_id(name: &str) -> ArtifactId {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "mock-artifact:".hash(&mut hasher);
    name.hash(&mut hasher);
    let h = hasher.finish();
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&h.to_le_bytes());
    // Fill remaining bytes with a second round for uniqueness
    let mut hasher2 = std::collections::hash_map::DefaultHasher::new();
    h.hash(&mut hasher2);
    name.hash(&mut hasher2);
    let h2 = hasher2.finish();
    bytes[8..16].copy_from_slice(&h2.to_le_bytes());
    bytes[16..24].copy_from_slice(&h.to_be_bytes());
    bytes[24..32].copy_from_slice(&h2.to_be_bytes());
    ArtifactId::Blob(bytes)
}

/// Generate mock entries for a given instance.
fn mock_entries(instance_name: &str) -> Vec<HomeArtifactEntry> {
    let now = chrono::Utc::now().timestamp_millis();

    // Shared entries (identical across instances)
    let mut entries = vec![
        entry(
            "project-spec.pdf",
            "application/pdf",
            2_400_000,
            Some(GeoLocation { lat: 38.72, lng: -9.14 }),
            0,
            now,
        ),
        entry(
            "team-photo.jpg",
            "image/jpeg",
            3_100_000,
            Some(GeoLocation { lat: 38.73, lng: -9.15 }),
            1,
            now,
        ),
        entry(
            "budget.xlsx",
            "application/vnd.ms-excel",
            450_000,
            Some(GeoLocation { lat: 41.16, lng: -8.63 }),
            0,
            now,
        ),
        entry(
            "meeting-notes.md",
            "text/markdown",
            12_000,
            Some(GeoLocation { lat: 38.80, lng: -9.38 }),
            2,
            now,
        ),
        entry(
            "logo-draft.png",
            "image/png",
            890_000,
            Some(GeoLocation { lat: 51.51, lng: -0.13 }),
            0,
            now,
        ),
        entry("app.rs", "text/x-rust", 45_000, None, 0, now),
        entry(
            "demo-video.mp4",
            "video/mp4",
            28_000_000,
            None,
            1,
            now,
        ),
        entry(
            "podcast-ep1.mp3",
            "audio/mpeg",
            15_000_000,
            Some(GeoLocation { lat: 35.68, lng: 139.69 }),
            0,
            now,
        ),
        entry("config.json", "application/json", 2_000, None, 0, now),
    ];

    // Instance-specific entries
    match instance_name {
        "Love" => {
            entries.push(entry(
                "love-presentation.pdf",
                "application/pdf",
                5_200_000,
                Some(GeoLocation { lat: 40.71, lng: -74.01 }),
                3,
                now,
            ));
            entries.push(entry(
                "love-vacation.jpg",
                "image/jpeg",
                4_500_000,
                None,
                0,
                now,
            ));
            entries.push(entry(
                "love-api-docs.html",
                "text/html",
                180_000,
                Some(GeoLocation { lat: 37.78, lng: -122.42 }),
                0,
                now,
            ));
        }
        "Joy" => {
            entries.push(entry(
                "joy-slides.pdf",
                "application/pdf",
                4_800_000,
                Some(GeoLocation { lat: 48.86, lng: 2.35 }),
                2,
                now,
            ));
            entries.push(entry(
                "joy-selfie.jpg",
                "image/jpeg",
                3_800_000,
                None,
                0,
                now,
            ));
            entries.push(entry(
                "joy-readme.md",
                "text/markdown",
                8_000,
                Some(GeoLocation { lat: 52.52, lng: 13.41 }),
                0,
                now,
            ));
        }
        _ => {
            entries.push(entry(
                "presentation.pdf",
                "application/pdf",
                5_200_000,
                Some(GeoLocation { lat: 40.71, lng: -74.01 }),
                3,
                now,
            ));
            entries.push(entry(
                "vacation-photo.jpg",
                "image/jpeg",
                4_500_000,
                None,
                0,
                now,
            ));
            entries.push(entry(
                "api-docs.html",
                "text/html",
                180_000,
                Some(GeoLocation { lat: 37.78, lng: -122.42 }),
                0,
                now,
            ));
        }
    }

    entries
}

/// Helper to build a `HomeArtifactEntry` with mock grants.
fn entry(
    name: &str,
    mime: &str,
    size: u64,
    location: Option<GeoLocation>,
    grant_count: usize,
    now: i64,
) -> HomeArtifactEntry {
    let grants = (0..grant_count)
        .map(|i| {
            let mut grantee = [0u8; 32];
            grantee[0] = (i + 1) as u8;
            indras_network::access::AccessGrant {
                grantee,
                mode: indras_network::access::AccessMode::Revocable,
                granted_at: now,
                granted_by: [0u8; 32],
            }
        })
        .collect();

    HomeArtifactEntry {
        id: mock_id(name),
        name: name.to_string(),
        mime_type: Some(mime.to_string()),
        size,
        created_at: now,
        encrypted_key: None,
        status: ArtifactStatus::Active,
        grants,
        provenance: None,
        parent: None,
        location,
    }
}
