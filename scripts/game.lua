-- game.lua — example script for Ralk engine (Fase 26)
-- Spawns physics cubes at random positions every 2 seconds.
-- Hot-reloads when you save this file.

engine.log("game.lua loaded!")

local spawn_count = 0

engine.every(2.0, function()
    local x = math.random(-4, 4)
    local z = math.random(-4, 4)
    engine.spawn({ position = { x, 8, z } })
    spawn_count = spawn_count + 1
    engine.log("Spawned cube #" .. spawn_count .. " at (" .. x .. ", 8, " .. z .. ")")
end)

engine.every(10.0, function()
    engine.log("Script alive — total cubes spawned: " .. spawn_count)
end)
