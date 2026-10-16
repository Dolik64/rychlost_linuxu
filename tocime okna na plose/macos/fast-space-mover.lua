-- Samostatný rychlý přesouvač oken mezi plochami (Title Bar Hold + Ctrl+Arrow).
-- Obsahuje živý přepínač (Option+Ctrl+Shift+M) mezi rychlou a původní Mission Control verzí.

local M = { busy = false, hotkeys = {}, toggleHotkey = nil, enabled = false }
local events = hs.eventtap.event
local mouseAPI = hs.mouse
local timers = {}

local function clearTimers()
    for _, t in ipairs(timers) do t:stop() end
    timers = {}
end

local function later(seconds, fn)
    timers[#timers + 1] = hs.timer.doAfter(seconds, fn)
end

local function postMouse(kind, pt)
    events.newMouseEvent(events.types[kind], pt):post()
end

local function getDragPoint(win)
    local f = win:frame()
    local xOffset = math.min(math.max(math.floor(f.w * 0.35), 90), f.w - 20)
    return { x = f.x + xOffset, y = f.y + 14 }
end

function M.move(win, target)
    if M.busy then return false, 'busy' end
    if not win or not win:isStandard() or win:isFullScreen() then
        hs.alert.show('Vyber běžné okno mimo celou obrazovku.')
        return false
    end

    local screen = win:screen()
    local spaces = hs.spaces.spacesForScreen(screen)
    local current = hs.spaces.activeSpaceOnScreen(screen)
    if current == target then return true end

    local curIdx, targetIdx
    for i, id in ipairs(spaces) do
        if id == current then curIdx = i end
        if id == target then targetIdx = i end
    end
    if not curIdx or not targetIdx then return false end

    local diff = targetIdx - curIdx
    local keyDir = diff < 0 and 'left' or 'right'
    local steps = math.abs(diff)

    M.busy = true
    local started = hs.timer.absoluteTime() / 1e9
    local origMouse = mouseAPI.absolutePosition()
    local dragPt = getDragPoint(win)

    local waitStart = hs.timer.absoluteTime() / 1e9
    local function waitForModifiersRelease()
        local mods = hs.eventtap.checkKeyboardModifiers()
        if mods.alt or mods.shift or mods.cmd then
            if (hs.timer.absoluteTime() / 1e9) - waitStart > 1.5 then
                M.busy = false
                hs.alert.show('Při přesunu pusť klávesy Option/Shift.')
                return
            end
            later(0.01, waitForModifiersRelease)
            return
        end

        win:focus()
        postMouse('mouseMoved', dragPt)
        postMouse('leftMouseDown', dragPt)

        later(0.02, function()
            postMouse('leftMouseDragged', { x = dragPt.x + 1, y = dragPt.y })

            for s = 1, steps do
                later(0.02 + (s - 1) * 0.12, function()
                    events.newKeyEvent({'ctrl'}, keyDir, true):post()
                    events.newKeyEvent({'ctrl'}, keyDir, false):post()
                end)
            end

            local releaseDelay = 0.08 + steps * 0.12
            later(releaseDelay, function()
                postMouse('leftMouseDragged', dragPt)
                postMouse('leftMouseUp', dragPt)
                mouseAPI.absolutePosition(origMouse)
                win:focus()
                M.busy = false
                M.lastResult = {
                    ok = true,
                    elapsed = (hs.timer.absoluteTime() / 1e9) - started
                }
                hs.printf('Fast space move: PASS, %.2f s', M.lastResult.elapsed)
            end)
        end)
    end

    waitForModifiersRelease()
    return true
end

function M.moveDirection(win, dir)
    if M.busy then return false, 'busy' end
    win = win or hs.window.focusedWindow()
    if not win or not win:isStandard() or win:isFullScreen() then
        hs.alert.show('Vyber běžné okno mimo celou obrazovku.')
        return false
    end

    local keyDir = dir
    if dir == 'up' then keyDir = 'left'
    elseif dir == 'down' then keyDir = 'right' end

    local screen = win:screen()
    local spaces = hs.spaces.spacesForScreen(screen)
    local current = hs.spaces.activeSpaceOnScreen(screen)
    local idx
    for i, id in ipairs(spaces) do
        if id == current then idx = i; break end
    end
    if not idx then return false end

    local targetIdx = idx + (keyDir == 'left' and -1 or 1)
    if targetIdx < 1 or targetIdx > #spaces then
        return false, 'edge'
    end

    return M.move(win, spaces[targetIdx])
end

local function swapWithLegacy(enableFast)
    if not linuxControls then return end
    if enableFast then
        if linuxControls.mover and linuxControls.mover ~= M then
            M.legacyMover = linuxControls.mover
            M.legacyMoveFn = linuxControls.mover.move
            linuxControls.mover = M
            -- Přepíše i samotnou metodu .move uvnitř staré tabulky (pro případ lokální reference)
            M.legacyMover.move = M.move
        end
    else
        if M.legacyMover then
            if M.legacyMoveFn then M.legacyMover.move = M.legacyMoveFn end
            linuxControls.mover = M.legacyMover
        end
    end
end

function M.enable()
    if M.enabled then return end
    swapWithLegacy(true)
    M.enabled = true
    hs.printf('Fast space mover: ENABLED')
    hs.alert.show('Přesun oken: RYCHLÝ režim (Title Bar)')
end

function M.disable()
    if not M.enabled then return end
    clearTimers()
    M.busy = false
    swapWithLegacy(false)
    M.enabled = false
    hs.printf('Fast space mover: DISABLED (Mission Control active)')
    hs.alert.show('Přesun oken: PŮVODNÍ režim (Mission Control)')
end

function M.toggle()
    if M.enabled then M.disable() else M.enable() end
end

function M.start()
    M.stop()
    local mash = {'alt', 'ctrl', 'shift'}
    M.toggleHotkey = hs.hotkey.bind(mash, 'm', function()
        M.toggle()
    end)
    M.enable()
    return M
end

function M.stop()
    clearTimers()
    M.busy = false
    if M.enabled then swapWithLegacy(false) end
    if M.toggleHotkey then M.toggleHotkey:delete(); M.toggleHotkey = nil end
    M.enabled = false
end

return M