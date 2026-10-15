assert(linuxControls and not linuxControls.mover.busy)
local screen=hs.screen.mainScreen()
local source=hs.spaces.activeSpaceOnScreen(screen)
local users,index={},nil
for _,id in ipairs(hs.spaces.spacesForScreen(screen)) do
    if hs.spaces.spaceType(id)=='user' then users[#users+1]=id; if id==source then index=#users end end
end
assert(index and #users>1)
local step=index<#users and 1 or -1
local target=users[index+step]
local original=hs.window.focusedWindow()
local membership=original and hs.spaces.windowSpaces(original)
local results={}
local function verify(label,expected)
    local actual=hs.spaces.activeSpaceOnScreen(screen)
    results[#results+1]=actual==expected
    print('NAVIGATION',label,actual==expected and 'PASS' or 'FAIL',expected,actual)
end
hs.timer.doAfter(2,function()
    if index==1 or index==#users then
        linuxControls.space(index==1 and -1 or 1,false)
        verify('boundary',source)
    end
    linuxControls.space(step,false)
    hs.timer.doAfter(1.3,function()
        verify('out',target)
        linuxControls.space(-step,false)
        hs.timer.doAfter(1.3,function()
            verify('back',source)
            if original then
                local now=hs.spaces.windowSpaces(original)
                local same=hs.inspect(now)==hs.inspect(membership)
                results[#results+1]=same
                print('NAVIGATION original window membership',same and 'PASS' or 'FAIL')
                original:focus()
            end
            local ok=true; for _,v in ipairs(results) do ok=ok and v end
            print('LIVE NAVIGATION',ok and 'PASS' or 'FAIL')
        end)
    end)
end)
