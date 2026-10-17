-- Regression: the native mover and Hammerspoon must never own the same keys.
local realDofile=dofile
local active,legacyStarts={},0
local function bind(mods,key)
    local h={signature=table.concat(mods,'+')..'+'..key}
    active[h]=true
    function h:delete() active[self]=nil end
    return h
end
hs={hotkey={bind=bind},window={filter={windowDestroyed='destroyed',new=function()
    return {subscribe=function() end,unsubscribeAll=function(self) return self end,pause=function() end}
end}}}
_G.dofile=function(path)
    if path:match('window%-cycle.lua$') then
        local keys={}
        return {start=function(self)
            keys={bind({'alt'},'left'),bind({'alt'},'right')}
            return {stop=function() for _,h in ipairs(keys) do h:delete() end end}
        end}
    elseif path:match('fast%-space%-mover.lua$') then
        return {start=function()
            legacyStarts=legacyStarts+1
            local h=bind({'alt','ctrl','shift'},'m')
            return {stop=function() h:delete() end}
        end}
    end
    return realDofile(path)
end
local profile=realDofile('linux-controls.lua')
local function count() local n=0 for _ in pairs(active) do n=n+1 end return n end
profile.start({nativeSpaces=true})
assert(count()==13 and not profile.mover and legacyStarts==0)
for h in pairs(active) do assert(not h.signature:find('alt+ctrl+shift',1,true)) end
profile.start()
assert(count()==18 and profile.mover and legacyStarts==1)
profile.start({nativeSpaces=true})
assert(count()==13 and not profile.mover and legacyStarts==1)
for h in pairs(active) do assert(not h.signature:find('alt+ctrl+shift',1,true)) end
profile.stop()
assert(count()==0)
_G.dofile=realDofile
print('PASS native handoff: 13 retained keys, no legacy mover, restart releases collisions')
