local root=debug.getinfo(1,'S').source:sub(2):match('(.*/)') or './'
local layout=dofile(root..'snap-layout.lua')
local M={}
local hotkeys, states={}, {}
local cycle, mover, destroyFilter, navigationTimer
local navigationBusy=false
local function busy() return navigationBusy or (mover and mover.busy) end
local function bind(mods,key,fn,repeats)
    local h=hs.hotkey.bind(mods,key,fn,nil,repeats and fn or nil)
    assert(h,'Cannot register '..table.concat(mods,'+')..'+'..key)
    hotkeys[#hotkeys+1]=h
end
local function ordinaryWindow()
    local win=hs.window.focusedWindow()
    if not win or not win:isStandard() or win:isFullScreen() then
        hs.alert.show('Vyber běžné okno mimo celou obrazovku.'); return
    end
    return win
end
function M.snap(direction)
    if busy() then return end
    local win=ordinaryWindow(); if not win then return end
    local id,screen,frame=win:id(),win:screen(),win:frame()
    local area=screen:frame()
    local saved=states[id]
    local state
    if saved and saved.screen==screen:id() and layout.near(saved.frame,frame) and layout.near(saved.area,area) then
        state=saved.state
    else state=layout.infer(frame,area) end
    local nextState,atEdge=layout.next(state,direction)
    -- Workspace movement is deliberately explicit via Ctrl+Option+Shift.
    if atEdge then return end
    win:setFrame(layout.frame(nextState,area),0)
    -- Apps may impose a width/height larger than a half. Keep their outer edge
    -- on screen after reading the actual size (Finder is one such app).
    local actual=win:frame()
    local x=nextState.x==1 and area.x+area.w-actual.w or area.x
    local y=nextState.y==1 and area.y+area.h-actual.h or area.y
    win:setTopLeft({x=math.max(area.x,x),y=math.max(area.y,y)})
    states[id]={state=nextState,screen=screen:id(),frame=win:frame(),area=area}
    return states[id].frame
end
local function targetSpace(screen,step)
    local current=hs.spaces.activeSpaceOnScreen(screen)
    local users={}
    for _,id in ipairs(hs.spaces.spacesForScreen(screen) or {}) do
        if hs.spaces.spaceType(id)=='user' then users[#users+1]=id end
    end
    for i,id in ipairs(users) do
        if id==current then return users[i+step] end
    end
end
function M.space(step,move)
    if busy() then return end
    local win=move and ordinaryWindow() or hs.window.focusedWindow()
    if move and not win then return end
    local screen=win and win:screen() or hs.screen.mainScreen()
    local target=targetSpace(screen,step)
    if not target then hs.alert.show('V tomto směru není další plocha.',0.7); return end
    if move then
        if mover then return mover.move(win,target) end
        hs.alert.show('Přesun oken nyní obsluhuje PopSpace.'); return
    end
    navigationBusy=true
    local ok,err=hs.spaces.gotoSpace(target)
    navigationTimer=hs.timer.doAfter(0.8,function() navigationBusy=false; navigationTimer=nil end)
    return ok,err
end
function M.stop()
    for _,h in ipairs(hotkeys) do h:delete() end
    hotkeys={}
    if navigationTimer then navigationTimer:stop(); navigationTimer=nil end
    navigationBusy=false
    if cycle then cycle.stop() end
    if mover then mover.stop() end
    if destroyFilter then destroyFilter:unsubscribeAll():pause(); destroyFilter=nil end
    states={}
end
function M.start(options)
    M.stop()
    options=options or {}
    if options.nativeSpaces then mover=nil
    else mover=dofile(root..'fast-space-mover.lua').start({canToggle=function() return not navigationBusy end}) end
    M.mover=mover
    cycle=dofile(root..'window-cycle.lua').start({modifiers={'alt'},enabledWhen=function() return not busy() end}); M.cycle=cycle
    local function previous() if not busy() then return cycle.previous() end end
    local function nextWindow() if not busy() then return cycle.next() end end
    bind({'alt'},'up',previous,true)
    bind({'alt'},'down',nextWindow,true)
    for _,direction in ipairs({'left','right','up','down'}) do
        local step=(direction=='left' or direction=='up') and -1 or 1
        bind({'alt','shift'},direction,function() M.snap(direction) end,false)
        bind({'alt','ctrl'},direction,function() M.space(step,false) end,false)
        if not options.nativeSpaces then
            bind({'alt','ctrl','shift'},direction,function() M.space(step,true) end,false)
        end
    end
    bind({'alt'},'w',function() if not busy() then hs.spaces.toggleMissionControl() end end,false)
    destroyFilter=hs.window.filter.new()
    destroyFilter:subscribe(hs.window.filter.windowDestroyed,function(win) local id=win:id(); if id then states[id]=nil end end)
    return M
end
return M
