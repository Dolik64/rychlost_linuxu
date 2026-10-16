-- Move ordinary windows through Mission Control on macOS 26.
-- Private moveWindowToSpace() can claim success without moving another app's window.
local M = {busy=false}
local events, ax, mouseAPI = hs.eventtap.event, hs.axuielement, hs.mouse
local timers, escape, pointer, mouseDown, lastPoint = {}, nil, nil, false, nil
local started, phases
local function now() return hs.timer.absoluteTime()/1e9 end
local function mark(name) phases[name]=now()-started end
local function mouse(kind, point)
    lastPoint=point
    events.newMouseEvent(events.types[kind],point):post()
end
local function child(parent, identifier)
    for _,e in ipairs(parent and parent.AXChildren or {}) do
        if e.AXIdentifier==identifier then return e end
    end
end
local function missionControl()
    local dock=hs.application.get('com.apple.dock')
    return dock and child(ax.applicationElement(dock),'mc')
end
local function displayGroup(screen)
    local mc=missionControl()
    for _,e in ipairs(mc and mc.AXChildren or {}) do
        if e.AXIdentifier=='mc.display' and e.AXDisplayID==screen:id() then return e end
    end
end
local function finish(ok,message)
    for _,t in ipairs(timers) do t:stop() end
    timers={}
    if mouseDown then mouse('leftMouseUp',lastPoint); mouseDown=false end
    if escape then escape:delete(); escape=nil end
    if not ok then hs.spaces.closeMissionControl() end
    if pointer then mouseAPI.absolutePosition(pointer); pointer=nil end
    M.busy=false
    M.lastResult={ok=ok,message=message,elapsed=now()-started,phases=phases,readiness=M.readiness}
    hs.printf('Space move: %s, %.2f s',ok and 'PASS' or 'FAIL',M.lastResult.elapsed)
    if not ok and message then hs.alert.show(message) end
end
local function later(seconds,fn)
    timers[#timers+1]=hs.timer.doAfter(seconds,function()
        local ok,err=xpcall(fn,debug.traceback)
        if not ok then hs.printf('Space move: %s',err); finish(false,'Přesun okna selhal.') end
    end)
end
local function waitUntil(test,success,timeout,message)
    local deadline=now()+timeout
    local function check()
        local result=test()
        if result then success(result)
        elseif now()>=deadline then finish(false,message)
        else later(0.05,check) end
    end
    later(0.05,check)
end
local function center(f) return {x=f.x+f.w/2,y=f.y+f.h/2} end
local function near(a,b)
    if not a or not b then return false end
    for _,k in ipairs({'x','y','w','h'}) do if math.abs(a[k]-b[k])>1 then return false end end
    return true
end
local function onSpace(win,id)
    for _,v in ipairs(hs.spaces.windowSpaces(win) or {}) do if v==id then return true end end
    return false
end
function M.stop() if M.busy then finish(false) end end
function M.move(win,target)
    if M.busy then return false,'busy' end
    if not win or not win:isStandard() or win:isFullScreen() then
        hs.alert.show('Vyber běžné okno mimo celou obrazovku.'); return false
    end
    local screen=win:screen()
    local spaces=hs.spaces.windowSpaces(win) or {}
    if #spaces~=1 or hs.spaces.spaceType(spaces[1])~='user' then
        hs.alert.show('Okno musí patřit jedné běžné ploše.'); return false
    end
    if hs.spaces.spaceType(target)~='user' or hs.spaces.spaceDisplay(target)~=screen:getUUID() then return false end
    if onSpace(win,target) then return true end
    local index
    for i,id in ipairs(hs.spaces.spacesForScreen(screen)) do if id==target then index=i end end
    if not index then return false end
    local title=win:title()
    if not title or title=='' then title=win:application():title() end
    M.busy=true; M.lastResult=nil
    started=now(); phases={}; M.readiness={}
    pointer=mouseAPI.absolutePosition()
    escape=hs.hotkey.bind({},'escape',function() finish(false,'Přesun zrušen.') end)
    later(12,function() finish(false,'Přesun nedokončen. Zkontroluj polohu okna.') end)
    local function begin()
        local mods=hs.eventtap.checkKeyboardModifiers()
        if mods.alt or mods.ctrl or mods.shift or mods.cmd then later(0.05,begin); return end
        mark('keysReleased')
        local focused=hs.window.focusedWindow()
        if not focused or focused:id()~=win:id() then win:focus() end
        hs.spaces.openMissionControl()
        mark('opened')
        local f=screen:fullFrame()
        local reduceMotion=hs.screen.accessibilitySettings().ReduceMotion
        local minimumOpeningTime=reduceMotion and 0.15 or 0.75
        local hoverCount=0
        local priorWindow,priorSpace,stableSince
        waitUntil(function()
            local display=displayGroup(screen)
            M.readiness.display=display~=nil
            if not display then return end
            local list=child(child(display,'mc.spaces'),'mc.spaces.list')
            local button=list and list.AXChildren and list.AXChildren[index]
            local bf=button and button.AXFrame
            -- The label becomes a valid drop target once the Spaces strip is expanded.
            M.readiness.space=bf
            if not bf or bf.y<f.y then
                hoverCount=hoverCount+1
                mouse('mouseMoved',{x=f.x+f.w/2+hoverCount%2,y=f.y+10})
                return
            end
            local windows=child(display,'mc.windows')
            local matches={}
            for _,e in ipairs(windows and windows.AXChildren or {}) do
                if e.AXTitle==title then matches[#matches+1]=e end
            end
            M.readiness.matches=#matches
            if #matches>1 then return {ambiguous=true} end
            if #matches==0 then return end
            local wf=matches[1].AXFrame
            if near(wf,priorWindow) and near(bf,priorSpace) then
                if stableSince and now()-stableSince>=0.1 and now()-started-phases.opened>=minimumOpeningTime then return {window=wf,space=bf,button=button} end
            else stableSince=now() end
            priorWindow,priorSpace=wf,bf
        end,function(found)
            if found.ambiguous then finish(false,'Více oken má stejný název; přesun je nejednoznačný.'); return end
            mark('ready')
            local from,to=center(found.window),center(found.space)
            mouse('mouseMoved',from)
            mouse('leftMouseDown',from); mouseDown=true
            for n=1,15 do later(n*0.02,function()
                mouse('leftMouseDragged',{x=from.x+(to.x-from.x)*n/15,y=from.y+(to.y-from.y)*n/15})
            end) end
            later(0.35,function()
                mouse('leftMouseUp',to); mouseDown=false; mark('released')
                waitUntil(function() return onSpace(win,target) end,function()
                    mark('moved')
                    -- Reacquire the destination after the drop changes the AX tree.
                    -- gotoSpace also lets Mission Control finish its drop animation.
                    local originalWait=hs.spaces.MCwaitTime
                    if reduceMotion then hs.spaces.MCwaitTime=0.15 end
                    local called,switched,err=pcall(hs.spaces.gotoSpace,target)
                    hs.spaces.MCwaitTime=originalWait
                    if not called then finish(false,'Přechod na plochu selhal.'); return end
                    if not switched then finish(false,err or 'Přechod na plochu se nezdařil.'); return end
                    waitUntil(function()
                        return hs.spaces.activeSpaceOnScreen(screen)==target and not missionControl()
                    end,function()
                        mark('arrived')
                        local active=hs.window.focusedWindow()
                        if not active or active:id()~=win:id() then win:focus() end
                        waitUntil(function()
                            local activeWindow=hs.window.focusedWindow()
                            return onSpace(win,target) and activeWindow and activeWindow:id()==win:id()
                        end,function() finish(true,'moved') end,1.5,'Okno přesunuto, aktivace se nezdařila.')
                    end,2,'Okno přesunuto, přechod na plochu se nezdařil.')
                end,1.5,'macOS přesun okna neprovedl.')
            end)
        end,3,'Okno nebo cílovou plochu nelze jednoznačně najít v Mission Control.')
    end
    later(0.01,begin)
    return true
end
return M
