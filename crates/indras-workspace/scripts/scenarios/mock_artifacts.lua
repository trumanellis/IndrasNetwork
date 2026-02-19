-- Mock artifact seeding via Lua scripting.
-- Usage: ./se Love Joy --clean --mock
-- Each instance creates 10 artifacts: 8 personal + 2 shared with the peer.
-- The peer receives those 2 as "from_peer" entries with real provenance.

indras.wait_for("app_ready")
indras.log.info("Seeding mock artifacts...")

-- Get our instance name
local identity = indras.query("identity")
local my_name = identity.name

-- Determine the other peer name
local other_peer = nil
if my_name == "Love" then other_peer = "Joy"
elseif my_name == "Joy" then other_peer = "Love"
end

-- Set reference location (Lisbon) for distance calculations
indras.set_user_location(38.7223, -9.1393)

-- Phase 1: Self-created artifacts (each peer keeps these)
indras.store_artifact({ name="project-spec.pdf", mime="application/pdf", size=2400000, lat=38.72, lng=-9.14 })
indras.store_artifact({ name="budget.xlsx", mime="application/vnd.ms-excel", size=450000, lat=41.16, lng=-8.63 })
indras.store_artifact({ name="logo-draft.png", mime="image/png", size=890000, lat=51.51, lng=-0.13 })
indras.store_artifact({ name="app.rs", mime="text/x-rust", size=45000 })
indras.store_artifact({ name="podcast-ep1.mp3", mime="audio/mpeg", size=15000000, lat=35.68, lng=139.69 })
indras.store_artifact({ name="config.json", mime="application/json", size=2000 })
indras.store_artifact({ name=my_name .. "-presentation.pdf", mime="application/pdf", size=5200000, lat=40.71, lng=-74.01 })
indras.store_artifact({ name=my_name .. "-api-docs.html", mime="text/html", size=180000, lat=37.78, lng=-122.42 })

-- Phase 2: Each peer creates the artifacts it will SHARE with the other.
-- Uses fixed names so both sides compute the same BLAKE3 ArtifactId.
if other_peer then
    if my_name < other_peer then
        -- First peer (Joy) creates these to share with second peer (Love)
        indras.store_artifact({ name="team-photo.jpg", mime="image/jpeg", size=3100000, lat=38.73, lng=-9.15 })
        indras.store_artifact({ name="meeting-notes.md", mime="text/markdown", size=12000, lat=38.80, lng=-9.38 })
    else
        -- Second peer (Love) creates these to share with first peer (Joy)
        indras.store_artifact({ name="demo-video.mp4", mime="video/mp4", size=28000000 })
        indras.store_artifact({ name="vacation-photo.jpg", mime="image/jpeg", size=4500000, lat=52.52, lng=13.40 })
    end
end

-- Wait for action dispatcher to process all store_artifact actions
indras.wait(2)

-- Phase 3: Grant access to the peer for shared artifacts (real P2P sync)
if other_peer then
    if my_name < other_peer then
        indras.grant_artifact("team-photo.jpg", other_peer)
        indras.grant_artifact("meeting-notes.md", other_peer)
    else
        indras.grant_artifact("demo-video.mp4", other_peer)
        indras.grant_artifact("vacation-photo.jpg", other_peer)
    end
end

indras.wait(1)

-- Phase 4: Recipient creates "received" entries with provenance.
-- Same name/mime/size = same BLAKE3 hash = same ArtifactId as the creator's entry.
if other_peer then
    if my_name < other_peer then
        -- First peer (Joy) receives these FROM second peer (Love)
        indras.store_artifact({ name="demo-video.mp4", mime="video/mp4", size=28000000, from_peer=other_peer })
        indras.store_artifact({ name="vacation-photo.jpg", mime="image/jpeg", size=4500000, lat=52.52, lng=13.40, from_peer=other_peer })
    else
        -- Second peer (Love) receives these FROM first peer (Joy)
        indras.store_artifact({ name="team-photo.jpg", mime="image/jpeg", size=3100000, lat=38.73, lng=-9.15, from_peer=other_peer })
        indras.store_artifact({ name="meeting-notes.md", mime="text/markdown", size=12000, lat=38.80, lng=-9.38, from_peer=other_peer })
    end
end

-- Wait for action dispatcher to process all store_artifact actions
indras.wait(2)

-- Verify
local count = indras.query("artifact_count")
indras.log.info("Seeded " .. count .. " artifacts")
indras.assert.ge(count, 10, "Should have at least 10 artifacts")
