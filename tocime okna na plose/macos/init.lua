-- Pop!_OS-style controls: Option replaces Super.
if fastMover then fastMover.stop() end
if linuxControls then linuxControls.stop() end
if windowCycle then windowCycle.stop() end

local baseDir = '/Users/jirka/Documents/rychlost_linuxu/tocime okna na plose/macos/'
linuxControls = dofile(baseDir .. 'linux-controls.lua').start({nativeSpaces=true})
windowCycle = linuxControls.cycle
-- PopSpace owns Option+Control+Shift+arrows; no legacy mover or mode toggle.
fastMover = linuxControls.mover
-- Release an in-progress drag before Hammerspoon reloads or exits.
hs.shutdownCallback = function() linuxControls.stop() end
