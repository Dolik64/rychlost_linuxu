-- Hammerspoon: cycle windows in a fixed order, wrapping in both directions.
-- Load with dofile(...).start(); defaults to Ctrl+Option+Left/Right.
local M = {}
local filter, spaceWatcher
local hotkeys = {}
local ranks, nextRank = {}, 0
local pendingID, pendingUntil, pendingScreen
local onlyCurrentScreen = true
local enabledWhen

local function resetPending()
    pendingID, pendingUntil, pendingScreen = nil, nil, nil
end

local function candidates(screenID)
    local result = {}
    for _, win in ipairs(filter:getWindows(hs.window.filter.sortByCreated)) do
        local id = win:id()
        local screen = win:screen()
        if id and (not onlyCurrentScreen or (screen and screen:id() == screenID)) then
            if not ranks[id] then
                nextRank = nextRank + 1
                ranks[id] = nextRank
            end
            result[#result + 1] = win
        end
    end
    -- Retain our own ranks: equal creation timestamps must not reorder windows.
    table.sort(result, function(a, b) return ranks[a:id()] < ranks[b:id()] end)
    return result
end

local function cycle(step)
    if not filter or (enabledWhen and not enabledWhen()) then return end
    local focused = hs.window.focusedWindow()
    local screen = (focused and focused:screen()) or hs.screen.mainScreen()
    local screenID = screen and screen:id()
    local windows = candidates(screenID)
    if #windows <= 1 then resetPending(); return end

    local now = hs.timer.absoluteTime() / 1e9
    local currentID = focused and focused:id()
    if pendingID and now < pendingUntil and pendingScreen == screenID then
        -- AX focus can arrive after another keypress. Advance from our target.
        for _, win in ipairs(windows) do
            if win:id() == pendingID then currentID = pendingID; break end
        end
    end

    local index
    for i, win in ipairs(windows) do
        if win:id() == currentID then index = i; break end
    end
    local targetIndex
    if index then
        targetIndex = ((index - 1 + step) % #windows) + 1
    else
        targetIndex = step > 0 and 1 or #windows
    end

    local target = windows[targetIndex]
    pendingID, pendingUntil, pendingScreen = target:id(), now + 0.8, screenID
    local ok, result = pcall(function() return target:focus() end)
    if not ok or not result then
        resetPending()
        hs.printf("Window cycle: could not focus window %s", tostring(target:id()))
        return nil
    end
    return target:id()
end

function M.next() return cycle(1) end
function M.previous() return cycle(-1) end

function M.stop()
    for _, key in ipairs(hotkeys) do key:delete() end
    hotkeys = {}
    if spaceWatcher then spaceWatcher:stop(); spaceWatcher = nil end
    if filter then filter:unsubscribeAll():pause(); filter = nil end
    ranks, nextRank = {}, 0
    resetPending()
    return M
end

function M.start(options)
    M.stop()
    options = options or {}
    onlyCurrentScreen = options.onlyCurrentScreen ~= false
    enabledWhen = options.enabledWhen
    local mods = options.modifiers or {"ctrl", "alt"}
    filter = hs.window.filter.new():setOverrideFilter({
        currentSpace = true,
        visible = true,
        -- Auxiliary dialogs may activate their parent instead of themselves.
        -- Including one would send the cycle back to the parent's position.
        allowRoles = {"AXStandardWindow"},
    })
    filter:subscribe(hs.window.filter.windowDestroyed, function(win)
        local id = win:id()
        if id then ranks[id] = nil end
        if id == pendingID then resetPending() end
    end)
    spaceWatcher = hs.spaces.watcher.new(resetPending):start()

    local function bind(key, callback)
        local binding = hs.hotkey.bind(mods, key, callback, nil, callback)
        if not binding then
            M.stop()
            error("Window cycle: cannot register shortcut " .. key)
        end
        hotkeys[#hotkeys + 1] = binding
    end
    bind("right", M.next)
    bind("left", M.previous)
    return M
end

return M
