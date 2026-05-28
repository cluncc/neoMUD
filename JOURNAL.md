# neoMUD — Engineering Journal

## What Is This?

neoMUD is a multi-user dungeon (MUD) engine written in Rust. It is simultaneously a classic text-based RPG server and a modern, scriptable game engine. Players connect over telnet or SSH, create characters, explore interconnected rooms, fight NPCs, trade with merchants, craft items, and interact with a world whose behaviour is defined entirely in data files and scripts — no recompilation required.

---

## Technologies

### Rust (async / Tokio)

All network I/O is non-blocking. A single binary handles any number of simultaneous players via Tokio's async runtime. Each player session runs as an independent `tokio::spawn` task rather than a thread, keeping memory usage low even at scale.

### russh / russh-keys

SSH transport is provided by the `russh 0.44` crate. The server accepts connections on a configurable port (default 2222) and rejects no auth method at the transport layer — all real authentication is done inside the MUD login flow. The SSH username is used only as a name hint to skip the "What is your name?" prompt. Ed25519 host keys are generated on first start and persisted to disk.

### Rhai (scripting)

Every room, NPC, and item can have a `.rhai` script in `world/scripts/`. Scripts implement lifecycle hooks (`on_enter`, `on_exit`, `on_tick`, `on_say`, `on_attack`, `on_die`, `on_use`, `on_pickup`, `describe`) that return arrays of structured action objects the engine then executes. Scripts are sandboxed (operation limit 50k, string limit 4k, array/map size limited) and hot-reloadable via an AST cache.

### Argon2id (password hashing)

Player passwords are hashed with Argon2id using a random salt (via the `argon2 0.5` crate). All accounts use Argon2id hashes.

### DashMap

`DashMap` provides a concurrent, lock-free map used for the active session registry (player name → message sender). This allows the game loop to send output to players without holding the global `RwLock<GameState>`.

### Serde / TOML

All world data — areas, rooms, NPCs, items, exits, shop inventories, dialogue trees, craft recipes, weather tables — is declared in `.toml` files under `world/areas/`. The engine deserializes them at startup with zero generated code, thanks to Serde derive macros.

### Tracing

Structured logging via the `tracing` / `tracing-subscriber` crate stack. Log level is controlled with the `RUST_LOG` environment variable (e.g. `RUST_LOG=info`).

---

## Architecture

```
src/
├── main.rs       — entry point, spawns telnet + SSH listeners + game loop
├── lib.rs        — library root exposing modules for integration tests
├── config.rs     — ServerConfig / GameConfig from config.toml
├── state.rs      — GameState (authoritative world model), game loop, combat tick
├── session.rs    — telnet session state machine + shared login helpers
├── ssh.rs        — SSH session state machine (reuses session.rs helpers)
├── commands.rs   — command dispatch, all player commands, password utilities
├── entity.rs     — Player, Stats, Race, Class, Item, NPC, inventory
├── world.rs      — Area, Room, Exit, NpcTemplate, ItemTemplate, parse_area_file_str
├── scripting.rs  — Rhai engine wrapper, script cache, action conversion
├── combat.rs     — attack resolution, damage formulas, combat messages
├── color.rs      — ANSI color/style helpers, health bar, output formatting
├── events.rs     — GameEvent enum (PlayerConnected, PlayerDisconnected, etc.)
├── error.rs      — reserved (empty)
├── server.rs     — telnet TCP listener
└── time.rs       — in-game time / weather cycle
```

### Session State Machine

Both telnet and SSH sessions share the same five-phase state machine:

```
AwaitingName → AwaitingPassword → NewCharRace → NewCharClass → Playing
```

The telnet path lives in `session.rs`; SSH in `ssh.rs`. All shared helpers (`sanitize_name`, `do_login`, `build_welcome`, `build_race_menu`, `build_class_menu`, `parse_race_choice`, `parse_class_choice`) are `pub` in `session.rs` and imported by `ssh.rs`.

### Game Loop

`state.rs` runs a tick loop (configurable interval, default ~250 ms) that handles NPC AI, combat resolution, respawns, weather advancement, and script `on_tick` hooks. The loop holds a write lock on `GameState` only for the duration of each tick.

### Player Persistence

Each player is stored as a pretty-printed JSON file under `data/players/<name>.json`. Writes are atomic: the engine first writes to `<name>.json.tmp`, then `rename()`s it into place. Loading validates that the `name` field in the file matches the requested name (case-insensitive) to prevent tampered-file spoofing.

---

## Security Considerations

### AuthN / AuthZ

- Passwords are hashed with Argon2id (memory-hard, salted). No password is ever stored in plaintext.
- Incorrect passwords result in an immediate disconnect rather than a retry loop, limiting brute-force attempts. The russh `auth_rejection_time` adds a 1-second delay at the SSH transport layer.
- The SSH transport layer accepts all auth methods (none / password / pubkey) because the MUD implements its own login flow. The SSH username is treated as an untrusted hint only.
- Admin commands check `player.is_admin` before executing.

### Path Traversal

- `safe_name_for_path` (in `entity.rs`) rejects any player name that contains non-ASCII-alphanumeric characters or exceeds 32 chars. This prevents `../`, null bytes, and other filesystem-hostile sequences from appearing in file paths.
- `sanitize_name` (in `session.rs`) only retains alphabetic characters, capitalising the first. Characters such as `.`, `/`, `\0`, and digits are silently stripped, so a name like `../admin` becomes `Admin`.

### Input Validation

- All player input is capped at 512 bytes before any processing.
- Command arguments are validated at the point of use (e.g. item names checked against inventory, room IDs validated before movement, description capped at 200 chars, alias expansions capped at 100 chars).
- TOML world data is parsed with strict serde typing; unexpected keys are ignored; malformed files are rejected with a descriptive error at startup.

### Race Conditions

- `GameState` is wrapped in a `tokio::sync::RwLock`. All mutations (combat, NPC death, item pickup, etc.) acquire the write lock. The session channel map uses `DashMap` for lock-free concurrent access.
- Player disconnection clears the NPC's `in_combat_with` field before removing the player, preventing dangling combat state.
- Player files are written atomically via `write → rename` to prevent partial saves from being read on crash.

### Injection

- No SQL. Player data is stored in JSON files.
- Rhai scripts run inside the engine sandbox with an operation budget and size limits. Scripts cannot access the filesystem or network.
- ANSI color codes are the only "markup" sent to clients; no HTML, no templating.

---

## Configuration (`config.toml`)

```toml
[server]
bind_addr = "0.0.0.0"
telnet_port = 4000
ssh_port = 2222
ssh_host_key_path = "data/ssh_host_key"
motd = "Welcome to neoMUD!"

[game]
players_path = "data/players"
world_path = "world"
start_room = "nexus:entrance"
tick_interval_ms = 250
```

All fields have compiled-in defaults; the config file is optional.

---

## World Data (`world/areas/`)

Each area is a `.toml` file with three top-level sections:

```toml
[area]
id = "nexus"
name = "The Nexus"
# ...rooms, exits, NPC spawns, item spawns, shop inventories

[npcs]
# NPC templates keyed by ID

[items]
# Item templates keyed by ID
```

Rooms reference each other via `"area_id:room_id"` strings (e.g. `"deepwood:entrance"`). Exits carry optional flags (`no_flee`, `hidden`, `locked`). The engine validates exit targets at startup and warns on broken links.

---

## Scripts (`world/scripts/`)

A room script named `nexus_entrance.rhai` is loaded when any hook fires in `nexus:entrance`. Example:

```rhai
fn on_enter(ctx) {
    if rand_bool(20) {
        #{
            action: "tell_player",
            player: ctx.player,
            msg: "A faint shimmer passes through the air."
        }
    } else {
        []
    }
}
```

Hooks return either a single action map or an array of them. The engine processes all returned actions after the hook returns.

---

## Testing

Integration tests live in `tests/integration.rs` and cover:

| Module | What is tested |
|---|---|
| `parse_input` | Verb/arg splitting, whitespace trimming, empty input |
| `passwords` | Argon2id round-trip, wrong password rejection, legacy SHA-256 compat, garbage hash rejection, uniqueness (random salts), very long passwords |
| `sanitize` | Capitalisation, non-alpha stripping, path traversal characters, null bytes, empty string |
| `char_creation` | Race/class choice parsing, range validation, whitespace trimming |
| `world` | TOML area loading, room counts, exits present, parse error handling |
| `player_io` | Save/reload round-trip, exists before/after save, load of nonexistent player, path traversal via name, slash in load name, name mismatch in tampered file |
| `scripting` | Missing script → empty action list, missing script → None describe |
| `color` | ANSI wrapping, health bar non-empty at 0/max/over-max HP |
| `combat` | Damage floor ≥1, critical doubles damage, lethal vs partial damage, XP level scaling, flee returns bool, poison DoT, regen HoT, expired effects removed |
| `entity` | Level-up XP threshold, HP/MP gains on level-up, reputation all tiers + clamping, find_item / take_item by keyword, Stats for class+race combos, is_in_combat default false |
| `time_tests` | time_of_day for all hour ranges, tick advances clock, weather transition no panic, weather Display strings are lowercase |
| `world_tests` | contextual_description fallback, night variant, heavy-rain variant differs from clear |

Run all tests with:

```sh
cargo test
```

---

## Deployment

### Prerequisites

- Rust toolchain (stable, 1.75+): `curl https://sh.rustup.rs -sSf | sh`
- No external database required.

### Build

```sh
git clone <repo>
cd neoMUD
cargo build --release
```

The binary is at `target/release/neomud`.

### First Run

```sh
mkdir -p data/players
./target/release/neomud
```

On first start:
- An Ed25519 SSH host key is generated at `data/ssh_host_key` (path configurable).
- World files are loaded from `world/areas/*.toml`.
- Telnet listener starts on port 4000; SSH on port 2222.

### Configuration Override

Copy `config.toml` from the repo root and edit as needed. The binary searches for `config.toml` in the current working directory.

```sh
cp config.toml /etc/neomud/config.toml
./target/release/neomud --config /etc/neomud/config.toml
```

### Running as a Service (systemd)

```ini
[Unit]
Description=neoMUD server
After=network.target

[Service]
Type=simple
WorkingDirectory=/opt/neomud
ExecStart=/opt/neomud/neomud
Restart=on-failure
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
```

```sh
sudo cp neomud.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now neomud
```

### Firewall

Open ports 4000 (telnet) and 2222 (SSH) inbound:

```sh
# ufw
sudo ufw allow 4000/tcp
sudo ufw allow 2222/tcp

# firewalld
sudo firewall-cmd --add-port=4000/tcp --permanent
sudo firewall-cmd --add-port=2222/tcp --permanent
sudo firewall-cmd --reload
```

### Connecting

```sh
# Telnet
telnet <host> 4000

# SSH
ssh -p 2222 <your-character-name>@<host>
```

The SSH username is used as a character name hint. You will still be prompted to set or confirm your password through the MUD's own login flow.

### Logs

```sh
RUST_LOG=debug ./neomud   # verbose
RUST_LOG=info  ./neomud   # normal
journalctl -u neomud -f   # when running via systemd
```

### Updating World Data

World TOML files are loaded at startup. To add or modify areas, edit or add files under `world/areas/` and restart the server. Scripts under `world/scripts/` are hot-reloaded from disk on each call — no restart required for script changes.

### Player Data

Player saves are JSON files in `data/players/`. They can be backed up with any standard file copy. No migration tooling is needed when adding new optional fields — serde's `default` attribute ensures old saves deserialise cleanly.

---

## Changelog

### Bug Fixes

- **`set_flag` scripting action**: `water` and `underwater` room flags were documented in ENGINE.md but silently ignored in `apply_script_actions`. Both flags now map correctly to `RoomFlags`.
- **Post-death counter-attack**: After a player died in combat and was respawned at 1 HP, the combat loop would execute a counter-attack because `is_alive()` returned true on the respawned player. The fix checks `in_combat_with.is_none()` (which is set to None by `handle_player_death`) before executing the counter-attack.
- **NPC respawn MP not reset**: `process_respawns` restored HP but not MP. NPC mana is now also fully restored on respawn.
- **`cmd_flee` missing room render**: After a successful flee, the player's room changed but no room description was shown. `drop(state)` + `render_room` is now called after a successful flee.
- **`cmd_put` silent no-op**: The `put` command dispatched to an empty function that returned nothing, giving the player no feedback. It now supports "put X in Y" with full container validation.

### Feature Implementations

- **`cmd_reply`**: Added `last_tell: Arc<DashMap<String, String>>` to `GameHandle`. `cmd_tell` now records the sender in that map whenever a tell is delivered; `cmd_reply` looks up the last sender and reuses the tell flow.
- **`cmd_put`**: Fully implemented. Parses "put X in Y", validates both items are in the player's inventory, checks the container's `ItemType::Container` type and its `container_size` capacity, adjusts indices correctly when removing the item before inserting into contents, and rejects putting a container inside itself.
- **`cmd_write`**: Writes arbitrary text into a `Book`-type inventory item by setting `ItemInstance.custom_desc`. Enforces a 500-character limit. `cmd_read` was updated to prefer `custom_desc` over the template's built-in content, so player-written text is displayed when reading.
- **`cmd_craft`**: Fully implemented. Parses "craft X with Y", finds matching ingredient items in inventory, searches all item templates (global and area-local) for a `CraftRecipe` whose two-item ingredient list matches, checks optional skill requirements, consumes both ingredients, creates the result item, and records the recipe label in `player.known_recipes`. "craft list" still works to display known recipes.
- **World data**: Added `nexus:leather_sack` (Container, capacity 8), `nexus:blank_journal` (writable Book), `nexus:herb_bundle` (Crafting ingredient), and `nexus:crude_salve` (Consumable result with a craft recipe that combines herb_bundle + antidote). The inn room now spawns the sack, two journals, and three herb bundles so players can find them at start.

### Security Hardening

- **SHA-256 constant-time comparison**: Legacy SHA-256 password verification used `== stored_hash` which is a regular string comparison and leaks timing information. Replaced with a constant-time byte-by-byte XOR comparison (`ct_eq`). The legacy path has since been removed entirely (see below).
- **Message length limits**: Communication commands (`say`, `tell`, `shout`, `chat`) now enforce a 200-character limit; `emote` enforces 150 characters. Previously, the only limit was the 512-byte socket input buffer.

### Dead Code Removal

- Removed legacy SHA-256 password verification path, `ct_eq` helper, and `sha2`/`hex` crate dependencies. All stored hashes are Argon2id; any SHA-256 player files will simply fail to authenticate, prompting a new account.
- Removed `Player::in_combat_with_player` (PvP not implemented; `is_in_combat()` now only checks NPC combat).
- Removed `AuthoredBook` struct and `authored_books` field (write is a stub; field was never populated).
- Removed `Player::find_item_mut` (unused; `find_item` + `take_item` cover all call sites).
- Removed `patrol_path` and `patrol_index` from `ActiveNpc` (patrol AI is not implemented; fields were set to `None`/`0` and never read).
- Removed empty `error.rs` module from module declarations in `main.rs` and `lib.rs`.
- Cleaned up `_can_afford` unused binding in `cmd_buy`.

---

## Known Limitations / Future Work

- **No TLS on telnet**: Telnet traffic is unencrypted. Use SSH for any environment where eavesdropping is a concern.
- **Single-process**: The game state is in-process memory. Horizontal scaling would require extracting state to an external store (Redis, etc.).
- **No account system**: Each character name is a separate identity. Alts are trivially created.
- **No rate limiting** on new character creation or failed logins beyond the 1-second SSH rejection delay.

---

## 2026-05-28 — cleanup pass

Another sweep through the tree to drop code that nothing calls and tidy a few
lints. Every removal was verified with `cargo check`, `cargo build
--all-targets`, `cargo clippy --all-targets`, and `cargo test` (86 tests, all
green).

### Removed (dead — no call sites in src/, tests/, or `world/scripts/`)

- `src/error.rs` — empty placeholder file. The module declaration was removed
  in a previous pass but the file remained on disk; deleted.
- `GameState::npcs_in_room` (state.rs) — `#[allow(dead_code)]` helper with no
  callers. NPC-in-room lookups are done inline by the few sites that need them.
- `TimeOfDay::ambient_light`, `Weather::affects_visibility`,
  `Weather::combat_modifier`, `GameTime::is_daytime` (time.rs) — speculative
  hooks for visibility and weather-aware combat that were never wired up.
- `Stats::hp_percent`, `Stats::condition_string` (entity.rs) — health-string
  helpers that no command, script action, or render path used.
- `Skill::use_skill`, `Skill::effectiveness` (entity.rs) — leveling/scaling
  hooks. Real skill progression is driven by `Player::known_skills` plumbing in
  `commands.rs`; these methods were orphaned.
- `Equipment::armor_bonus` (entity.rs) — placeholder armor formula; combat uses
  `Stats::armor_class` directly.
- `ActiveNpc::update_memory` (entity.rs) — NPC memory write helper. Memory is
  read in dialogue (`memory_of`) but never written; the script-driven flag
  system handles persistent NPC state.

### Stale lint allow

- `parse_area_file_str` (world.rs) had `#[allow(dead_code)]` but is used by
  `tests/integration.rs` (via the lib crate) and by the binary's `#[cfg(test)]`
  loader. Replaced the bare allow with a comment explaining why the binary's
  non-test build still flags it.

### Small refactors

- Module-level `///` doc blocks followed by a blank line in `lib.rs`,
  `color.rs`, `scripting.rs`, `session.rs`, `ssh.rs`, and `tests/integration.rs`
  converted to `//!` inner doc comments (silences the
  `empty_line_after_doc_comments` lint and is actually what the docs were
  trying to be).
- `session.rs` and `ssh.rs`: replaced two manual `name_tag[4..]` slices after
  `starts_with("new:")` with `strip_prefix("new:")`, removing the implicit
  panic surface if the prefix shape ever changes.
- `GameState::announce_weather_change`: two `for (_, x) in &map` loops
  rewritten as `map.values()` since the keys were unused.

### Left alone (deliberately)

- `ServerConfig::max_players` — never read in code but is part of the public
  `config.toml` schema. Removing it would silently break user configs. Kept
  with its `#[allow(dead_code)]` until a real enforcement path lands.
- The remaining clippy warnings (`manual_is_multiple_of`, `type_complexity` on
  the three `parse_area_file_*` returns, `sort_by_key` suggestions in
  `commands.rs`, the `clamp`-pattern in `cmd_set`, a collapsible match in
  `state.rs`) are stylistic and would either churn signatures across the
  call-graph or hurt local readability. Not worth touching in a cleanup pass.
- The color palette in `color.rs` keeps `magenta` and `bright_magenta` even
  though only the semantic aliases (`tell_text`, `shout_text`) reference them
  today — the public palette is intentional API surface.

