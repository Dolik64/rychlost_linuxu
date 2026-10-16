-- Pop!_OS-style controls: Option replaces Super.
if linuxControls then linuxControls.stop() end
if windowCycle then windowCycle.stop() end
linuxControls = dofile('/Users/jirka/Documents/rychlost_linuxu/tocime okna na plose/macos/linux-controls.lua').start()
windowCycle = linuxControls.cycle

-- Pop!_OS-style controls: Option replaces Super.
if fastMover then fastMover.stop() end
if linuxControls then linuxControls.stop() end
if windowCycle then windowCycle.stop() end

local baseDir = '/Users/jirka/Documents/rychlost_linuxu/tocime okna na plose/macos/'

linuxControls = dofile(baseDir .. 'linux-controls.lua').start()
windowCycle = linuxControls.cycle

-- Načte rychlý přesouvač a automaticky přepíše linuxControls.mover (přepínání přes Option+Ctrl+Shift+M)
fastMover = dofile(baseDir .. 'fast-space-mover.lua').start()