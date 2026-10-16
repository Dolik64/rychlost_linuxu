-- Returns a runner for an explicitly selected, disposable Codex test window.
-- Four moves: normal frame out/back, maximized (not fullscreen) out/back.
return function(win)
    assert(linuxControls and not linuxControls.mover.busy)
    assert(win and win:title():match('^Codex[-]'), 'Select a Codex scratch window explicitly')
    assert(win:isStandard() and not win:isFullScreen())
    local originalFrame,screen=win:frame(),win:screen()
    local source=(hs.spaces.windowSpaces(win) or {})[1]
    local target
    local users={}
    for _,id in ipairs(hs.spaces.spacesForScreen(screen)) do
        if hs.spaces.spaceType(id)=='user' then users[#users+1]=id end
    end
    for i,id in ipairs(users) do if id==source then target=users[i+1] or users[i-1] end end
    assert(target,'A second desktop is required')
    local p={done=false,passed=0,results={},window=win:id(),source=source}
    moverWindowProbe=p
    local function finish(err)
        win:setFrame(originalFrame,0)
        p.done=true; p.error=err
        print('LIVE MOVER WINDOW',err and 'FAIL' or 'PASS',p.passed,'moves',err or '',hs.inspect(p.results))
    end
    local function nextMove(i)
        if i>4 then finish(); return end
        win:focus()
        if i==3 then win:setFrame(screen:frame(),0) end
        local expected=win:frame()
        local destination=i%2==1 and target or source
        hs.timer.doAfter(0.8,function()
            if not linuxControls.mover.move(win,destination) then finish('move refused'); return end
            local started=hs.timer.secondsSinceEpoch()
            local poll
            poll=hs.timer.doEvery(0.15,function()
                if linuxControls.mover.busy and hs.timer.secondsSinceEpoch()-started<15 then return end
                poll:stop()
                local result=linuxControls.mover.lastResult
                local current=hs.spaces.windowSpaces(win) or {}
                local focused=hs.window.focusedWindow()
                local ok=result and result.ok and #current==1 and current[1]==destination
                    and hs.spaces.activeSpaceOnScreen(screen)==destination
                    and focused and focused:id()==win:id()
                for _,k in ipairs({'x','y','w','h'}) do
                    if math.abs(win:frame()[k]-expected[k])>3 then ok=false end
                end
                p.results[i]={ok=ok,elapsed=result and result.elapsed,method=result and result.method,
                    fallback=result and result.fallbackReason,target=destination,actual=current,
                    frame=win:frame(),expected=expected}
                if not ok then finish('move '..i); return end
                p.passed=p.passed+1
                hs.timer.doAfter(0.5,function() nextMove(i+1) end)
            end)
        end)
    end
    hs.timer.doAfter(2,function() nextMove(1) end)
end
