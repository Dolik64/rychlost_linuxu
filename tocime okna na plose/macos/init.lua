-- Nekonečné přepínání oken: Control + Option + šipka vlevo/vpravo.
if windowCycle then windowCycle.stop() end
windowCycle = dofile("/Users/jirka/Documents/rychlost_linuxu/tocime okna na plose/macos/window-cycle.lua").start()
