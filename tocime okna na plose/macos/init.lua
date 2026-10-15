-- Pop!_OS-style controls: Option replaces Super.
if linuxControls then linuxControls.stop() end
if windowCycle then windowCycle.stop() end
linuxControls = dofile('/Users/jirka/Documents/rychlost_linuxu/tocime okna na plose/macos/linux-controls.lua').start()
windowCycle = linuxControls.cycle
