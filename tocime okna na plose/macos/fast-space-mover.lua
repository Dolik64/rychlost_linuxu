-- User's fast-move idea, with a single owner for cancellation and fallback.
-- Option+Control+Shift+M switches between the fast and Mission Control paths.
local root=debug.getinfo(1,'S').source:sub(2):match('(.*/)') or './'
local M={busy=false,enabled=false}
local legacy=dofile(root..'space-mover.lua')
local events=hs.eventtap.event
local job, timers, escape, canToggle
local function now() return hs.timer.absoluteTime()/1e9 end
local function onSpace(win,id)
    local spaces=hs.spaces.windowSpaces(win) or {}
    return #spaces==1 and spaces[1]==id
end
local function near(a,b)
    if not a or not b then return false end
    for _,k in ipairs({'x','y','w','h'}) do if math.abs(a[k]-b[k])>3 then return false end end
    return true
end
local function missionControl()
    local dock=hs.application.get('com.apple.dock')
    local element=dock and hs.axuielement.applicationElement(dock)
    for _,e in ipairs(element and element.AXChildren or {}) do
        if e.AXIdentifier=='mc' then return true end
    end
    return false
end
local function mouse(kind,point)
    job.lastPoint=point
    events.newMouseEvent(events.types[kind],point):setFlags({}):post()
end
local function releaseMouse()
    if job and job.mouseDown then
        -- Clear only after posting, so cleanup can retry if the event API throws.
        mouse('leftMouseUp',job.lastPoint)
        job.mouseDown=false
    end
end
local function deleteEscape()
    if escape then escape:delete(); escape=nil end
end
local function clearTimers()
    for _,t in ipairs(timers or {}) do t:stop() end
    timers={}
end
local function finish(ok,message)
    if not job then return end
    local completed=job
    clearTimers()
    if completed.keyDown then
        pcall(function() events.newKeyEvent({'ctrl','fn'},completed.keyDown,false):post() end)
    end
    pcall(releaseMouse)
    pcall(deleteEscape)
    if legacy.busy then pcall(legacy.stop) end
    if not ok then pcall(hs.spaces.closeMissionControl) end
    if completed.dragged then pcall(function() completed.win:setFrame(completed.frame,0) end) end
    if completed.pointer then pcall(hs.mouse.absolutePosition,completed.pointer) end
    job=nil; M.busy=false
    M.lastResult={ok=ok,message=message,elapsed=now()-completed.started,
        activeElapsed=completed.releasedAt and now()-completed.releasedAt,
        method=completed.method,fallbackReason=completed.fallbackReason,
        fastAttempt=completed.fastAttempt,legacyResult=completed.legacyResult}
    hs.printf('Space move (%s): %s, %.2f s',completed.method,ok and 'PASS' or 'FAIL',M.lastResult.elapsed)
    if not ok and message then hs.alert.show(message) end
end
local function later(seconds,fn)
    local owner=job
    timers[#timers+1]=hs.timer.doAfter(seconds,function()
        if job~=owner then return end
        local ok,err=xpcall(fn,debug.traceback)
        if not ok then
            hs.printf('Fast space move error: %s',err)
            finish(false,'Přesun okna selhal; ovládání bylo uvolněno.')
        end
    end)
end
local function waitUntil(test,success,timeout,onTimeout)
    local deadline=now()+timeout
    local function check()
        if test() then success()
        elseif now()>=deadline then onTimeout()
        else later(0.04,check) end
    end
    later(0.04,check)
end
local function gotoSpace(target)
    local originalWait=hs.spaces.MCwaitTime
    if hs.screen.accessibilitySettings().ReduceMotion then hs.spaces.MCwaitTime=0.15 end
    local called,ok,err=pcall(hs.spaces.gotoSpace,target)
    hs.spaces.MCwaitTime=originalWait
    if not called then error(ok) end
    return ok,err
end
local function verified()
    local focused=hs.window.focusedWindow()
    return onSpace(job.win,job.target)
        and hs.spaces.activeSpaceOnScreen(job.screen)==job.target
        and not missionControl()
        and focused and focused:id()==job.win:id()
        and near(job.win:frame(),job.frame)
end
local function focusWindow()
    local focused=hs.window.focusedWindow()
    if not focused or focused:id()~=job.win:id() then job.win:focus() end
end
local function completeMove()
    releaseMouse()
    if not near(job.win:frame(),job.frame) then job.win:setFrame(job.frame,0) end
    focusWindow()
    waitUntil(verified,function() finish(true,'moved') end,1.5,
        function() finish(false,'Přesun, aktivaci a rozměry okna se nepodařilo potvrdit.') end)
end
local function fallback(reason)
    job.fallbackReason=reason
    if reason then job.fastAttempt=job.method end
    releaseMouse()
    hs.spaces.closeMissionControl()
    if job.dragged then job.win:setFrame(job.frame,0); job.dragged=false end
    if job.pointer then hs.mouse.absolutePosition(job.pointer) end
    if onSpace(job.win,job.target) then
        gotoSpace(job.target); completeMove(); return
    end
    if not onSpace(job.win,job.source) then
        finish(false,'Plocha okna se během přesunu změnila.'); return
    end
    job.method='mission-control'
    focusWindow()
    waitUntil(function()
        local w=hs.window.focusedWindow()
        return hs.spaces.activeSpaceOnScreen(job.screen)==job.source
            and w and w:id()==job.win:id() and not missionControl()
    end,function()
        deleteEscape() -- The legacy mover owns Escape while dragging its thumbnail.
        if not legacy.move(job.win,job.target) then finish(false,'Přesun byl odmítnut.'); return end
        waitUntil(function() return not legacy.busy end,function()
            job.legacyResult=legacy.lastResult
            if legacy.lastResult and legacy.lastResult.ok then completeMove()
            else finish(false,'Přesun přes Mission Control nebyl dokončen.') end
        end,10,function() finish(false,'Přesun přes Mission Control vypršel.') end)
    end,2,function() finish(false,'Původní plochu okna nelze aktivovat.') end)
end
local function dragPoint(win,frame)
    local windowElement=hs.axuielement.windowElement(win)
    local allowed={AXWindow=true,AXStaticText=true,AXToolbar=true}
    local interactive={AXButton=true,AXMenuButton=true,AXPopUpButton=true,AXTextField=true,
        AXTextArea=true,AXComboBox=true,AXCheckBox=true,AXRadioButton=true,AXLink=true}
    for _,ratio in ipairs({0.35,0.65,0.9}) do
        local point={x=frame.x+math.min(math.max(math.floor(frame.w*ratio),90),frame.w-20),y=frame.y+14}
        local element=hs.axuielement.systemElementAtPosition(point)
        if element and allowed[element.AXRole] and (element==windowElement or element.AXWindow==windowElement) then
            local ancestor,blocked=element,false
            for _=1,10 do
                if not ancestor or ancestor==windowElement then break end
                if interactive[ancestor.AXRole] then blocked=true; break end
                ancestor=ancestor.AXParent
            end
            if not blocked and ancestor==windowElement then return point end
        end
    end
end
local function tryFast()
    job.method='direct'
    -- A true return value is insufficient on macOS 26: verify actual membership.
    pcall(hs.spaces.moveWindowToSpace,job.win:id(),job.target)
    if onSpace(job.win,job.target) then
        gotoSpace(job.target); completeMove(); return
    end
    -- The system shortcut can carry a held window without opening Mission Control.
    -- Only use one adjacent Space; fullscreen/intermediate Spaces use the fallback.
    local list=hs.spaces.spacesForScreen(job.screen) or {}
    local sourceIndex,targetIndex
    for i,id in ipairs(list) do
        if id==job.source then sourceIndex=i end
        if id==job.target then targetIndex=i end
    end
    if not sourceIndex or not targetIndex or math.abs(targetIndex-sourceIndex)~=1 then
        fallback('Cílová plocha není bezprostředně sousední.'); return
    end
    local direction=targetIndex<sourceIndex and 'left' or 'right'
    job.method='titlebar'
    local f=job.frame
    local point=dragPoint(job.win,f)
    if not point then fallback('Na titulkové liště není bezpečné místo pro tažení.'); return end
    mouse('mouseMoved',point)
    job.mouseDown=true; job.dragged=true
    mouse('leftMouseDown',point)
    later(0.02,function()
        mouse('leftMouseDragged',{x=point.x+12,y=point.y+2})
        later(0.12,function()
            if near(job.win:frame(),job.frame) then
                fallback('Titulkovou lištu se nepodařilo uchopit.'); return
            end
            -- macOS's arrow shortcut includes the secondary-function flag.
            job.keyDown=direction
            events.newKeyEvent({'ctrl','fn'},direction,true):post()
            events.newKeyEvent({'ctrl','fn'},direction,false):post()
            job.keyDown=nil
            waitUntil(function()
                return hs.spaces.activeSpaceOnScreen(job.screen)==job.target and not missionControl()
            end,function()
                mouse('leftMouseDragged',point); releaseMouse()
                waitUntil(function() return onSpace(job.win,job.target) end,completeMove,0.25,
                    function() fallback('Přetažení titulkové lišty okno nepřesunulo.') end)
            end,1.5,function() fallback('Přechod s uchopeným oknem vypršel.') end)
        end)
    end)
end
function M.move(win,target)
    if M.busy then return false,'busy' end
    if not win or not win:isStandard() or win:isFullScreen() then
        hs.alert.show('Vyber běžné okno mimo celou obrazovku.'); return false
    end
    local screen=win:screen()
    local spaces=hs.spaces.windowSpaces(win) or {}
    if not screen or #spaces~=1 or hs.spaces.spaceType(spaces[1])~='user' then
        hs.alert.show('Okno musí patřit jedné běžné ploše.'); return false
    end
    if hs.spaces.spaceType(target)~='user' or hs.spaces.spaceDisplay(target)~=screen:getUUID() then return false end
    if spaces[1]==target then return false,'already-there' end
    M.busy=true; M.lastResult=nil; timers={}
    job={win=win,target=target,source=spaces[1],screen=screen,frame=win:frame(),
        started=now(),method=M.enabled and 'fast' or 'mission-control'}
    escape=hs.hotkey.bind({},'escape',function() finish(false,'Přesun zrušen.') end)
    later(12,function() finish(false,'Přesun nedokončen. Zkontroluj polohu okna.') end)
    local function begin()
        local mods=hs.eventtap.checkKeyboardModifiers()
        if mods.alt or mods.ctrl or mods.shift or mods.cmd then later(0.04,begin); return end
        job.releasedAt=now(); job.pointer=hs.mouse.absolutePosition()
        if not M.enabled then fallback(); return end
        focusWindow()
        waitUntil(function()
            local focused=hs.window.focusedWindow()
            return focused and focused:id()==win:id()
                and hs.spaces.activeSpaceOnScreen(screen)==spaces[1]
        end,tryFast,1.5,function() finish(false,'Okno nelze aktivovat.') end)
    end
    later(0.01,begin)
    return true
end
local function setEnabled(enabled,silent)
    if M.busy or (canToggle and not canToggle()) then
        if not silent then hs.alert.show('Počkej na dokončení přesunu.') end
        return false
    end
    M.enabled=enabled
    if not silent then hs.alert.show(enabled and 'Přesun: rychlý režim' or 'Přesun: Mission Control') end
    return true
end
function M.enable() return setEnabled(true) end
function M.disable() return setEnabled(false) end
function M.toggle() return setEnabled(not M.enabled) end
function M.stop()
    if job then finish(false) end
    clearTimers(); legacy.stop(); deleteEscape()
    if M.toggleHotkey then M.toggleHotkey:delete(); M.toggleHotkey=nil end
    M.enabled=false
end
function M.start(options)
    M.stop(); options=options or {}
    canToggle=options.canToggle
    M.enabled=options.enabled~=false
    M.toggleHotkey=assert(hs.hotkey.bind({'alt','ctrl','shift'},'m',M.toggle))
    return M
end
return M
