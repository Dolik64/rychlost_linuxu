-- Behavioral tests for ownership, cancellation, verification and fallback.
-- No real keyboard, pointer, application or Spaces APIs are called.
local realDofile=dofile
local root=debug.getinfo(1,'S').source:sub(2):match('(.*/)') or './'
local passed=0
local function scenario(name,body)
    local t,queue,hooks=0,{},{}
    local env={space=1,active=1,pointer={x=400,y=300},mouseDown=false,legacyCalls=0}
    local function copy(v) local r={} for k,x in pairs(v) do r[k]=x end return r end
    local screen={getUUID=function() return 'screen' end}
    local frame={x=100,y=100,w=600,h=500}
    local win={id=function() return 10 end,isStandard=function() return true end,
        isFullScreen=function() return false end,screen=function() return screen end,
        frame=function() return copy(frame) end,setFrame=function(_,f) frame=copy(f) end}
    win.focus=function() if not env.noFocus then env.focus=win; env.active=env.space end end
    env.focus=win
    local windowElement={AXRole='AXWindow'}
    local function after(seconds,fn)
        local timer={due=t+seconds,fn=fn,stopped=false,stop=function(self) self.stopped=true end}
        queue[#queue+1]=timer; return timer
    end
    local legacy={busy=false}
    legacy.stop=function() legacy.busy=false end
    legacy.move=function(w,target)
        env.legacyCalls=env.legacyCalls+1; legacy.busy=true
        after(0.1,function()
            if not legacy.busy then return end
            env.space=target; env.active=target; env.focus=w
            legacy.lastResult={ok=true}; legacy.busy=false
        end)
        return true
    end
    _G.hs={
        timer={absoluteTime=function() return t*1e9 end,doAfter=after},
        spaces={MCwaitTime=0.3,windowSpaces=function() return {env.space} end,
            spacesForScreen=function() return env.intermediate and {1,3,2} or {1,2} end,
            spaceType=function() return 'user' end,spaceDisplay=function() return 'screen' end,
            activeSpaceOnScreen=function() return env.active end,
            moveWindowToSpace=function(_,target) if env.direct then env.space=target end return true end,
            gotoSpace=function(target)
                if env.gotoThrows then error('simulated goto error') end
                if env.noSwitch then return false end
                env.active=target
                if env.mouseDown and env.dragWorks then env.space=target end
                return true
            end,closeMissionControl=function() end},
        eventtap={checkKeyboardModifiers=function() return env.modifiers or {} end,
            event={types={mouseMoved='move',leftMouseDown='down',leftMouseDragged='drag',leftMouseUp='up'},
                newKeyEvent=function(_,direction,down)
                    return {post=function()
                        if env.keyThrows then error('simulated key error') end
                        if down and not env.noSwitch then
                            local target=direction=='right' and 2 or 1
                            env.active=target
                            if env.mouseDown and env.dragWorks then env.space=target end
                        end
                    end}
                end,
                newMouseEvent=function(kind,point)
                    return {setFlags=function(self) return self end,post=function()
                        if kind=='drag' and env.dragThrows then error('simulated drag error') end
                        env.pointer=copy(point)
                        if kind=='down' then env.mouseDown=true; env.downPoint=copy(point); env.downFrame=copy(frame) end
                        if kind=='drag' and env.mouseDown and not env.ungrabbable then
                            frame.x=env.downFrame.x+point.x-env.downPoint.x; frame.y=env.downFrame.y+point.y-env.downPoint.y
                        end
                        if kind=='up' then env.mouseDown=false; env.releases=(env.releases or 0)+1 end
                    end}
                end}},
        mouse={absolutePosition=function(point) if point then env.pointer=copy(point) end return copy(env.pointer) end},
        hotkey={bind=function(_,key,fn)
            hooks[key]=fn
            return {delete=function() hooks[key]=nil end}
        end},
        screen={accessibilitySettings=function() return {ReduceMotion=true} end},
        window={focusedWindow=function() return env.focus end},
        application={get=function() return nil end},
        axuielement={windowElement=function() return windowElement end,
            systemElementAtPosition=function()
                return {AXRole=env.interactive and 'AXButton' or 'AXStaticText',AXWindow=windowElement,AXParent=windowElement}
            end},printf=function() end,alert={show=function() end}}
    _G.dofile=function(path)
        if path==root..'space-mover.lua' then return legacy end
        return realDofile(path)
    end
    local mover=realDofile(root..'fast-space-mover.lua').start()
    local function run(untilTime)
        untilTime=untilTime or 20
        for _=1,2000 do
            local index,nextTimer
            for i,timer in ipairs(queue) do
                if not timer.stopped and timer.due<=untilTime and (not nextTimer or timer.due<nextTimer.due) then index,nextTimer=i,timer end
            end
            if not nextTimer then t=untilTime; return end
            table.remove(queue,index); t=nextTimer.due; nextTimer.fn()
        end
        error('timer loop did not terminate')
    end
    local function clean()
        assert(not mover.busy,'busy leaked')
        assert(not env.mouseDown,'mouse button leaked')
        assert(not hooks.escape,'Escape hotkey leaked')
        assert(env.pointer.x==400 and env.pointer.y==300,'pointer not restored')
    end
    body(mover,env,run,clean,hooks,win)
    mover.stop(); _G.dofile=realDofile
    passed=passed+1; print('PASS '..name)
end
scenario('Mission Control mode skips the fast attempt',function(m,e,run,clean,_,w)
    assert(not m.enabled); assert(m.move(w,2)); run(); clean()
    assert(e.legacyCalls==1 and m.lastResult.ok and e.space==2)
end)
scenario('false API success and failed titlebar drag fall back, including total time',function(m,e,run,clean,_,w)
    m.enable(); m.move(w,2); run(); clean()
    assert(m.lastResult.ok and e.legacyCalls==1 and m.lastResult.fallbackReason)
    assert(m.lastResult.elapsed>0.3 and m.lastResult.method=='mission-control')
end)
scenario('actual titlebar transfer is verified without fallback',function(m,e,run,clean,_,w)
    e.dragWorks=true; m.enable(); m.move(w,2); run(); clean()
    assert(m.lastResult.ok and e.legacyCalls==0 and m.lastResult.method=='titlebar')
end)
scenario('direct move verifies focus and membership',function(m,e,run,clean,_,w)
    e.direct=true; m.enable(); m.move(w,2); run(); clean()
    assert(m.lastResult.ok and e.active==2 and e.legacyCalls==0)
end)
scenario('toggle and second move refused during drag; stop releases mouse',function(m,e,run,clean,_,w)
    m.enable(); m.move(w,2); run(0.08); assert(e.mouseDown)
    assert(m.toggle()==false and m.enabled); assert(m.move(w,2)==false)
    m.stop(); run(); clean(); assert(not m.lastResult.ok and e.legacyCalls==0)
end)
scenario('Escape releases drag and cancels all callbacks',function(m,e,run,clean,h,w)
    m.enable(); m.move(w,2); run(0.08); assert(e.mouseDown)
    h.escape(); run(); clean(); assert(not m.lastResult.ok and e.legacyCalls==0)
end)
scenario('exception during drag releases button',function(m,e,run,clean,_,w)
    e.dragThrows=true; m.enable(); m.move(w,2); run(); clean(); assert(not m.lastResult.ok)
end)
scenario('goto exception restores global MC delay and pointer',function(m,e,run,clean,_,w)
    e.gotoThrows=true; e.direct=true; m.enable(); m.move(w,2); run(); clean()
    assert(not m.lastResult.ok and hs.spaces.MCwaitTime==0.3)
end)
scenario('held modifiers delay all movement',function(m,e,run,clean,_,w)
    e.modifiers={alt=true}; m.enable(); m.move(w,2); run(1)
    assert(m.busy and not e.mouseDown and e.legacyCalls==0)
    e.modifiers={}; run(); clean(); assert(m.lastResult.ok)
end)
scenario('watchdog releases busy when modifiers never released',function(m,e,run,clean,_,w)
    e.modifiers={ctrl=true}; m.enable(); m.move(w,2); run(); clean(); assert(not m.lastResult.ok)
end)
scenario('failure to focus does not click another application',function(m,e,run,clean,_,w)
    e.noFocus=true; e.focus=nil; m.enable(); m.move(w,2); run(); clean()
    assert(not m.lastResult.ok and not e.releases and e.legacyCalls==0)
end)
scenario('restart removes old mode and leaves one toggle',function(m,e,run,clean,h)
    m.enable(); m.start({enabled=false}); assert(not m.enabled and h.m); m.stop(); assert(not h.m); clean()
end)
scenario('intermediate fullscreen is not traversed with a held window',function(m,e,run,clean,_,w)
    e.intermediate=true; m.enable(); m.move(w,2); run(); clean()
    assert(m.lastResult.ok and e.legacyCalls==1 and not e.releases)
end)
scenario('stop also cancels an in-progress fallback',function(m,e,run,clean,_,w)
    m.disable(); m.move(w,2); run(0.06); assert(e.legacyCalls==1); m.stop(); run(); clean()
    assert(not m.lastResult.ok and e.space==1)
end)
scenario('interactive titlebar control is never clicked',function(m,e,run,clean,_,w)
    e.interactive=true; m.enable(); m.move(w,2); run(); clean()
    assert(m.lastResult.ok and e.legacyCalls==1 and not e.releases)
end)
scenario('non-draggable titlebar falls back without switching first',function(m,e,run,clean,_,w)
    e.ungrabbable=true; m.enable(); m.move(w,2); run(); clean()
    assert(m.lastResult.ok and e.legacyCalls==1 and m.lastResult.fallbackReason)
end)
print(passed..' mover lifecycle tests passed')
