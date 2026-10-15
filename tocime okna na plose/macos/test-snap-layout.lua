local root=debug.getinfo(1,'S').source:sub(2):match('(.*/)') or './'
local m=dofile(root..'snap-layout.lua')
local function check(s,d,x,y,edge)
    local n,e=m.next(s,d)
    assert(n.x==x and n.y==y and e==edge,d)
    return n
end
local s=check(nil,'left',-1,0,false)
s=check(s,'up',-1,-1,false)
s=check(s,'down',-1,0,false)
s=check(s,'right',0,0,false)
s=check(s,'down',0,1,false)
s=check(s,'right',1,1,false)
check(s,'right',1,1,true)
check(s,'up',1,0,false)
check(s,'left',0,1,false)
local count=9
for x=-1,1 do for y=-1,1 do
    for _,d in ipairs({'up','down','left','right'}) do
        local n=m.next({x=x,y=y},d)
        assert(n.x>=-1 and n.x<=1 and n.y>=-1 and n.y<=1)
        local a={x=-1920,y=25,w=1513,h=957}
        local f=m.frame(n,a)
        assert(f.x>=a.x and f.y>=a.y and f.x+f.w<=a.x+a.w and f.y+f.h<=a.y+a.h)
        local inferred=m.infer(f,a)
        assert(inferred.x==n.x and inferred.y==n.y)
        count=count+1
    end
end end
assert(not m.infer({x=100,y=100,w=500,h=400},{x=0,y=25,w=1512,h=900}))
print('PASS: '..count..' snap transitions; odd dimensions, negative monitor coordinates, manual resize')
