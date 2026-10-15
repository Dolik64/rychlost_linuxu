-- Call from Hammerspoon console. Uses the named scratch window and one Finder window.
-- Restores each original frame. No document contents are changed.
assert(linuxControls and not linuxControls.mover.busy)
local root=debug.getinfo(1,'S').source:sub(2):match('(.*/)') or './'
local layout=dofile(root..'snap-layout.lua')
local original=hs.window.focusedWindow()
local windows={}
for _,bundle in ipairs({'com.apple.TextEdit','com.apple.finder'}) do
    local app=hs.application.get(bundle)
    if app then
        for _,win in ipairs(app:allWindows()) do
            if win:isStandard() and not win:isFullScreen() and not win:isMinimized() and
               (bundle~='com.apple.TextEdit' or win:title()=='Codex-window-test-20261015.rtf') then
                windows[#windows+1]={win=win,frame=win:frame(),name=bundle}; break
            end
        end
    end
end
local cases={
    {'left',-1,0},{'up',-1,-1},{'down',-1,0},{'right',0,0},
    {'right',1,0},{'down',1,1},{'left',0,1},{'up',0,0},
    {'up',0,-1},{'right',1,-1},{'left',0,-1},{'down',0,0},
    {'down',0,1},{'left',-1,1},{'up',-1,0},{'right',0,0},
}
local p={done=false,passed=0,clamped=0,failures={}}
layoutProbe=p
local wi,ci=1,1
local function finish()
    for _,item in ipairs(windows) do item.win:setFrame(item.frame,0) end
    if original then original:focus() end
    p.done=true
    print('LIVE LAYOUT:',#p.failures==0 and 'PASS' or 'FAIL',p.passed,'checks',p.clamped,'app minimum-size cases',#windows,'apps')
    for _,v in ipairs(p.failures) do print(v) end
end
local function step()
    local item=windows[wi]
    if not item then finish(); return end
    local win=item.win
    if ci==1 then
        local a=win:screen():frame()
        win:setFrame({x=a.x+90,y=a.y+90,w=850,h=650},0)
    end
    win:focus()
    hs.timer.doAfter(0.5,function()
        local case=cases[ci]
        linuxControls.snap(case[1])
        hs.timer.doAfter(0.25,function()
            local expected=layout.frame({x=case[2],y=case[3]},win:screen():frame())
            local actual,area=win:frame(),win:screen():frame()
            local constrained=actual.w>=expected.w-3 and actual.h>=expected.h-3 and actual.w<=area.w+3 and actual.h<=area.h+3
            local edgeX=case[2]==1 and area.x+area.w-actual.w or area.x
            local edgeY=case[3]==1 and area.y+area.h-actual.h or area.y
            constrained=constrained and math.abs(actual.x-edgeX)<3 and math.abs(actual.y-edgeY)<3
            if layout.near(actual,expected) then p.passed=p.passed+1
            elseif constrained then p.passed=p.passed+1; p.clamped=p.clamped+1
            else p.failures[#p.failures+1]=item.name..' step '..ci..' '..case[1]..' actual '..tostring(win:frame()) end
            ci=ci+1
            if ci>#cases then win:setFrame(item.frame,0); ci=1; wi=wi+1 end
            step()
        end)
    end)
end
print('Live layout check started',#windows,'apps')
step()
