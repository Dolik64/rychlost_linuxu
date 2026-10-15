-- Move only the named scratch window, twice in each direction. Verify actual membership,
-- active Space, focus, and frame. Begin with scratch on Desktop 1. No user windows moved.
assert(linuxControls and not linuxControls.mover.busy)
local app=assert(hs.application.get('com.apple.TextEdit'))
local win
for _,w in ipairs(app:allWindows()) do if w:title()=='Codex-window-test-20261015.rtf' then win=w end end
assert(win,'Scratch window missing')
local screen=win:screen()
local source=(hs.spaces.windowSpaces(win) or {})[1]
local target
local users={}
for _,id in ipairs(hs.spaces.spacesForScreen(screen)) do if hs.spaces.spaceType(id)=='user' then users[#users+1]=id end end
for i,id in ipairs(users) do if id==source then target=users[i+1] or users[i-1] end end
assert(target,'A second desktop is required')
local frame=win:frame()
local original=hs.window.focusedWindow()
local steps={target,source,target,source}
local p={done=false,passed=0,results={}}
spacesProbe=p
local function finish(err)
    p.done=true; p.error=err
    print('LIVE SPACES:',err and 'FAIL' or 'PASS',p.passed,'moves',err or '')
    if original and not err then original:focus() end
end
local function nextMove(i)
    if not steps[i] then finish(); return end
    win:focus()
    hs.timer.doAfter(0.65,function()
        local accepted=linuxControls.mover.move(win,steps[i])
        if not accepted then finish('move refused'); return end
        local started=hs.timer.secondsSinceEpoch()
        local poll
        poll=hs.timer.doEvery(0.25,function()
            if linuxControls.mover.busy and hs.timer.secondsSinceEpoch()-started<15 then return end
            poll:stop()
            local result=linuxControls.mover.lastResult
            local current=hs.spaces.windowSpaces(win)
            local actual=hs.window.focusedWindow()
            local ok=result and result.ok and #current==1 and current[1]==steps[i]
                and hs.spaces.activeSpaceOnScreen(screen)==steps[i]
                and actual and actual:id()==win:id()
            for _,k in ipairs({'x','y','w','h'}) do if math.abs(win:frame()[k]-frame[k])>3 then ok=false end end
            p.results[i]={target=steps[i],actual=current,ok=ok,result=result}
            print('SPACE MOVE',i,ok and 'PASS' or 'FAIL',hs.inspect(p.results[i]))
            if not ok then finish('move '..i); return end
            p.passed=p.passed+1
            hs.timer.doAfter(0.6,function() nextMove(i+1) end)
        end)
    end)
end
nextMove(1)
