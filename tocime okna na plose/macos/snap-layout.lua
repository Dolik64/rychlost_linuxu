-- Geometry-only floating-window transitions: halves, corners, opposite expands.
local M = {}
local directions = {left={'x',-1},right={'x',1},up={'y',-1},down={'y',1}}
function M.next(state, direction)
    local d=assert(directions[direction], 'invalid direction')
    local nextState={x=state and state.x or 0, y=state and state.y or 0}
    if nextState[d[1]]==d[2] then return nextState, true end
    nextState[d[1]]=nextState[d[1]]==-d[2] and 0 or d[2]
    return nextState, false
end
function M.frame(state, area)
    local w=state.x==0 and area.w or math.floor(area.w/2)
    local h=state.y==0 and area.h or math.floor(area.h/2)
    return {x=area.x+(state.x==1 and area.w-w or 0),
            y=area.y+(state.y==1 and area.h-h or 0),w=w,h=h}
end
function M.near(a,b)
    for _,k in ipairs({'x','y','w','h'}) do if math.abs(a[k]-b[k])>3 then return false end end
    return true
end
function M.infer(frame,area)
    for x=-1,1 do for y=-1,1 do
        local state={x=x,y=y}
        if M.near(frame,M.frame(state,area)) then return state end
    end end
end
return M
