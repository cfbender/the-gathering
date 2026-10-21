-- Runs in every deck chooser sandbox before the member's script, under the same limits.
--
-- The limits are checked by an instruction hook, which only runs between Lua instructions,
-- not inside the C code of a library function or in a finalizer. This replaces every library
-- function that can loop in C for longer than its arguments' memory (or that can run Lua
-- code with hooks off) so a script cannot outlast its limits, and keeps `pcall` from
-- swallowing the error that ends a script.
local guard, find = ...

local G, string, table = _G, string, table
local error, pcall, rawget, select, setmetatable, type, xpcall =
  error, pcall, rawget, select, setmetatable, type, xpcall
local rep, tointeger, maxinteger = string.rep, math.tointeger, math.maxinteger

-- `guard` passes the results through, or raises the limit error again once a limit tripped.
G.pcall = function(...)
  return guard(pcall(...))
end
G.xpcall = function(...)
  return guard(xpcall(...))
end

-- Finalizers run with hooks disabled. Lua only finalizes objects whose metatable had a
-- `__gc` field when it was set, so refusing those leaves nothing to finalize.
G.setmetatable = function(t, mt)
  if type(mt) == "table" and rawget(mt, "__gc") ~= nil then
    error("__gc metamethods are not allowed", 2)
  end
  return setmetatable(t, mt)
end

-- Lua patterns backtrack in C, exponentially on some short inputs, so only plain-text search
-- (`find`, implemented in Rust) remains.
string.find, string.match, string.gmatch, string.gsub = find, nil, nil, nil

-- Repeating an empty string loops once per repetition without allocating anything.
string.rep = function(s, n, sep)
  if s == "" and (sep == nil or sep == "") and tointeger(n) then
    return ""
  end
  return rep(s, n, sep)
end

-- The C versions of these shift elements for as many positions as `#t` (which `__len` can
-- make anything) or the arguments ask for; as Lua, every step counts against the limits.
-- Raises an argument error `level` frames up from the caller, like `error`.
local function bad(level, arg, name, message)
  error("bad argument #" .. arg .. " to '" .. name .. "' (" .. message .. ")", level + 1)
end

local function integer(value, arg, name)
  local n = tointeger(value)
  if n == nil then
    bad(3, arg, name, "number has no integer representation")
  end
  return n
end

local function check_table(t, arg, name)
  if type(t) ~= "table" then
    bad(3, arg, name, "table expected, got " .. type(t))
  end
end

local function length(t, name)
  if type(t) ~= "table" then
    bad(3, 1, name, "table expected, got " .. type(t))
  end
  local n = tointeger(#t)
  if n == nil then
    error("object length is not an integer", 3)
  end
  return n
end

table.insert = function(t, ...)
  local e = length(t, "insert") + 1
  local count = select("#", ...)
  if count == 1 then
    t[e] = ...
    return
  elseif count ~= 2 then
    error("wrong number of arguments to 'insert'", 2)
  end
  local pos, value = ...
  pos = integer(pos, 2, "insert")
  if pos < 1 or pos > e then
    bad(2, 2, "insert", "position out of bounds")
  end
  for i = e, pos + 1, -1 do
    t[i] = t[i - 1]
  end
  t[pos] = value
end

table.remove = function(t, pos)
  local size = length(t, "remove")
  if pos == nil then
    pos = size
  else
    pos = integer(pos, 2, "remove")
    if pos ~= size and (pos < 1 or pos > size + 1) then
      bad(2, 2, "remove", "position out of bounds")
    end
  end
  local value = t[pos]
  for i = pos, size - 1 do
    t[i] = t[i + 1]
  end
  t[pos < size and size or pos] = nil
  return value
end

table.move = function(a1, f, e, t, a2)
  local given = a2 ~= nil
  if not given then
    a2 = a1
  end
  f = integer(f, 2, "move")
  e = integer(e, 3, "move")
  t = integer(t, 4, "move")
  check_table(a1, 1, "move")
  check_table(a2, 5, "move")
  if e >= f then
    if not (f > 0 or e < maxinteger + f) then
      bad(2, 3, "move", "too many elements to move")
    end
    local n = e - f + 1
    if t > maxinteger - n + 1 then
      bad(2, 4, "move", "destination wrap around")
    end
    if t > e or t <= f or (given and a1 ~= a2) then
      for i = 0, n - 1 do
        a2[t + i] = a1[f + i]
      end
    else
      for i = n - 1, 0, -1 do
        a2[t + i] = a1[f + i]
      end
    end
  end
  return a2
end
