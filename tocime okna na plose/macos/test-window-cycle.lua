-- Regression tests for wraparound and delayed focus, without running Hammerspoon.
local tests = 0
local function equal(actual, expected, label)
    assert(actual == expected, (label or "value") .. ": expected " ..
        tostring(expected) .. ", got " .. tostring(actual))
end

local function setup(options)
    local state = {now = 1, windows = {}, focused = nil, targets = {}, keys = {},
                   settle = true, mainScreenID = 1}
    local function screen(id) return {id = function() return id end} end
    function state.window(id, screenID)
        local win = {windowID = id, screenID = screenID or 1}
        function win:id() return self.windowID end
        function win:screen() return screen(self.screenID) end
        function win:focus()
            if self.fail then error("window closed") end
            state.targets[#state.targets + 1] = self:id()
            if state.settle then state.focused = self end
            return self
        end
        return win
    end
    function state.tick(seconds) state.now = state.now + (seconds or 0.4) end
    function state.last() return state.targets[#state.targets] end
    local wf = {}
    function wf:setOverrideFilter(settings) state.settings = settings; return self end
    function wf:getWindows(sort)
        equal(sort, "created")
        -- The filtering itself is Hammerspoon's responsibility.
        return state.windows
    end
    function wf:subscribe(event, callback) state.destroy = callback; return self end
    function wf:unsubscribeAll() state.unsubscribed = true; return self end
    function wf:pause() state.paused = true; return self end
    local watcher = {}
    function watcher:start() return self end
    function watcher:stop() state.watcherStopped = true end
    _G.hs = {
        window = {
            focusedWindow = function() return state.focused end,
            filter = {new = function() return wf end, sortByCreated = "created",
                      windowDestroyed = "destroyed"},
        },
        screen = {mainScreen = function() return screen(state.mainScreenID) end},
        timer = {absoluteTime = function() return state.now * 1e9 end},
        spaces = {watcher = {new = function(callback)
            state.spaceChanged = callback; return watcher
        end}},
        hotkey = {bind = function(mods, key, pressed, released, repeated)
            local binding = {pressed = pressed, repeated = repeated, mods = mods}
            function binding:delete() self.deleted = true end
            state.keys[key] = binding
            return binding
        end},
        printf = function() state.loggedError = true end,
    }
    local module = dofile("window-cycle.lua").start(options)
    equal(state.settings.currentSpace, true)
    equal(state.settings.visible, true)
    equal(#state.settings.allowRoles, 1)
    equal(state.settings.allowRoles[1], "AXStandardWindow")
    local a, b, c = state.window(1), state.window(2), state.window(3)
    state.windows, state.focused = {a, b, c}, a
    return state, module, a, b, c
end

local function test(name, fn)
    fn()
    tests = tests + 1
    print("PASS " .. name)
end

test("repeated forward and reverse wraparound after releasing modifiers", function()
    local s, m = setup()
    for i = 1, 30 do s.tick(); m.next(); equal(s.last(), (i % 3) + 1) end
    for i = 1, 30 do s.tick(); m.previous(); equal(s.last(), ((-i) % 3) + 1) end
end)

test("rapid input advances before focus notifications arrive", function()
    local s, m = setup()
    s.settle = false
    for i = 1, 20 do s.tick(0.01); m.next(); equal(s.last(), (i % 3) + 1) end
    m.previous(); equal(s.last(), 2)
end)

test("late and out of order focus does not move the cycle backwards", function()
    local s, m, a, b = setup()
    s.settle = false
    m.next(); equal(s.last(), 2)
    s.tick(0.01); m.next(); equal(s.last(), 3)
    s.focused = b
    s.tick(0.01); m.next(); equal(s.last(), 1)
    s.tick(0.4); s.focused = a
    m.next(); equal(s.last(), 2)
end)

test("changing system enumeration does not reorder the cycle", function()
    local s, m, a, b, c = setup()
    m.next(); equal(s.last(), 2)
    s.windows = {c, a, b}
    s.tick(); m.next(); equal(s.last(), 3)
    s.tick(); m.next(); equal(s.last(), 1)
end)

test("closing the pending target and adding a window", function()
    local s, m, a, b, c = setup()
    s.settle = false
    m.next()
    s.windows = {a, c}; s.destroy(b)
    m.next(); equal(s.last(), 3)
    local d = s.window(4)
    s.windows = {d, c, a}
    m.next(); equal(s.last(), 4)
    m.next(); equal(s.last(), 1)
end)

test("space change clears the pending target even with the same windows", function()
    local s, m = setup()
    s.settle = false
    m.next(); equal(s.last(), 2)
    s.spaceChanged()
    m.next(); equal(s.last(), 2)
end)

test("only the focused monitor participates by default", function()
    local s, m, a, b, c = setup()
    c.screenID = 2
    m.next(); equal(s.last(), 2)
    s.tick(); m.next(); equal(s.last(), 1)
    s.focused = c; s.tick(); m.next()
    equal(#s.targets, 2)
end)

test("all monitors are optional", function()
    local s, m, a, b, c = setup({onlyCurrentScreen = false})
    c.screenID = 2
    m.next(); s.tick(); m.next(); equal(s.last(), 3)
    s.tick(); m.next(); equal(s.last(), 1)
end)

test("zero, one and no focused window", function()
    local s, m, a = setup()
    s.windows = {}; m.next(); m.previous(); equal(#s.targets, 0)
    s.windows = {a}; m.next(); m.previous(); equal(#s.targets, 0)
    s.windows = {a, s.window(2)}; s.focused = nil
    m.previous(); equal(s.last(), 2)
    s.tick(); s.focused = nil
    m.next(); equal(s.last(), 1)
end)

test("failed activation clears optimistic state", function()
    local s, m, a, b = setup()
    b.fail = true
    m.next(); equal(s.loggedError, true)
    b.fail = false
    m.next(); equal(s.last(), 2)
end)

test("keypress and hold callbacks work and stop removes bindings", function()
    local s, m = setup()
    s.keys.right.pressed(); equal(s.last(), 2)
    s.keys.right.repeated(); equal(s.last(), 3)
    s.keys.left.pressed(); equal(s.last(), 2)
    m.stop()
    equal(s.keys.right.deleted, true)
    equal(s.keys.left.deleted, true)
    equal(s.unsubscribed, true)
    equal(s.paused, true)
    equal(s.watcherStopped, true)
    m.next(); equal(#s.targets, 3)
end)

test("cycling is suspended during a Space transition", function()
    local enabled = false
    local s, m = setup({enabledWhen = function() return enabled end})
    m.next(); s.keys.left.pressed(); equal(#s.targets, 0)
    enabled = true
    m.next(); equal(s.last(), 2)
end)

test("half-second focus delay still advances at human repeat speed", function()
    local s, m = setup()
    s.settle = false
    for i=1,9 do s.tick(0.4); m.next(); equal(s.last(), (i%3)+1) end
end)

print(string.format("%d tests passed", tests))
