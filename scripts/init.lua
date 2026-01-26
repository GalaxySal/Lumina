-- Lumina init.lua Example
-- This file is executed when Lumina starts.
-- You can use the 'lumina' global object to interact with the browser.

-- Print a welcome message to the debug console
print("Lumina: init.lua loaded successfully!")
print("Lumina Version: " .. lumina.version)
print("Platform: " .. lumina.platform)

-- Example: Define a global helper function that can be called from other scripts
function greet_user(name)
    return "Hello, " .. name .. "! Welcome to Safkan Yapı."
end

-- Example: Simple math calculation
local x = 10
local y = 20
print("Math Check: " .. x .. " + " .. y .. " = " .. (x + y))
