# neoMUD Scripting Engine Reference

Scripts extend the world without touching Rust. Rooms, NPCs, and items can each carry a reference to a `.rhai` file. When the engine triggers a lifecycle event on that entity, it compiles the file (once), calls the relevant function, and executes the actions the function returns.

Everything here describes what the engine *actually does today*. All hooks and actions listed in this document are implemented and active.

---

## The Language: Rhai

Scripts are written in [Rhai](https://rhai.rs), a scripting language designed to be embedded in Rust applications. Its syntax is a close relative of Rust with JavaScript-flavored conveniences:

- Dynamically typed (`let x = 42; let y = "hello";`)
- Familiar control flow: `if`, `while`, `for`, `loop`
- Arrays (`[1, 2, 3]`) and object maps (`#{ key: value }`)
- String concatenation with `+`
- String methods: `.to_lower()`, `.to_upper()`, `.len()`, `.trim()`, `.contains()`
- `in` operator for membership: `"word" in some_string`
- No filesystem, network, or process access — by design

Rhai is not Rust. There are no lifetimes, no ownership, no generics. All values are cloned freely.

---

## File Layout

```
world/
  areas/
    nexus.toml        ← area definition; rooms/NPCs/items reference scripts by filename
    deepwood.toml
  scripts/
    herald.rhai       ← script files; flat directory, no subdirectory nesting
    merchant.rhai
    spirit_grove.rhai
    library.rhai
    standing_stones.rhai
```

Scripts live in `world/scripts/`. An entity references a script by filename only (e.g. `"herald.rhai"`), not by path. The engine prepends the world path at load time.

---

## Attaching Scripts to Entities

### Rooms (in area TOML)

```toml
[[rooms]]
id = "library"
name = "The Archive"
description = "Shelves of ancient tomes stretch into shadow."
script = "library.rhai"
```

### NPC Templates

```toml
[npcs.herald]
id = "nexus:herald"
name = "The Herald"
script = "herald.rhai"
# ...other fields
```

### Item Templates

```toml
[items.orb_of_sight]
id = "nexus:orb_of_sight"
name = "Orb of Sight"
script = "orb.rhai"
# ...other fields
```

All three `script` fields are `Option<String>` — omit the field entirely if the entity needs no scripting.

---

## How the Engine Processes Scripts

1. **First call**: the engine compiles the `.rhai` file into an AST and caches it. Subsequent calls use the cached AST. The cache is per-process; restart the server to pick up script changes (or call `reload_all` if you wire it to an admin command).
2. **Hook dispatch**: the engine calls the named function (e.g. `on_enter`) with a single `ctx` map argument. If the function doesn't exist in the script, the error is silently swallowed and no actions are returned — you can safely have a script that only defines some hooks.
3. **Action parsing**: the return value is expected to be an array of maps. Each map must have an `"action"` key whose value is a string naming the action to perform. The engine reads the remaining keys as parameters. A return value that is not an array, or maps that lack an `"action"` key, are silently skipped.
4. **Action execution**: after the hook returns, the engine iterates over the parsed actions and executes them in order against the live game state.

### Sandbox Limits

These limits are enforced by the Rhai engine. Exceeding any of them aborts the script with an error logged to the server — no actions from that call are executed.

| Limit | Value |
|---|---|
| Max operations per call | 50,000 |
| Max string length | 4,096 bytes |
| Max array length | 1,024 elements |
| Max map size | 256 entries |

Design scripts to be short and stateless. The operation budget is generous for logic but will be exhausted by tight loops over large data.

---

## Hooks

A hook is a top-level function in the script file. The engine calls it by name. Hooks that are not defined in a given file are silently ignored.

### Hooks

---

#### `on_enter(ctx)` — Room hook

Called when a player successfully moves into a room that has a script.

**ctx fields:**

| Field | Type | Description |
|---|---|---|
| `ctx.player` | string | Name of the player who entered |
| `ctx.room` | string | Full room ID (`"area:room"`) |
| `ctx.from_dir` | string | Direction the player came from (`"north"`, `"south"`, etc.) |

**Return:** array of actions, or `[]`.

```rhai
fn on_enter(ctx) {
    let actions = [];
    if rand_bool(25) {
        actions.push(#{
            action: "tell_player",
            player: ctx.player,
            msg: "A chill runs down your spine as you enter."
        });
    }
    actions.push(#{
        action: "record_history",
        room: ctx.room,
        event: ctx.player + " entered."
    });
    actions
}
```

---

#### `describe(ctx)` — Room hook

Called when the engine renders a room description. If this function returns a non-empty string, it replaces the TOML description (including any `[descriptions]` conditionals). Return an empty string `""` to fall back to the TOML description.

**ctx fields:**

| Field | Type | Description |
|---|---|---|
| `ctx.player` | string | Name of the player viewing the room |
| `ctx.time` | string | Time-of-day label: `"dawn"`, `"morning"`, `"day"`, `"afternoon"`, `"evening"`, `"night"`, `"deep night"` |
| `ctx.weather` | string | Current weather: `"Clear"`, `"Cloudy"`, `"Rain"`, `"Thunderstorm"`, `"Fog"`, `"Snow"`, `"Blizzard"`, `"Heatwave"` |
| `ctx.hour` | integer | Game hour, 0–23 |

**Return:** string description, or `""` to use the TOML default.

```rhai
fn describe(ctx) {
    if ctx.weather == "thunderstorm" {
        return "Rain hammers the stones. Lightning silhouettes the tower against a purple sky.";
    }
    if ctx.time == "night" {
        return "The torches have burned low. Shadows pool in every corner.";
    }
    ""   // fall through to TOML description
}
```

---

#### `on_exit(ctx)` — Room hook

Called before a player leaves a room (while they are still in it). Actions execute against the departing player's current location.

**ctx fields:**

| Field | Type | Description |
|---|---|---|
| `ctx.player` | string | Player name |
| `ctx.room` | string | Full room ID the player is leaving |
| `ctx.to_dir` | string | Direction of the exit being taken |

---

#### `on_tick(ctx)` — Room or NPC hook

Called every 4 engine ticks (~1 second) for each scripted room that has at least one player in it, and for each alive scripted NPC in an occupied room. Not called for empty rooms or for dead NPCs, keeping the overhead proportional to actual player activity.

Use `rand_bool` with a low probability to produce occasional ambient events:

```rhai
fn on_tick(ctx) {
    if rand_bool(2) {   // 2% per call ≈ once every ~50 seconds on average
        return [#{ action: "tell_room", room: ctx.room, msg: "..." }];
    }
    []
}
```

**ctx fields for rooms:**

| Field | Type | Description |
|---|---|---|
| `ctx.room` | string | Full room ID |
| `ctx.players` | array | Names of players currently in the room |
| `ctx.time` | string | Time-of-day label |
| `ctx.hour` | integer | Game hour, 0–23 |

**ctx fields for NPCs:**

| Field | Type | Description |
|---|---|---|
| `ctx.npc` | string | NPC instance ID |
| `ctx.npc_name` | string | NPC display name |
| `ctx.room` | string | Room ID the NPC is in |
| `ctx.time` | string | Time-of-day label |
| `ctx.hour` | integer | Game hour, 0–23 |

---

#### `on_say(ctx)` — Room or NPC hook

Called after a player uses the `say` command, once for the room's script and once for each alive NPC in the room that has a script. NPC ctx additionally includes `ctx.npc` with the NPC's name.

**ctx fields:**

| Field | Type | Description |
|---|---|---|
| `ctx.player` | string | Player who spoke |
| `ctx.room` | string | Full room ID |
| `ctx.message` | string | The text the player said |
| `ctx.npc` | string | NPC name (NPC scripts only) |

```rhai
fn on_say(ctx) {
    let msg = ctx.message.to_lower();
    if "secret" in msg {
        return [#{
            action: "tell_player",
            player: ctx.player,
            msg: "A panel slides open in the wall."
        }];
    }
    []
}
```

---

#### `on_command(ctx)` — Room or NPC hook

Called when a player types a command the engine does not recognise as a built-in. The hook is tried on the room's script and on each alive NPC's script. If any script returns at least one action, those actions execute and the "Unknown command" error is suppressed. Return `[]` to pass the command through as an error.

**ctx fields:**

| Field | Type | Description |
|---|---|---|
| `ctx.player` | string | Player who typed the command |
| `ctx.room` | string | Full room ID |
| `ctx.command` | string | The verb (lowercased) |
| `ctx.args` | string | Everything after the verb |

```rhai
fn on_command(ctx) {
    if ctx.command == "meditate" {
        return [#{
            action: "heal_player",
            player: ctx.player,
            amount: 10
        }];
    }
    []
}
```

---

#### `on_attack(ctx)` — NPC hook

Called when a player initiates combat with the NPC, after combat state is set up. The NPC can respond with actions (e.g. a war cry broadcast to the room).

**ctx fields:**

| Field | Type | Description |
|---|---|---|
| `ctx.player` | string | Attacking player |
| `ctx.npc` | string | NPC instance ID |
| `ctx.npc_name` | string | NPC display name |
| `ctx.room` | string | Full room ID |

---

#### `on_die(ctx)` — NPC hook

Called after an NPC is killed and loot/XP is awarded, before the NPC respawn timer starts. Use this for death speeches, area-wide announcements, or spawning reward items.

**ctx fields:**

| Field | Type | Description |
|---|---|---|
| `ctx.player` | string | Player who dealt the killing blow |
| `ctx.npc` | string | NPC instance ID |
| `ctx.npc_name` | string | NPC display name |
| `ctx.room` | string | Full room ID where the NPC died |

---

#### `on_use(ctx)` — Item hook

Called after a player uses a consumable item and its effects are applied. The item has already been removed from the player's inventory at this point.

**ctx fields:**

| Field | Type | Description |
|---|---|---|
| `ctx.player` | string | Player who used the item |
| `ctx.item` | string | Item display name |
| `ctx.template` | string | Item template ID |
| `ctx.room` | string | Full room ID |

---

#### `on_pickup(ctx)` — Item hook

Called after a player picks up an item and it is placed in their inventory.

**ctx fields:**

| Field | Type | Description |
|---|---|---|
| `ctx.player` | string | Player who picked up the item |
| `ctx.item` | string | Item display name |
| `ctx.template` | string | Item template ID |
| `ctx.room` | string | Full room ID |

---

## Actions

A hook returns an array of action maps. Each map must contain an `"action"` key with a string value naming the action. Additional keys are parameters. All values passed as parameters must be one of: string, integer (`i64`), boolean, or float (`f64`). Nested maps and arrays in parameters are not supported — they will be converted to `null`.

### Implemented Actions

These are processed by `apply_script_actions` in `commands.rs`.

---

#### `tell_player`

Send a message to a specific player's terminal.

```rhai
#{ action: "tell_player", player: ctx.player, msg: "You hear a distant bell." }
```

| Param | Type | Notes |
|---|---|---|
| `player` | string | Player name. Defaults to the player who triggered the hook. |
| `msg` | string | Message text. May contain ANSI escape codes. |

---

#### `tell_room`

Broadcast a message to every player currently in a room.

```rhai
#{ action: "tell_room", room: "nexus:hub", msg: "The obelisk flares with blue light." }
```

| Param | Type | Notes |
|---|---|---|
| `room` | string | Full room ID (`"area:room"`). Defaults to the room that triggered the hook. |
| `msg` | string | Message text. |

---

#### `move_player`

Teleport a player to a different room. The destination must exist; invalid room IDs are silently ignored.

```rhai
#{ action: "move_player", player: ctx.player, to: "nexus:inner_sanctum" }
```

| Param | Type | Notes |
|---|---|---|
| `player` | string | Player name. |
| `to` | string | Full room ID. Must match an existing room. |

This moves the player's `room` field directly — it does not trigger `on_enter` or `on_exit` hooks and does not call `render_room`. Pair with a `tell_player` message to describe what happened.

---

#### `heal_player`

Restore HP to a player, capped at their maximum.

```rhai
#{ action: "heal_player", player: ctx.player, amount: 20 }
```

| Param | Type | Notes |
|---|---|---|
| `player` | string | Player name. |
| `amount` | integer | HP to restore. Negative values are treated as 0. Capped at `max_hp - current_hp`. |

---

#### `damage_player`

Deal HP damage to a player. Does not kill the player directly — brings HP to zero or below; the engine handles death on the next combat tick.

```rhai
#{ action: "damage_player", player: ctx.player, amount: 5 }
```

| Param | Type | Notes |
|---|---|---|
| `player` | string | Player name. |
| `amount` | integer | Damage to deal. Negative values are treated as 0. |

---

#### `record_history`

Append a string to a room's history ring buffer (last 20 entries kept). History is visible to players via the `history` command and to the Herald NPC.

```rhai
#{ action: "record_history", room: ctx.room, event: ctx.player + " solved the puzzle." }
```

| Param | Type | Notes |
|---|---|---|
| `room` | string | Full room ID. Defaults to the triggering room. |
| `event` | string | Plain-text description of the event. |

---

#### `grant_skill`

Add a skill to a player's skill list. If the player already has the skill it is left unchanged (no duplicate, no XP award — skills are boolean flags, not levels).

```rhai
#{ action: "grant_skill", player: ctx.player, skill: "lore" }
```

| Param | Type | Notes |
|---|---|---|
| `player` | string | Player name. |
| `skill` | string | Skill identifier. Any non-empty string is accepted. |

---

#### `adjust_rep`

Add or subtract reputation with a named faction. The amount can be negative (hostile action).

```rhai
#{ action: "adjust_rep", player: ctx.player, faction: "ancient_spirits", amount: 25 }
```

| Param | Type | Notes |
|---|---|---|
| `player` | string | Player name. |
| `faction` | string | Faction identifier. Any non-empty string is accepted. |
| `amount` | integer | Reputation change (positive or negative). |

---

---

#### `tell_area`

Broadcast a message to every player currently in any room of an area.

```rhai
#{ action: "tell_area", area: "nexus", msg: "The obelisk shudders." }
```

| Param | Type | Notes |
|---|---|---|
| `area` | string | Area ID (the part before `:` in a room ID). Required. |
| `msg` | string | Message text. |

---

#### `move_npc`

Move an NPC to a different room, updating both the NPC's position and the room NPC lists. The destination must exist; invalid room IDs are silently ignored.

```rhai
#{ action: "move_npc", npc: ctx.npc, to: "nexus:hub" }
```

| Param | Type | Notes |
|---|---|---|
| `npc` | string | NPC instance ID (from `ctx.npc`). |
| `to` | string | Full room ID of the destination. |

---

#### `spawn_npc`

Instantiate an NPC from a template into a room. Uses the same logic as world startup spawning.

```rhai
#{ action: "spawn_npc", template: "nexus:herald", room: "nexus:hub" }
```

| Param | Type | Notes |
|---|---|---|
| `template` | string | NPC template ID. Must match a template defined in a world TOML file. |
| `room` | string | Full room ID. Defaults to the room that triggered the hook. |

---

#### `spawn_item`

Instantiate an item from a template and place it on the ground in a room.

```rhai
#{ action: "spawn_item", template: "nexus:crystal_shard", room: "nexus:hub" }
```

| Param | Type | Notes |
|---|---|---|
| `template` | string | Item template ID. |
| `room` | string | Full room ID. Defaults to the triggering room. |

---

#### `give_item`

Instantiate an item from a template and place it directly into a player's inventory. Sends a "You receive …" confirmation to the player.

```rhai
#{ action: "give_item", player: ctx.player, template: "nexus:ancient_coin" }
```

| Param | Type | Notes |
|---|---|---|
| `player` | string | Player name. Defaults to the player who triggered the hook. |
| `template` | string | Item template ID. |

---

#### `set_flag`

Toggle a named flag on a player, NPC, or room.

**On players** — writes to `player.quest_flags`, a `HashMap<String, bool>`. Use this to track quest progression, puzzle states, or any boolean condition tied to a specific player.

```rhai
#{ action: "set_flag", target: "player", id: ctx.player, flag: "spoke_to_herald", value: true }
```

**On rooms** — sets a named field of the room's `RoomFlags` struct.

```rhai
#{ action: "set_flag", target: "room", id: "nexus:hub", flag: "safe", value: false }
```

Valid room flags: `safe`, `dark`, `outside`, `no_magic`, `no_recall`, `indoors`, `shop`, `bank`, `death_trap`.

**On NPCs** — currently supports only `alive` (to kill or revive an NPC by script).

```rhai
#{ action: "set_flag", target: "npc", id: ctx.npc, flag: "alive", value: false }
```

| Param | Type | Notes |
|---|---|---|
| `target` | string | `"player"`, `"room"`, or `"npc"`. |
| `id` | string | Player name, room ID, or NPC instance ID. Defaults to the triggering player/room if empty. |
| `flag` | string | Flag name. |
| `value` | bool | `true` or `false`. |

---

## Built-in Functions

Three utility functions are registered on the engine and available in all scripts.

### `rand_range(min, max)` → integer

Returns a random integer in the range `[min, max]` (both ends inclusive).

```rhai
let index = rand_range(0, 3);   // 0, 1, 2, or 3
let arr = ["a", "b", "c", "d"];
let pick = arr[index];
```

### `rand_bool(percent)` → bool

Returns `true` with the given percentage probability (0–100).

```rhai
if rand_bool(10) {   // 10% chance
    // rare event
}
```

### `action(name)` → map

Creates a bare action map with only the `"action"` key set. Useful as a starting point when you want to add parameters afterwards.

```rhai
let a = action("tell_player");
a.player = ctx.player;
a.msg = "Hello.";
```

In practice, the `#{ ... }` literal syntax is cleaner for most cases.

---

## Return Value Format

### Hooks (`on_enter`, `on_tick`, etc.)

Must return an array. Each element must be a map with an `"action"` key.

```rhai
// Correct
[#{ action: "tell_player", player: ctx.player, msg: "..." }]

// Correct — empty, no actions
[]

// Wrong — a bare map is not an array
#{ action: "tell_player", ... }
```

Returning a single map (not wrapped in `[]`) will produce no actions. The engine only iterates the top-level array.

### `describe`

Must return a string. Return `""` (empty string) to signal "use the TOML description instead". Any non-empty string replaces the description entirely.

```rhai
fn describe(ctx) {
    if ctx.time == "night" {
        return "It is dark.";
    }
    ""   // fall through
}
```

---

## ANSI Color in Scripts

Scripts can embed ANSI escape codes directly in strings using `\x1b[...]m` notation. The engine sends raw bytes to the client — no escaping is applied.

Common codes used in the bundled scripts:

| Code | Effect |
|---|---|
| `\x1b[0m` | Reset all attributes |
| `\x1b[1m` | Bold |
| `\x1b[2m` | Dim |
| `\x1b[32m` | Green (nature, healing) |
| `\x1b[33m` | Yellow (merchants, coin) |
| `\x1b[35m` | Magenta (magic) |
| `\x1b[36m` | Cyan (dialogue) |
| `\x1b[93m` | Bright yellow (rewards, skill grants) |

Always close with `\x1b[0m` to avoid leaking color into the player's prompt.

```rhai
msg: "\x1b[36m" + ctx.npc + " says, 'Welcome.'\x1b[0m"
```

Alternatively, you can use ANSI codes defined by the color module, but since scripts have no access to Rust functions beyond those explicitly registered, you must embed the codes literally.

---

## Contextual Room Descriptions (no scripting required)

Before reaching for `describe`, check whether the TOML `[descriptions]` table covers your needs. It supports keys for time-of-day and weather conditions without any code:

```toml
[rooms.descriptions]
night = "The room is pitch black save for a glowing crystal."
rain  = "Water drips through a crack in the ceiling."
night_rain = "The crystal glows dimly through the drumming of rain on stone."
```

Keys are matched in priority order: `"time_weather"` > `"time"` > `"weather"` > base `description`. The `describe` script hook overrides all of these when it returns a non-empty string.

Valid time-of-day keys: `dawn`, `morning`, `day`, `afternoon`, `evening`, `night`, `deep night`.

Valid weather keys (case-sensitive, match the `Weather` enum's `Display` output): `Clear`, `Cloudy`, `Rain`, `Thunderstorm`, `Fog`, `Snow`, `Blizzard`, `Heatwave`.

---

## Script Caching and Reloading

Scripts are compiled from source to an AST on first use and cached for the lifetime of the server process. The cache is keyed by the full file path.

To force a reload of all scripts (e.g. after editing files on disk), call `ScriptEngine::reload_all()`. This clears the cache; each script is recompiled on the next call to it. No admin command currently exposes this — wire it to `cmd_reload` or similar if you need runtime reloads in production.

Scripts that fail to compile (syntax error, etc.) are not cached. The error is logged at `ERROR` level and the hook returns no actions. Fix the syntax and the next call will attempt compilation again.

---

## Dialogue Line Hooks

NPC templates support a `dialogue` array for keyword-triggered responses without a script:

```toml
[[npcs.guard.dialogue]]
trigger = "gate"
response = "The gate closes at midnight. Best be through before then."

[[npcs.guard.dialogue]]
trigger = "password"
response = "I don't know what you're talking about."
script_hook = "guard_password.rhai"   # optional: call a hook after the response
```

The `script_hook` field names a script file. When the dialogue line fires, the engine is intended to call `call_hook(script_hook, "on_dialogue", ctx)` with the same ctx shape as `on_say`. This wiring is not yet implemented — the `script_hook` field is deserialized but not invoked.

---

## Complete Script Example

The following script covers the common patterns: ambient ticks, conditional `on_enter` messages, a `describe` override, and a custom command handler.

```rhai
// world/scripts/ritual_chamber.rhai
// A room with a magical altar that responds to player actions.

fn on_enter(ctx) {
    let actions = [];

    if rand_bool(50) {
        let reactions = [
            "The torches flicker as you enter.",
            "A low hum rises from the altar and then fades.",
            "The temperature drops several degrees.",
        ];
        actions.push(#{
            action: "tell_player",
            player: ctx.player,
            msg: "\x1b[2m" + reactions[rand_range(0, 2)] + "\x1b[0m"
        });
    }

    actions.push(#{
        action: "record_history",
        room: ctx.room,
        event: ctx.player + " entered the chamber."
    });

    actions
}

fn describe(ctx) {
    if ctx.time == "night" || ctx.time == "deep night" {
        return "At night the altar's veins of red crystal pulse with slow light, like a sleeping heartbeat. The rest of the chamber is lost in shadow.";
    }
    ""
}

fn on_command(ctx) {
    let actions = [];

    if ctx.command == "touch" && "altar" in ctx.args {
        actions.push(#{
            action: "tell_player",
            player: ctx.player,
            msg: "\x1b[35mThe altar is cold under your hand, but something stirs beneath the stone.\x1b[0m"
        });
        if rand_bool(15) {
            actions.push(#{
                action: "tell_player",
                player: ctx.player,
                msg: "\x1b[93mAn old memory surfaces. You understand something you did not before.\r\n+Skill: arcana\x1b[0m"
            });
            actions.push(#{ action: "grant_skill", player: ctx.player, skill: "arcana" });
        }
        return actions;
    }

    if ctx.command == "pray" {
        actions.push(#{
            action: "heal_player",
            player: ctx.player,
            amount: 10
        });
        actions.push(#{
            action: "tell_player",
            player: ctx.player,
            msg: "\x1b[32mYou feel the wounds on your body knit closed.\x1b[0m"
        });
        actions.push(#{ action: "adjust_rep", player: ctx.player, faction: "old_gods", amount: 5 });
        return actions;
    }

    actions   // empty — not consumed
}

fn on_tick(ctx) {
    if rand_bool(2) {
        let ambience = [
            "The altar pulses once with deep red light.",
            "A sound like a distant choir rises and falls.",
            "The shadows in the corners lengthen momentarily.",
        ];
        return [#{
            action: "tell_room",
            room: ctx.room,
            msg: "\x1b[2m" + ambience[rand_range(0, 2)] + "\x1b[0m"
        }];
    }
    []
}
```

---

## Quick Reference

### Hooks at a glance

| Hook | Entity | Status | Trigger |
|---|---|---|---|
| `on_enter(ctx)` | Room | **Active** | Player enters room |
| `describe(ctx)` | Room | **Active** | Room description rendered |
| `on_exit(ctx)` | Room | **Active** | Player leaves room |
| `on_tick(ctx)` | Room, NPC | **Active** | Each game tick |
| `on_say(ctx)` | Room, NPC | **Active** | Player uses `say` |
| `on_command(ctx)` | Room, NPC | **Active** | Player types a command |
| `on_attack(ctx)` | NPC | **Active** | Player attacks NPC |
| `on_die(ctx)` | NPC | **Active** | NPC killed |
| `on_use(ctx)` | Item | **Active** | Player uses item |
| `on_pickup(ctx)` | Item | **Active** | Player picks up item |

### Actions at a glance

| Action | Status | Key params |
|---|---|---|
| `tell_player` | **Active** | `player`, `msg` |
| `tell_room` | **Active** | `room`, `msg` |
| `move_player` | **Active** | `player`, `to` |
| `heal_player` | **Active** | `player`, `amount` |
| `damage_player` | **Active** | `player`, `amount` |
| `record_history` | **Active** | `room`, `event` |
| `grant_skill` | **Active** | `player`, `skill` |
| `adjust_rep` | **Active** | `player`, `faction`, `amount` |
| `tell_area` | **Active** | `area`, `msg` |
| `move_npc` | **Active** | `npc`, `to` |
| `spawn_npc` | **Active** | `template`, `room` |
| `spawn_item` | **Active** | `template`, `room` |
| `give_item` | **Active** | `player`, `template` |
| `set_flag` | **Active** | `target`, `id`, `flag`, `value` |
