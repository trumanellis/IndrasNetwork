-- Two-peer connect and chat test
-- Run with: ./scripts/run-lua-test.sh scripts/scenarios/two_peer_chat.lua

local me = indras.my_name()
indras.log.info("Starting as " .. me)

-- Both instances wait for app to be ready
indras.wait_for("app_ready")

if me == "Joy" then
    -- Joy has Love's code (passed via env)
    local love_code = os.getenv("LOVE_CODE") or "indras://test-love-code"

    indras.wait(2) -- let Love's instance fully start
    indras.open_contacts()
    indras.paste_code(love_code)
    indras.click_connect()
    indras.wait_for("peer_connected", "Love", 15)
    indras.log.info("Connected to Love!")

    -- Navigate to Love's contact and send a message
    indras.click_sidebar("Love")
    indras.wait(1)
    indras.type_message("Hello from Joy!")
    indras.send_message()

    -- Verify
    local count = indras.query("peer_count")
    indras.assert.ge(count, 1, "Should have at least 1 peer")
    indras.log.info("Test passed: Joy sent message")

elseif me == "Love" then
    -- Love waits for Joy to connect
    indras.wait_for("peer_connected", "Joy", 20)
    indras.log.info("Joy connected!")

    -- Navigate to Joy's contact
    indras.click_sidebar("Joy")

    -- Verify peer count
    local count = indras.query("peer_count")
    indras.assert.ge(count, 1, "Should have at least 1 peer")
    indras.log.info("Test passed: Love sees Joy")
end
