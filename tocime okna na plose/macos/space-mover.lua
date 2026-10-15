-- Move ordinary windows through Mission Control on macOS 26.
-- Private moveWindowToSpace() can return true without moving another app's window.
local M = {busy = false}
local events = hs.eventtap.event -- Load before starting a timed operation.
local ax, mouseAPI = hs.axuielement, hs.mouse
local timers, escape, pointer, mouseDown, lastPoint = {}, nil, nil, false, nil
local function mouse(kind, point)
    lastPoint = point
    events.newMouseEvent(events.types[kind], point):post()
end
local function finish(ok, message)
    for _,t in ipairs(timers) do t:stop() end
    timers = {}
    if mouseDown then mouse('leftMouseUp', lastPoint); mouseDown = false end
    if escape then escape:delete(); escape = nil end
    hs.spaces.closeMissionControl()
    if pointer then mouseAPI.absolutePosition(pointer); pointer = nil end
    M.busy = false
    M.lastResult = {ok=ok, message=message}
    if not ok and message then hs.alert.show(message) end
end
local function later(seconds, fn)
    timers[#timers+1] = hs.timer.doAfter(seconds, function()
        local ok, err = xpcall(fn, debug.traceback)
        if not ok then hs.printf('Space move: %s', err); finish(false, 'Přesun okna selhal.') end
    end)
end
local function child(parent, identifier)
    for _,e in ipairs(parent and parent.AXChildren or {}) do
        if e.AXIdentifier == identifier then return e end
    end
end
local function displayGroup(screen)
    local dock = hs.application.get('com.apple.dock')
    local mc = dock and child(ax.applicationElement(dock), 'mc')
    for _,e in ipairs(mc and mc.AXChildren or {}) do
        if e.AXIdentifier == 'mc.display' and e.AXDisplayID == screen:id() then return e end
    end
end
local function center(frame) return {x=frame.x+frame.w/2, y=frame.y+frame.h/2} end
local function onSpace(win, id)
    for _,v in ipairs(hs.spaces.windowSpaces(win) or {}) do if v==id then return true end end
    return false
end
function M.stop() if M.busy then finish(false) end end
function M.move(win, target)
    if M.busy then return false, 'busy' end
    if not win or not win:isStandard() or win:isFullScreen() then
        hs.alert.show('Vyber běžné okno mimo celou obrazovku.'); return false
    end
    local screen = win:screen()
    local spaces = hs.spaces.windowSpaces(win) or {}
    if #spaces ~= 1 or hs.spaces.spaceType(spaces[1]) ~= 'user' then
        hs.alert.show('Okno musí patřit jedné běžné ploše.'); return false
    end
    if hs.spaces.spaceType(target) ~= 'user' or hs.spaces.spaceDisplay(target) ~= screen:getUUID() then return false end
    if onSpace(win, target) then return true end
    local index
    for i,id in ipairs(hs.spaces.spacesForScreen(screen)) do if id==target then index=i end end
    if not index then return false end
    local title = win:title()
    if not title or title=='' then title=win:application():title() end
    M.busy = true
    M.lastResult = nil
    pointer = mouseAPI.absolutePosition()
    escape = hs.hotkey.bind({}, 'escape', function() finish(false, 'Přesun zrušen.') end)
    later(12, function() finish(false, 'Přesun nedokončen. Zkontroluj polohu okna.') end)
    local function begin()
        local mods = hs.eventtap.checkKeyboardModifiers()
        if mods.alt or mods.ctrl or mods.shift or mods.cmd then later(0.05, begin); return end
        win:focus()
        hs.spaces.openMissionControl()
        local f = screen:fullFrame()
        later(0.4, function() mouse('mouseMoved', {x=f.x+f.w/2,y=f.y+10}) end)
        later(1.0, function()
            local display = displayGroup(screen)
            local windows = child(display, 'mc.windows')
            local list = child(child(display, 'mc.spaces'), 'mc.spaces.list')
            local button = list and list.AXChildren and list.AXChildren[index]
            local matches = {}
            for _,e in ipairs(windows and windows.AXChildren or {}) do
                if e.AXTitle == title then matches[#matches+1]=e end
            end
            if #matches ~= 1 then finish(false, 'Okno nelze jednoznačně najít v Mission Control.'); return end
            if not button or not button.AXFrame or button.AXFrame.y < f.y then
                finish(false, 'Náhled cílové plochy není dostupný.'); return
            end
            local from, to = center(matches[1].AXFrame), center(button.AXFrame)
            mouse('mouseMoved', from)
            mouse('leftMouseDown', from); mouseDown = true
            for n=1,15 do later(n*0.025, function()
                mouse('leftMouseDragged', {x=from.x+(to.x-from.x)*n/15, y=from.y+(to.y-from.y)*n/15})
            end) end
            later(0.5, function() mouse('leftMouseUp', to); mouseDown=false end)
            later(0.95, function()
                if not onSpace(win,target) then finish(false, 'macOS přesun okna neprovedl.'); return end
                button:performAction('AXPress')
                later(0.65, function()
                    win:focus()
                    later(0.2, function()
                        local focused=hs.window.focusedWindow()
                        local ok = onSpace(win,target) and hs.spaces.activeSpaceOnScreen(screen)==target
                            and focused and focused:id()==win:id()
                        finish(ok, ok and 'moved' or 'Okno přesunuto, aktivace se nezdařila.')
                    end)
                end)
            end)
        end)
    end
    later(0.05, begin)
    return true
end
return M
