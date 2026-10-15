-- Run from the Hammerspoon console with dofile(...). Temporarily focuses windows.
-- Records window IDs only, restores the initial window, and prints the result.
assert(windowCycle and hs.accessibilityState(false), "Window cycle/accessibility unavailable")
if windowCycleProbe and not windowCycleProbe.done then
    error("A live cycle check is already running")
end
local original = hs.window.focusedWindow()
local screenID = hs.screen.mainScreen():id()
local filter = hs.window.filter.new():setOverrideFilter({
    currentSpace = true, visible = true,
    allowRoles = {"AXStandardWindow"},
})
local expected, count = {}, 0
for _, win in ipairs(filter:getWindows()) do
    if win:screen() and win:screen():id() == screenID then
        expected[win:id()] = true
        count = count + 1
    end
end
filter:unsubscribeAll():pause()
assert(count > 1, "At least two windows are required")
local p = {forward = {}, backward = {}, count = count, done = false}
windowCycleProbe = p

local function finish(err)
    p.done = true
    p.error = err
    if original then pcall(function() original:focus() end) end
    print("Window cycle live check:", err and "FAIL" or "PASS", "windows:", count)
    print("Forward IDs:", table.concat(p.forward, ","))
    print("Backward IDs:", table.concat(p.backward, ","))
    if err then print(err) end
end

local function verify()
    for _, sequence in ipairs({p.forward, p.backward}) do
        local seen = {}
        for i = 1, count do
            local id = sequence[i]
            if not expected[id] or seen[id] then
                return "A window was skipped, repeated, or outside the expected set"
            end
            seen[id] = true
        end
        if sequence[count + 1] ~= sequence[1] then
            return "The cycle did not wrap to its first window"
        end
    end
    for i, id in ipairs(p.backward) do
        if id ~= p.forward[((count - i) % count) + 1] then
            return "Backward order differs from forward order"
        end
    end
end

local step
step = function()
    local forward = #p.forward < count + 1
    local sequence = forward and p.forward or p.backward
    if not forward and #sequence == count + 1 then finish(verify()); return end
    local ok, requestedID = pcall(forward and windowCycle.next or windowCycle.previous)
    if not ok then finish(tostring(requestedID)); return end
    p.timer = hs.timer.doAfter(0.35, function()
        local focused = hs.window.focusedWindow()
        local actualID = (focused and focused:id()) or -1
        if actualID ~= requestedID then
            finish("Focus mismatch: requested " .. tostring(requestedID) .. ", actual " .. actualID)
            return
        end
        sequence[#sequence + 1] = actualID
        p.timer = hs.timer.doAfter(0.2, step)
    end)
end
print("Window cycle live check started:", count, "windows, both directions")
p.timer = hs.timer.doAfter(0.5, step)
