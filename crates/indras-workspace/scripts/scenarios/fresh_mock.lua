-- Fresh mock: clean start with auto-connect + artifact seeding.
-- Usage: ./se Love Joy --remock
-- Equivalent to --clean + automatic peer mesh + mock artifacts.
--
-- Flow: wait for app_ready → publish identity URI → connect peers → seed artifacts

indras.wait_for("app_ready")

local my_name = indras.my_name()
indras.log.info(my_name .. ": app ready, starting fresh mock setup")

-- Get our identity URI and publish it for other instances
local uri = indras.query("identity_uri")
indras.file_write("/tmp/indras-mesh/" .. my_name, uri)
indras.log.info(my_name .. ": published identity URI")

-- Discover peers by reading env var (set by se script)
local peers_str = os.getenv("INDRAS_PEERS") or ""
local peers = {}
for name in string.gmatch(peers_str, "([^,]+)") do
    if name ~= my_name then
        table.insert(peers, name)
    end
end

-- Connect to each peer
for _, peer in ipairs(peers) do
    indras.log.info(my_name .. ": waiting for " .. peer .. "'s identity URI...")
    local peer_uri = nil
    for i = 1, 120 do  -- up to 60 seconds
        peer_uri = indras.file_read("/tmp/indras-mesh/" .. peer)
        if peer_uri and #peer_uri > 0 then break end
        indras.wait(0.5)
    end

    if peer_uri and #peer_uri > 0 then
        indras.log.info(my_name .. ": connecting to " .. peer)
        indras.connect_to(peer_uri)
        indras.wait_for("peer_connected", peer, 15)
        indras.log.info(my_name .. ": connected to " .. peer)
    else
        indras.log.warn(my_name .. ": timed out waiting for " .. peer)
    end
end

-- Brief pause to let connections stabilize
indras.wait(1)

-- Set reference location (Lisbon) for distance calculations
indras.set_user_location(38.7223, -9.1393)

-- Determine the "other peer" for provenance (supports 2-instance case)
local other_peer = nil
if #peers > 0 then other_peer = peers[1] end

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
        -- First peer creates these to share with second peer
        indras.store_artifact({ name="team-photo.jpg", mime="image/jpeg", size=3100000, lat=38.73, lng=-9.15 })
        indras.store_artifact({ name="meeting-notes.md", mime="text/markdown", size=12000, lat=38.80, lng=-9.38 })
    else
        -- Second peer creates these to share with first peer
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
        -- First peer receives these FROM second peer
        indras.store_artifact({ name="demo-video.mp4", mime="video/mp4", size=28000000, from_peer=other_peer })
        indras.store_artifact({ name="vacation-photo.jpg", mime="image/jpeg", size=4500000, lat=52.52, lng=13.40, from_peer=other_peer })
    else
        -- Second peer receives these FROM first peer
        indras.store_artifact({ name="team-photo.jpg", mime="image/jpeg", size=3100000, lat=38.73, lng=-9.15, from_peer=other_peer })
        indras.store_artifact({ name="meeting-notes.md", mime="text/markdown", size=12000, lat=38.80, lng=-9.38, from_peer=other_peer })
    end
end

-- Wait for action dispatcher to process
indras.wait(2)

-- Verify
local count = indras.query("artifact_count")
indras.log.info(my_name .. ": seeded " .. count .. " artifacts")
indras.assert.ge(count, 10, "Should have at least 10 artifacts")
indras.log.info(my_name .. ": fresh mock setup complete!")
