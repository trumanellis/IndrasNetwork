-- SDK Helpers for stress testing and verification
local sdk = {}

-- Configuration levels (follows existing pattern)
sdk.LEVELS = {
    quick = { realms = 3, messages = 100, documents = 5, members = 3 },
    medium = { realms = 10, messages = 1000, documents = 20, members = 10 },
    full = { realms = 26, messages = 10000, documents = 100, members = 26 }
}

-- Get current stress level from environment
function sdk.get_level()
    return os.getenv("STRESS_LEVEL") or "quick"
end

function sdk.get_config()
    local level = sdk.get_level()
    return sdk.LEVELS[level] or sdk.LEVELS.quick
end

-- Create a new correlation context for SDK scenarios
function sdk.new_context(scenario_name)
    local ctx = indras.correlation.new_root()
    ctx = ctx:with_tag("scenario", scenario_name)
    ctx = ctx:with_tag("level", sdk.get_level())
    ctx = ctx:with_tag("subsystem", "sdk")
    return ctx
end

-- Latency tracking for SDK operations
function sdk.latency_tracker()
    return {
        samples = {},
        add = function(self, latency_us)
            table.insert(self.samples, latency_us)
        end,
        count = function(self)
            return #self.samples
        end,
        average = function(self)
            if #self.samples == 0 then return 0 end
            local sum = 0
            for _, v in ipairs(self.samples) do sum = sum + v end
            return sum / #self.samples
        end,
        percentile = function(self, p)
            if #self.samples == 0 then return 0 end
            local sorted = {}
            for _, v in ipairs(self.samples) do table.insert(sorted, v) end
            table.sort(sorted)
            local idx = math.ceil(#sorted * p / 100)
            return sorted[math.max(1, idx)]
        end,
        p50 = function(self) return self:percentile(50) end,
        p95 = function(self) return self:percentile(95) end,
        p99 = function(self) return self:percentile(99) end
    }
end

-- Message generation helpers
function sdk.random_message(min_len, max_len)
    min_len = min_len or 10
    max_len = max_len or 100
    local len = math.random(min_len, max_len)
    local chars = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 "
    local msg = ""
    for _ = 1, len do
        local idx = math.random(1, #chars)
        msg = msg .. chars:sub(idx, idx)
    end
    return msg
end

-- Document data generation
function sdk.random_document_data()
    return {
        title = "Doc-" .. math.random(1000, 9999),
        content = sdk.random_message(50, 500),
        version = 1,
        timestamp = os.time()
    }
end

-- Wait for async operation with timeout
function sdk.wait_for(predicate, max_wait_ms, check_interval_ms)
    max_wait_ms = max_wait_ms or 5000
    check_interval_ms = check_interval_ms or 100
    local elapsed = 0
    while elapsed < max_wait_ms do
        if predicate() then return true end
        -- Note: In real async, this would yield
        elapsed = elapsed + check_interval_ms
    end
    return false
end

-- Assert with detailed SDK context
function sdk.assert_metric(name, actual, min_val, max_val, ctx)
    if actual < min_val or actual > max_val then
        indras.log.error("SDK metric out of range", {
            trace_id = ctx and ctx.trace_id,
            metric = name,
            actual = actual,
            min = min_val,
            max = max_val
        })
        indras.assert.fail(string.format(
            "%s out of range: %s (expected %s-%s)",
            name, tostring(actual), tostring(min_val), tostring(max_val)
        ))
    end
end

-- Build result table for SDK scenarios
function sdk.result_builder(scenario_name)
    return {
        scenario = scenario_name,
        level = sdk.get_level(),
        metrics = {},
        add = function(self, key, value)
            self.metrics[key] = value
            return self
        end,
        build = function(self)
            local result = {
                scenario = self.scenario,
                level = self.level
            }
            for k, v in pairs(self.metrics) do
                result[k] = v
            end
            return result
        end
    }
end

return sdk
