# neoMUD

A modern, async MUD engine written in Rust. neoMUD runs persistent, scriptable worlds over both raw TCP (port 4000) and SSH (port 2222), with a live scripting layer that lets world-builders extend gameplay without touching engine code.

## Features

- **Async runtime** — built on Tokio; handles many concurrent connections without blocking
- **Dual transport** — raw TCP telnet and SSH via `russh`; players can connect either way
- **Rhai scripting** — rooms, NPCs, and items each carry an optional `.rhai` script; the engine calls lifecycle hooks and executes the returned actions against live game state
- **Dynamic world** — world definition lives in TOML files under `world/`; no recompile needed to change areas, NPCs, or items
- **Time and weather** — 24-hour game clock (configurable multiplier) and weather system with eight states; both are available to scripts and conditional room descriptions
- **Combat** — round-based combat with configurable hit chance, round duration, and flee chance
- **Reputation and skills** — per-player faction reputation and boolean skill flags, both grantable from scripts
- **Player persistence** — player state saved to disk; passwords hashed with Argon2
- **Sandboxed scripts** — Rhai operation limits prevent runaway scripts from affecting server performance

## Building

```sh
cargo build --release
```

Requires Rust 2021 edition or later.

## Running

```sh
cargo run --release -- --config config.toml
```

The server reads `config.toml` from the working directory by default. Adjust `world_path` and `players_path` as needed.

### Default ports

| Transport | Port |
|-----------|------|
| TCP (telnet) | 4000 |
| SSH | 2222 |

Both are configurable in `config.toml`.

## Configuration

`config.toml` controls server, game, and combat settings:

```toml
[server]
port = 4000
bind_addr = "0.0.0.0"
max_players = 100
ssh_port = 2222
ssh_host_key_path = "data/ssh_host_key"

[game]
tick_rate_ms = 250
game_time_multiplier = 60   # 1 real minute = 1 game hour
world_path = "world"
players_path = "players"
start_room = "nexus:entrance"

[combat]
base_hit_chance = 65
round_duration_ticks = 8    # 2 seconds at 250ms ticks
flee_success_chance = 40
```

## World Layout

```
world/
  areas/
    nexus.toml        # area definition; rooms/NPCs/items reference scripts by filename
    deepwood.toml
  scripts/
    herald.rhai       # script files; flat directory, no subdirectory nesting
    merchant.rhai
```

Areas are TOML files. Each area declares rooms, NPC templates, and item templates. Entities reference scripts by filename only — the engine prepends the world path at load time.

## Scripting

Scripts are written in [Rhai](https://rhai.rs), a lightweight scripting language designed for embedding in Rust. A script file defines hook functions; the engine calls them on lifecycle events and executes the returned action list.

### Hooks

| Hook | Entity | Trigger |
|------|--------|---------|
| `on_enter(ctx)` | Room | Player enters |
| `describe(ctx)` | Room | Room description rendered |
| `on_exit(ctx)` | Room | Player leaves |
| `on_tick(ctx)` | Room, NPC | Each game tick (occupied rooms/alive NPCs only) |
| `on_say(ctx)` | Room, NPC | Player uses `say` |
| `on_command(ctx)` | Room, NPC | Unrecognised player command |
| `on_attack(ctx)` | NPC | Player attacks NPC |
| `on_die(ctx)` | NPC | NPC killed |
| `on_use(ctx)` | Item | Player uses item |
| `on_pickup(ctx)` | Item | Player picks up item |

### Actions

Hooks return an array of action maps. Available actions:

| Action | Key params |
|--------|------------|
| `tell_player` | `player`, `msg` |
| `tell_room` | `room`, `msg` |
| `tell_area` | `area`, `msg` |
| `move_player` | `player`, `to` |
| `move_npc` | `npc`, `to` |
| `heal_player` | `player`, `amount` |
| `damage_player` | `player`, `amount` |
| `grant_skill` | `player`, `skill` |
| `adjust_rep` | `player`, `faction`, `amount` |
| `spawn_npc` | `template`, `room` |
| `spawn_item` | `template`, `room` |
| `give_item` | `player`, `template` |
| `set_flag` | `target`, `id`, `flag`, `value` |
| `record_history` | `room`, `event` |

See `ENGINE.md` for the full scripting reference, including hook context fields, sandbox limits, built-in functions (`rand_range`, `rand_bool`, `action`), and ANSI color usage.

### Quick example

```rhai
fn on_enter(ctx) {
    if rand_bool(25) {
        return [#{
            action: "tell_player",
            player: ctx.player,
            msg: "A chill runs down your spine."
        }];
    }
    []
}
```

## Authors

[ytcracker](https://github.com/realytcracker) and [clord](https://github.com/clord)

## License

See repository for license details.
