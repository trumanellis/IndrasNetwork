-- mp_helpers.lua
--
-- Multi-process coordination helpers for cross-process vault sync tests.
-- Uses file-based signaling: each process writes signals to a shared
-- coordination directory, and peers poll for signals from each other.

local M = {}

--- Write a signal file to the coordination directory.
--- @param coord_dir string  Shared coordination directory path
--- @param name string       Signal name (becomes the filename)
--- @param data string       Signal data (written as file content)
function M.write_signal(coord_dir, name, data)
    local path = coord_dir .. "/" .. name
    local f = assert(io.open(path, "w"), "Failed to write signal: " .. path)
    f:write(data)
    f:close()
end

--- Wait for a signal file to appear in the coordination directory.
--- Polls every 0.5s until the file exists and is non-empty.
--- @param coord_dir string  Shared coordination directory path
--- @param name string       Signal name to wait for
--- @param timeout number    Timeout in seconds (default 30)
--- @return string           Signal data (file content)
function M.wait_for_signal(coord_dir, name, timeout)
    timeout = timeout or 30
    local path = coord_dir .. "/" .. name
    local deadline = os.time() + timeout

    while os.time() < deadline do
        local f = io.open(path, "r")
        if f then
            local data = f:read("*a")
            f:close()
            if data and #data > 0 then
                return data
            end
        end
        indras.sleep(0.5)
    end

    error("Timed out waiting for signal: " .. name .. " (after " .. timeout .. "s)")
end

--- Read a file from disk (for verifying synced vault content).
--- @param path string  Full path to the file
--- @return string|nil  File content, or nil if not found
function M.read_file(path)
    local f = io.open(path, "rb")
    if not f then return nil end
    local content = f:read("*a")
    f:close()
    return content
end

--- Wait for a file to appear on disk with expected content.
--- @param path string     Full path to the file
--- @param expected string Expected content
--- @param timeout number  Timeout in seconds (default 30)
function M.wait_for_file_content(path, expected, timeout)
    timeout = timeout or 30
    local deadline = os.time() + timeout

    while os.time() < deadline do
        local content = M.read_file(path)
        if content == expected then
            return content
        end
        indras.sleep(1)
    end

    local actual = M.read_file(path) or "(file not found)"
    error("Timed out waiting for file content at " .. path ..
          "\n  expected: " .. expected ..
          "\n  actual:   " .. actual)
end

return M
