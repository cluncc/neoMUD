use rand::Rng;
use crate::color::*;
use crate::entity::{ItemInstance, Player};
use crate::events::GameEvent;
use crate::state::GameHandle;
use crate::world::{NpcBehavior, ItemType};

/// Parse a raw input line into (verb, rest_of_line).
pub fn parse_input(input: &str) -> (&str, &str) {
    let s = input.trim();
    if let Some(pos) = s.find(' ') {
        (&s[..pos], s[pos + 1..].trim())
    } else {
        (s, "")
    }
}

/// Dispatch a command from a logged-in player.
/// Returns an optional message to send back to the session.
pub async fn dispatch(handle: &GameHandle, player_name: &str, input: &str) {
    let input = input.trim();
    if input.is_empty() {
        return;
    }

    // Expand aliases first
    let expanded = {
        let state = handle.state.read().await;
        if let Some(player) = state.players.get(player_name) {
            let (verb, rest) = parse_input(input);
            if let Some(alias) = player.aliases.get(verb) {
                if rest.is_empty() {
                    alias.clone()
                } else {
                    format!("{} {}", alias, rest)
                }
            } else {
                input.to_string()
            }
        } else {
            input.to_string()
        }
    };

    let (verb, args) = parse_input(&expanded);
    let verb_lower = verb.to_lowercase();

    match verb_lower.as_str() {
        // ── Navigation ──────────────────────────────────────────────────────
        "n" | "north"  => cmd_move(handle, player_name, "north").await,
        "s" | "south"  => cmd_move(handle, player_name, "south").await,
        "e" | "east"   => cmd_move(handle, player_name, "east").await,
        "w" | "west"   => cmd_move(handle, player_name, "west").await,
        "u" | "up"     => cmd_move(handle, player_name, "up").await,
        "d" | "down"   => cmd_move(handle, player_name, "down").await,
        "ne" | "northeast" => cmd_move(handle, player_name, "northeast").await,
        "nw" | "northwest" => cmd_move(handle, player_name, "northwest").await,
        "se" | "southeast" => cmd_move(handle, player_name, "southeast").await,
        "sw" | "southwest" => cmd_move(handle, player_name, "southwest").await,

        // ── Information ─────────────────────────────────────────────────────
        "l" | "look"   => cmd_look(handle, player_name, args).await,
        "ex" | "exa" | "examine" | "x" => cmd_examine(handle, player_name, args).await,
        "sc" | "score" | "stats"   => cmd_score(handle, player_name).await,
        "i" | "inv" | "inventory"  => cmd_inventory(handle, player_name).await,
        "eq" | "equipment"         => cmd_equipment(handle, player_name).await,
        "sk" | "skills"            => cmd_skills(handle, player_name).await,
        "who"                      => cmd_who(handle, player_name).await,
        "time"                     => cmd_time(handle, player_name).await,
        "weather"                  => cmd_weather(handle, player_name).await,
        "rep" | "reputation"       => cmd_reputation(handle, player_name).await,
        "map"                      => cmd_map(handle, player_name).await,
        "help" | "h"               => cmd_help(handle, player_name, args).await,

        // ── Communication ───────────────────────────────────────────────────
        "'" | "say"         => cmd_say(handle, player_name, args).await,
        "t" | "tell"        => cmd_tell(handle, player_name, args).await,
        "yell" | "shout"    => cmd_shout(handle, player_name, args).await,
        ":" | "emote"       => cmd_emote(handle, player_name, args).await,
        "chat" | "ooc"      => cmd_chat(handle, player_name, args).await,
        "whisper"           => cmd_whisper(handle, player_name, args).await,
        "reply" | "r"       => cmd_reply(handle, player_name, args).await,

        // ── Items ────────────────────────────────────────────────────────────
        "get" | "take"      => cmd_get(handle, player_name, args).await,
        "drop"              => cmd_drop(handle, player_name, args).await,
        "put"               => cmd_put(handle, player_name, args).await,
        "give"              => cmd_give(handle, player_name, args).await,
        "wear" | "wield"    => cmd_wear(handle, player_name, args).await,
        "remove"            => cmd_remove(handle, player_name, args).await,
        "use"               => cmd_use(handle, player_name, args).await,
        "buy"               => cmd_buy(handle, player_name, args).await,
        "sell"              => cmd_sell(handle, player_name, args).await,
        "list"              => cmd_list(handle, player_name).await,
        "craft" | "combine" => cmd_craft(handle, player_name, args).await,

        // ── Combat ───────────────────────────────────────────────────────────
        "talk" | "greet" => cmd_talk(handle, player_name, args).await,

        // ── Combat ───────────────────────────────────────────────────────────
        "k" | "kill" | "attack" | "hit" => cmd_attack(handle, player_name, args).await,
        "flee"              => cmd_flee(handle, player_name).await,
        "consider" | "con"  => cmd_consider(handle, player_name, args).await,

        // ── Character ────────────────────────────────────────────────────────
        "title"             => cmd_title(handle, player_name, args).await,
        "describe"          => cmd_describe(handle, player_name, args).await,
        "alias"             => cmd_alias(handle, player_name, args).await,
        "unalias"           => cmd_unalias(handle, player_name, args).await,
        "write"             => cmd_write(handle, player_name, args).await,
        "read"              => cmd_read(handle, player_name, args).await,
        "save"              => cmd_save(handle, player_name).await,
        "quit" | "q"        => cmd_quit(handle, player_name).await,

        // ── Admin ────────────────────────────────────────────────────────────
        "goto"              => cmd_admin_goto(handle, player_name, args).await,
        "spawn"             => cmd_admin_spawn(handle, player_name, args).await,
        "reload"            => cmd_admin_reload(handle, player_name, args).await,
        "teleport" | "tp"   => cmd_admin_teleport(handle, player_name, args).await,
        "info"              => cmd_admin_info(handle, player_name, args).await,
        "shutdown"          => cmd_admin_shutdown(handle, player_name).await,
        "set"               => cmd_admin_set(handle, player_name, args).await,

        _ => {
            // Try on_command hook on the current room and any NPC scripts before
            // falling back to "unknown command".
            let (room_script, npc_scripts, room_id) = {
                let state = handle.state.read().await;
                let room_id = state.players.get(player_name)
                    .map(|p| p.room.clone())
                    .unwrap_or_default();
                let room = state.world.get_room(&room_id);
                let room_script = room.and_then(|r| r.script.clone());
                let npc_scripts: Vec<String> = room.map(|r| {
                    r.npcs.iter()
                        .filter_map(|id| state.npcs.get(id))
                        .filter(|n| n.alive)
                        .filter_map(|n| {
                            state.world.get_npc_template(&n.template_id)
                                .and_then(|t| t.script.clone())
                        })
                        .collect()
                }).unwrap_or_default();
                (room_script, npc_scripts, room_id)
            };

            let ctx = rhai::Dynamic::from({
                let mut m = rhai::Map::new();
                m.insert("player".into(), rhai::Dynamic::from(player_name.to_string()));
                m.insert("room".into(), rhai::Dynamic::from(room_id.clone()));
                m.insert("command".into(), rhai::Dynamic::from(verb_lower.clone()));
                m.insert("args".into(), rhai::Dynamic::from(args.to_string()));
                m
            });

            let mut all_actions = vec![];
            if let Some(ref script_name) = room_script {
                all_actions.extend(handle.scripts.call_hook(script_name, "on_command", ctx.clone()));
            }
            for script_name in &npc_scripts {
                all_actions.extend(handle.scripts.call_hook(script_name, "on_command", ctx.clone()));
            }

            if all_actions.is_empty() {
                handle.state.read().await
                    .tell_player(player_name, &error_msg(&format!("Unknown command '{}'. Type 'help' for a list.", verb)), &handle.sessions).await;
            } else {
                let mut state = handle.state.write().await;
                state.apply_script_actions(all_actions, player_name, &room_id, &handle.sessions).await;
            }
        }
    }
}

// ─── Navigation ──────────────────────────────────────────────────────────────

async fn cmd_move(handle: &GameHandle, player_name: &str, direction: &str) {
    let mut state = handle.state.write().await;

    let player = match state.players.get(player_name) {
        Some(p) => p.clone(), None => return,
    };

    if player.is_in_combat() {
        state.tell_player(player_name, &error_msg("You can't run away while in combat! Try 'flee'."), &handle.sessions).await;
        return;
    }

    let room = match state.world.get_room(&player.room) {
        Some(r) => r.clone(), None => return,
    };

    let exit = match room.exits.get(direction) {
        Some(e) => e.clone(),
        None => {
            state.tell_player(player_name, &error_msg("You can't go that way."), &handle.sessions).await;
            return;
        }
    };

    if exit.locked {
        state.tell_player(player_name, &error_msg("That way is locked."), &handle.sessions).await;
        return;
    }

    let dest_id = exit.to.clone();
    let dest = match state.world.get_room(&dest_id) {
        Some(r) => r.clone(),
        None => {
            state.tell_player(player_name, &error_msg("That exit leads nowhere."), &handle.sessions).await;
            return;
        }
    };

    let old_room = player.room.clone();

    // on_exit: fire before moving the player
    if let Some(script_name) = room.script.clone() {
        let ctx = rhai::Dynamic::from({
            let mut m = rhai::Map::new();
            m.insert("player".into(), rhai::Dynamic::from(player_name.to_string()));
            m.insert("room".into(), rhai::Dynamic::from(room.id.clone()));
            m.insert("to_dir".into(), rhai::Dynamic::from(direction.to_string()));
            m
        });
        let actions = handle.scripts.call_hook(&script_name, "on_exit", ctx);
        state.apply_script_actions(actions, player_name, &room.id, &handle.sessions).await;
    }

    // Announce departure
    let depart_msg = format!("{} leaves {}.", player_name, direction);
    state.tell_room_except(&old_room, player_name, &dim(&depart_msg), &handle.sessions).await;

    // Move the player
    state.players.get_mut(player_name).unwrap().room = dest_id.clone();

    // Announce arrival
    let arrive_msg = format!("{} arrives.", player_name);
    state.tell_room_except(&dest_id, player_name, &dim(&arrive_msg), &handle.sessions).await;

    let _ = handle.events.send(GameEvent::PlayerLeaveRoom {
        player: player_name.to_string(), room: old_room, to_dir: Some(direction.to_string()),
    });
    let _ = handle.events.send(GameEvent::PlayerEnterRoom {
        player: player_name.to_string(), room: dest_id.clone(), from_dir: Some(direction.to_string()),
    });

    // on_enter
    if let Some(script_name) = dest.script.clone() {
        let ctx = rhai::Dynamic::from({
            let mut m = rhai::Map::new();
            m.insert("player".into(), rhai::Dynamic::from(player_name.to_string()));
            m.insert("from_dir".into(), rhai::Dynamic::from(direction.to_string()));
            m.insert("room".into(), rhai::Dynamic::from(dest_id.clone()));
            m
        });
        let actions = handle.scripts.call_hook(&script_name, "on_enter", ctx);
        state.apply_script_actions(actions, player_name, &dest_id, &handle.sessions).await;
    }

    // Show the new room
    drop(state);
    render_room(handle, player_name).await;
}

// ─── Look ─────────────────────────────────────────────────────────────────────

pub async fn render_room(handle: &GameHandle, player_name: &str) {
    let state = handle.state.read().await;
    let player = match state.players.get(player_name) { Some(p) => p, None => return };
    let room = match state.world.get_room(&player.room) { Some(r) => r, None => return };
    let weather = state.get_room_weather(&player.room);
    let time = &state.time;

    let mut output = String::new();

    output.push_str("\r\n");
    output.push_str(&room_title(&room.name));
    output.push_str("\r\n");

    // Check for script override of description
    let desc = if let Some(ref script_name) = room.script {
        let ctx = rhai::Dynamic::from({
            let mut m = rhai::Map::new();
            m.insert("player".into(), rhai::Dynamic::from(player_name.to_string()));
            m.insert("time".into(), rhai::Dynamic::from(time.time_of_day().to_string()));
            m.insert("weather".into(), rhai::Dynamic::from(weather.to_string()));
            m.insert("hour".into(), rhai::Dynamic::from(time.hour as i64));
            m
        });
        handle.scripts.call_describe(script_name, ctx)
            .unwrap_or_else(|| room.contextual_description(time, &weather))
    } else {
        room.contextual_description(time, &weather)
    };

    output.push_str(&room_desc(&desc));
    output.push_str("\r\n");

    // Weather line for outside rooms
    if room.flags.outside && !matches!(weather, crate::time::Weather::Clear) {
        output.push_str(&dim(&format!("  {}\r\n", weather.description())));
    }

    output.push_str("\r\n");

    // NPCs
    let alive_npcs: Vec<_> = room.npcs.iter()
        .filter_map(|id| state.npcs.get(id))
        .filter(|n| n.alive)
        .collect();
    for npc in &alive_npcs {
        let tmpl = state.world.get_npc_template(&npc.template_id);
        let short = tmpl.map(|t| t.short_desc.as_str()).unwrap_or(&npc.name);
        output.push_str(&format!("  {}\r\n", npc_name(short)));
    }

    // Items on ground
    if let Some(items) = state.items_on_ground.get(&room.id) {
        for item in items {
            output.push_str(&format!("  {}\r\n", item_name(&item.name)));
        }
    }

    // Players (excluding self)
    for p in state.players_in_room(&room.id) {
        if p.name != player_name {
            let title = if p.title.is_empty() { String::new() } else { format!(" {}", p.title) };
            output.push_str(&format!("  {}{} is here.\r\n", crate::color::player_name(&p.name), title));
        }
    }

    // Exits
    output.push_str("\r\n");
    output.push_str(&exit_list(&format!("[ Exits: {} ]", room.exit_list())));
    output.push_str("\r\n");

    state.tell_player(player_name, &output, &handle.sessions).await;
}

async fn cmd_look(handle: &GameHandle, player_name: &str, args: &str) {
    if args.is_empty() {
        render_room(handle, player_name).await;
        return;
    }
    cmd_examine(handle, player_name, args).await;
}

async fn cmd_examine(handle: &GameHandle, player_name: &str, target: &str) {
    if target.is_empty() {
        render_room(handle, player_name).await;
        return;
    }
    let state = handle.state.read().await;
    let player = match state.players.get(player_name) { Some(p) => p, None => return };
    let room = match state.world.get_room(&player.room) { Some(r) => r, None => return };
    let kw = target.to_lowercase();

    // Check NPCs in room
    for npc_id in &room.npcs {
        if let Some(npc) = state.npcs.get(npc_id) {
            if npc_matches(npc, &state.world, &kw) {
                let tmpl = state.world.get_npc_template(&npc.template_id);
                let long = tmpl.map(|t| t.long_desc.as_str()).unwrap_or("Nothing special about them.");
                let mem_sentiment = npc.memory_of(player_name).map(|m| m.sentiment).unwrap_or(0);
                let attitude = match mem_sentiment {
                    51..=100 => " They seem quite friendly toward you.",
                    1..=50 => " They regard you warmly.",
                    -50..=-1 => " They eye you with suspicion.",
                    _ if mem_sentiment <= -51 => " They glare at you with hostility.",
                    _ => "",
                };
                state.tell_player(player_name, &format!("{}{}", room_desc(long), info_msg(attitude)), &handle.sessions).await;
                return;
            }
        }
    }

    // Check inventory
    if let Some(item) = player.find_item(&kw) {
        let tmpl = state.world.get_item_template(&item.template_id);
        let desc = item.custom_desc.as_deref()
            .or_else(|| tmpl.map(|t| t.long_desc.as_str()))
            .unwrap_or("You see nothing special about it.");
        state.tell_player(player_name, &room_desc(desc), &handle.sessions).await;
        return;
    }

    // Check room lore (static objects)
    for lore_entry in &room.lore {
        if lore_entry.to_lowercase().starts_with(&kw) {
            let rest = lore_entry[kw.len()..].trim_start_matches(':').trim();
            state.tell_player(player_name, &room_desc(rest), &handle.sessions).await;
            return;
        }
    }

    state.tell_player(player_name, &error_msg("You don't see that here."), &handle.sessions).await;
}

// ─── Score / Stats ────────────────────────────────────────────────────────────

async fn cmd_score(handle: &GameHandle, player_name: &str) {
    let state = handle.state.read().await;
    let player = match state.players.get(player_name) { Some(p) => p, None => return };
    let s = &player.stats;

    let title_line = if player.title.is_empty() {
        format!("{} the {}", player.name, player.class)
    } else {
        format!("{}, {} — {} {}", player.name, player.title, player.race, player.class)
    };

    let hp_bar = health_bar(s.hp, s.max_hp, 20);
    let mp_bar = health_bar(s.mp, s.max_mp, 20);

    let out = format!(
        "\r\n{}\r\n{}\r\n\
         HP: {}/{} {}\r\n\
         MP: {}/{} {}\r\n\
         Coins: {}\r\n\r\n\
         STR:{:3}  DEX:{:3}  CON:{:3}\r\n\
         INT:{:3}  WIS:{:3}  CHA:{:3}\r\n\
         AC: {:3}  Hit:{:+3}  Dam:{:+3}\r\n\r\n\
         Level: {}  XP: {}/{}  Kills: {}  Deaths: {}\r\n",
        bold(&title_line), separator(),
        bright_green(&s.hp.to_string()), bright_green(&s.max_hp.to_string()), hp_bar,
        bright_blue(&s.mp.to_string()), bright_blue(&s.max_mp.to_string()), mp_bar,
        yellow(&player.coins.to_string()),
        s.strength, s.dexterity, s.constitution,
        s.intelligence, s.wisdom, s.charisma,
        s.armor_class, s.hit_bonus, s.dam_bonus,
        player.level, player.experience, player.xp_to_next, player.kills, player.deaths,
    );
    state.tell_player(player_name, &out, &handle.sessions).await;
}

// ─── Inventory ───────────────────────────────────────────────────────────────

async fn cmd_inventory(handle: &GameHandle, player_name: &str) {
    let state = handle.state.read().await;
    let player = match state.players.get(player_name) { Some(p) => p, None => return };
    if player.inventory.is_empty() {
        state.tell_player(player_name, &info_msg("You are carrying nothing."), &handle.sessions).await;
        return;
    }
    let mut out = format!("\r\n{}  ({} coins)\r\n", bold("Inventory:"), player.coins);
    for item in &player.inventory {
        out.push_str(&format!("  {}\r\n", item_name(&item.name)));
    }
    state.tell_player(player_name, &out, &handle.sessions).await;
}

async fn cmd_equipment(handle: &GameHandle, player_name: &str) {
    let state = handle.state.read().await;
    let player = match state.players.get(player_name) { Some(p) => p, None => return };
    if player.equipment.slots.is_empty() {
        state.tell_player(player_name, &info_msg("You are wearing nothing."), &handle.sessions).await;
        return;
    }
    let mut out = format!("\r\n{}\r\n", bold("Equipment:"));
    let mut slots: Vec<_> = player.equipment.slots.iter().collect();
    slots.sort_by_key(|(k, _)| k.as_str());
    for (slot, item) in slots {
        out.push_str(&format!("  {:12} {}\r\n", dim(slot), item_name(&item.name)));
    }
    state.tell_player(player_name, &out, &handle.sessions).await;
}

async fn cmd_skills(handle: &GameHandle, player_name: &str) {
    let state = handle.state.read().await;
    let player = match state.players.get(player_name) { Some(p) => p, None => return };
    if player.skills.is_empty() {
        state.tell_player(player_name, &info_msg("You have no skills."), &handle.sessions).await;
        return;
    }
    let mut out = format!("\r\n{}\r\n", bold("Skills:"));
    let mut skills: Vec<_> = player.skills.values().collect();
    skills.sort_by(|a, b| b.level.cmp(&a.level));
    for skill in skills {
        let pct_bar = "█".repeat((skill.level / 10) as usize);
        out.push_str(&format!(
            "  {:16} {:3}  {:<10}  {}\r\n",
            cyan(&skill.name),
            bright_white(&skill.level.to_string()),
            dim(&format!("{:?}", skill.mastery)),
            dim(&pct_bar),
        ));
    }
    state.tell_player(player_name, &out, &handle.sessions).await;
}

async fn cmd_who(handle: &GameHandle, player_name: &str) {
    let state = handle.state.read().await;
    let mut out = format!("\r\n{}  ({} online)\r\n{}\r\n",
        bold("Players Online:"), state.players.len(), separator());
    let mut players: Vec<_> = state.players.values().collect();
    players.sort_by(|a, b| b.level.cmp(&a.level));
    for p in players {
        let title = if p.title.is_empty() { String::new() } else { format!(" — {}", p.title) };
        let admin = if p.is_admin { bright_red(" [ADMIN]") } else { String::new() };
        out.push_str(&format!(
            "  [{}] {} {} {}{}  {}{}\r\n",
            yellow(&p.level.to_string()),
            bright_white(&p.name),
            dim(&p.race.to_string()),
            dim(&p.class.to_string()),
            title,
            dim(&p.room),
            admin,
        ));
    }
    state.tell_player(player_name, &out, &handle.sessions).await;
}

async fn cmd_time(handle: &GameHandle, player_name: &str) {
    let state = handle.state.read().await;
    let t = &state.time;
    let msg = format!("\r\n{}\r\n  {}\r\n",
        bold("Current Time:"), cyan(&t.display()));
    state.tell_player(player_name, &msg, &handle.sessions).await;
}

async fn cmd_weather(handle: &GameHandle, player_name: &str) {
    let state = handle.state.read().await;
    let player = match state.players.get(player_name) { Some(p) => p, None => return };
    let weather = state.get_room_weather(&player.room);
    let room = state.world.get_room(&player.room);
    let is_outside = room.map(|r| r.flags.outside).unwrap_or(false);
    if !is_outside {
        state.tell_player(player_name, &info_msg("You are indoors and cannot see the weather."), &handle.sessions).await;
    } else {
        state.tell_player(player_name,
            &format!("{}: {}", bold("Weather"), cyan(weather.description())),
            &handle.sessions).await;
    }
}

async fn cmd_reputation(handle: &GameHandle, player_name: &str) {
    let state = handle.state.read().await;
    let player = match state.players.get(player_name) { Some(p) => p, None => return };
    if player.faction_rep.is_empty() {
        state.tell_player(player_name, &info_msg("You have no faction standing."), &handle.sessions).await;
        return;
    }
    let mut out = format!("\r\n{}\r\n", bold("Faction Standing:"));
    let mut reps: Vec<_> = player.faction_rep.iter().collect();
    reps.sort_by(|a, b| b.1.cmp(a.1));
    for (faction, rep) in reps {
        let standing = player.reputation_standing(faction);
        let color_fn: fn(&str) -> String = match *rep {
            751..=1000 => bright_yellow,
            251..=750 => bright_green,
            1..=250 => green,
            -250..=-1 => yellow,
            -500..=-251 => red,
            _ => bright_red,
        };
        out.push_str(&format!(
            "  {:20} {:6}  {}\r\n",
            cyan(faction), rep, color_fn(&standing.to_string())
        ));
    }
    state.tell_player(player_name, &out, &handle.sessions).await;
}

async fn cmd_map(handle: &GameHandle, player_name: &str) {
    // ASCII mini-map of nearby rooms
    let state = handle.state.read().await;
    let player = match state.players.get(player_name) { Some(p) => p, None => return };
    let current_room = match state.world.get_room(&player.room) { Some(r) => r, None => return };

    let mut out = format!("\r\n{}\r\n", bold("Area Map (nearby):"));
    out.push_str(&render_mini_map(current_room, &state.world, 2));
    state.tell_player(player_name, &out, &handle.sessions).await;
}

fn render_mini_map(center: &crate::world::Room, world: &crate::world::World, _depth: u32) -> String {
    // Simple compass rose showing adjacent rooms
    let dirs = [("north","↑"),("south","↓"),("east","→"),("west","←"),("up","▲"),("down","▼")];
    let mut lines = vec![
        format!("  {:^40}", room_title(&center.name)),
        format!("  {}",     dim("  [current location]")),
        String::new(),
    ];
    for (dir, arrow) in &dirs {
        if let Some(exit) = center.exits.get(*dir) {
            if let Some(room) = world.get_room(&exit.to) {
                lines.push(format!("  {} {} → {}", arrow, cyan(dir), room.name));
            }
        }
    }
    lines.join("\r\n") + "\r\n"
}

// ─── Communication ────────────────────────────────────────────────────────────

const MAX_MSG_LEN: usize = 200;
const MAX_EMOTE_LEN: usize = 150;

async fn cmd_say(handle: &GameHandle, player_name: &str, msg: &str) {
    if msg.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("Say what?"), &handle.sessions).await;
        return;
    }
    if msg.len() > MAX_MSG_LEN {
        handle.state.read().await.tell_player(player_name, &error_msg("Message too long (max 200 characters)."), &handle.sessions).await;
        return;
    }

    // Send messages and collect script info under one read lock
    let (room_id, room_script, npc_script_pairs) = {
        let state = handle.state.read().await;
        let room_id = match state.players.get(player_name) { Some(p) => p.room.clone(), None => return };
        let room = state.world.get_room(&room_id);
        let room_script = room.and_then(|r| r.script.clone());
        let npc_script_pairs: Vec<(String, String)> = room.map(|r| {
            r.npcs.iter()
                .filter_map(|id| state.npcs.get(id))
                .filter(|n| n.alive)
                .filter_map(|n| {
                    state.world.get_npc_template(&n.template_id)
                        .and_then(|t| t.script.clone())
                        .map(|s| (s, n.name.clone()))
                })
                .collect()
        }).unwrap_or_default();

        let self_msg = format!("{}: \"{}\"", bold("You say"), white(msg));
        let other_msg = format!("{} says, \"{}\"", crate::color::player_name(player_name), say_text(msg));
        state.tell_player(player_name, &self_msg, &handle.sessions).await;
        state.tell_room_except(&room_id, player_name, &other_msg, &handle.sessions).await;
        let _ = handle.events.send(GameEvent::PlayerSay {
            player: player_name.to_string(), room: room_id.clone(), message: msg.to_string(),
        });
        (room_id, room_script, npc_script_pairs)
    };

    // Call on_say hooks — no lock held during script execution
    let base_ctx: rhai::Map = {
        let mut m = rhai::Map::new();
        m.insert("player".into(), rhai::Dynamic::from(player_name.to_string()));
        m.insert("room".into(), rhai::Dynamic::from(room_id.clone()));
        m.insert("message".into(), rhai::Dynamic::from(msg.to_string()));
        m
    };

    let mut all_actions = vec![];
    if let Some(ref script_name) = room_script {
        all_actions.extend(handle.scripts.call_hook(script_name, "on_say", rhai::Dynamic::from(base_ctx.clone())));
    }
    for (script_name, npc_name) in &npc_script_pairs {
        let mut ctx = base_ctx.clone();
        ctx.insert("npc".into(), rhai::Dynamic::from(npc_name.clone()));
        all_actions.extend(handle.scripts.call_hook(script_name, "on_say", rhai::Dynamic::from(ctx)));
    }
    if !all_actions.is_empty() {
        let mut state = handle.state.write().await;
        state.apply_script_actions(all_actions, player_name, &room_id, &handle.sessions).await;
    }
}

async fn cmd_tell(handle: &GameHandle, player_name: &str, args: &str) {
    let (target, msg) = parse_input(args);
    if target.is_empty() || msg.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("Tell whom, what?"), &handle.sessions).await;
        return;
    }
    if msg.len() > MAX_MSG_LEN {
        handle.state.read().await.tell_player(player_name, &error_msg("Message too long (max 200 characters)."), &handle.sessions).await;
        return;
    }
    let state = handle.state.read().await;
    let target_name = find_player_name(&state.players, target);
    match target_name {
        None => {
            state.tell_player(player_name, &error_msg(&format!("{} is not online.", target)), &handle.sessions).await;
        }
        Some(name) => {
            state.tell_player(&name, &tell_text(&format!("{} tells you: \"{}\"", player_name, msg)), &handle.sessions).await;
            state.tell_player(player_name, &tell_text(&format!("You tell {}: \"{}\"", name, msg)), &handle.sessions).await;
            handle.last_tell.insert(name.to_string(), player_name.to_string());
        }
    }
}

async fn cmd_shout(handle: &GameHandle, player_name: &str, msg: &str) {
    if msg.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("Shout what?"), &handle.sessions).await;
        return;
    }
    if msg.len() > MAX_MSG_LEN {
        handle.state.read().await.tell_player(player_name, &error_msg("Message too long (max 200 characters)."), &handle.sessions).await;
        return;
    }
    let state = handle.state.read().await;
    let player = match state.players.get(player_name) { Some(p) => p.clone(), None => return };
    let area_id = player.room.split(':').next().unwrap_or("nexus").to_string();
    let shout_msg = shout_text(&format!("{} shouts, \"{}\"", player_name, msg));
    state.tell_area(&area_id, &shout_msg, &handle.sessions).await;
    let _ = handle.events.send(GameEvent::PlayerShout {
        player: player_name.to_string(), area: area_id, message: msg.to_string(),
    });
}

async fn cmd_emote(handle: &GameHandle, player_name: &str, action: &str) {
    if action.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("Emote what?"), &handle.sessions).await;
        return;
    }
    if action.len() > MAX_EMOTE_LEN {
        handle.state.read().await.tell_player(player_name, &error_msg("Emote too long (max 150 characters)."), &handle.sessions).await;
        return;
    }
    let state = handle.state.read().await;
    let room_id = match state.players.get(player_name) { Some(p) => p.room.clone(), None => return };
    let msg = italic(&format!("{} {}", player_name, action));
    state.tell_room(&room_id, &msg, &handle.sessions).await;
    let _ = handle.events.send(GameEvent::PlayerEmote {
        player: player_name.to_string(), room: room_id, action: action.to_string(),
    });
}

async fn cmd_chat(handle: &GameHandle, player_name: &str, msg: &str) {
    if msg.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("Chat what?"), &handle.sessions).await;
        return;
    }
    if msg.len() > MAX_MSG_LEN {
        handle.state.read().await.tell_player(player_name, &error_msg("Message too long (max 200 characters)."), &handle.sessions).await;
        return;
    }
    let state = handle.state.read().await;
    let chat_msg = dim(&format!("[OOC] {}: {}", player_name, msg));
    state.tell_all(&chat_msg, &handle.sessions).await;
}

async fn cmd_whisper(handle: &GameHandle, player_name: &str, args: &str) {
    let (target, msg) = parse_input(args);
    let state = handle.state.read().await;
    let player = match state.players.get(player_name) { Some(p) => p.clone(), None => return };

    if let Some(t) = state.players.values().find(|p| p.name.to_lowercase() == target.to_lowercase() && p.room == player.room) {
        let w1 = dim(&format!("You whisper to {}: \"{}\"", t.name, msg));
        let w2 = dim(&format!("{} whispers to you: \"{}\"", player_name, msg));
        state.tell_player(player_name, &w1, &handle.sessions).await;
        state.tell_player(&t.name, &w2, &handle.sessions).await;
    } else {
        state.tell_player(player_name, &error_msg(&format!("{} is not here.", target)), &handle.sessions).await;
    }
}

async fn cmd_reply(handle: &GameHandle, player_name: &str, args: &str) {
    if args.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("Reply what?"), &handle.sessions).await;
        return;
    }
    let sender = match handle.last_tell.get(player_name) {
        Some(r) => r.clone(),
        None => {
            handle.state.read().await.tell_player(player_name, &error_msg("No one to reply to."), &handle.sessions).await;
            return;
        }
    };
    // Reuse tell logic: validate length, find recipient, deliver
    if args.len() > MAX_MSG_LEN {
        handle.state.read().await.tell_player(player_name, &error_msg("Message too long (max 200 characters)."), &handle.sessions).await;
        return;
    }
    let state = handle.state.read().await;
    let target_name = find_player_name(&state.players, &sender).map(|s| s.to_string());
    match target_name {
        None => {
            state.tell_player(player_name, &error_msg(&format!("{} is no longer online.", sender)), &handle.sessions).await;
        }
        Some(name) => {
            state.tell_player(&name, &tell_text(&format!("{} tells you: \"{}\"", player_name, args)), &handle.sessions).await;
            state.tell_player(player_name, &tell_text(&format!("You tell {}: \"{}\"", name, args)), &handle.sessions).await;
            handle.last_tell.insert(name.to_string(), player_name.to_string());
        }
    }
}

// ─── Items ───────────────────────────────────────────────────────────────────

async fn cmd_get(handle: &GameHandle, player_name: &str, args: &str) {
    let kw = args.to_lowercase();
    if kw.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("Get what?"), &handle.sessions).await;
        return;
    }
    let mut state = handle.state.write().await;
    let room_id = match state.players.get(player_name) { Some(p) => p.room.clone(), None => return };
    let items = state.items_on_ground.entry(room_id.clone()).or_default();
    let pos = items.iter().position(|i| i.name.to_lowercase().contains(&kw));
    match pos {
        None => { state.tell_player(player_name, &error_msg("You don't see that here."), &handle.sessions).await; }
        Some(idx) => {
            let item = items.remove(idx);
            let item_n = item.name.clone();
            let tmpl_id = item.template_id.clone();
            state.players.get_mut(player_name).unwrap().inventory.push(item);
            state.tell_player(player_name, &success_msg(&format!("You pick up {}.", item_name(&item_n))), &handle.sessions).await;
            state.tell_room_except(&room_id, player_name, &dim(&format!("{} picks up {}.", player_name, item_n)), &handle.sessions).await;
            let _ = handle.events.send(GameEvent::ItemPickedUp { player: player_name.to_string(), item: item_n.clone(), room: room_id.clone() });
            // on_pickup hook
            let item_script = state.world.get_item_template(&tmpl_id).and_then(|t| t.script.clone());
            if let Some(script_name) = item_script {
                let ctx = rhai::Dynamic::from({
                    let mut m = rhai::Map::new();
                    m.insert("player".into(), rhai::Dynamic::from(player_name.to_string()));
                    m.insert("item".into(), rhai::Dynamic::from(item_n));
                    m.insert("template".into(), rhai::Dynamic::from(tmpl_id));
                    m.insert("room".into(), rhai::Dynamic::from(room_id.clone()));
                    m
                });
                let actions = handle.scripts.call_hook(&script_name, "on_pickup", ctx);
                state.apply_script_actions(actions, player_name, &room_id, &handle.sessions).await;
            }
        }
    }
}

async fn cmd_drop(handle: &GameHandle, player_name: &str, args: &str) {
    let kw = args.to_lowercase();
    if kw.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("Drop what?"), &handle.sessions).await;
        return;
    }
    let mut state = handle.state.write().await;
    let room_id = match state.players.get(player_name) { Some(p) => p.room.clone(), None => return };
    let player = state.players.get_mut(player_name).unwrap();
    match player.take_item(&kw) {
        None => { state.tell_player(player_name, &error_msg("You don't have that."), &handle.sessions).await; }
        Some(item) => {
            let no_drop = state.world.get_item_template(&item.template_id)
                .map(|t| t.flags.no_drop)
                .unwrap_or(false);
            if no_drop {
                state.players.get_mut(player_name).unwrap().inventory.push(item);
                state.tell_player(player_name, &error_msg("You can't drop that."), &handle.sessions).await;
                return;
            }
            let item_n = item.name.clone();
            state.items_on_ground.entry(room_id.clone()).or_default().push(item);
            state.tell_player(player_name, &info_msg(&format!("You drop {}.", item_n)), &handle.sessions).await;
            state.tell_room_except(&room_id, player_name, &dim(&format!("{} drops {}.", player_name, item_n)), &handle.sessions).await;
            let _ = handle.events.send(GameEvent::ItemDropped { player: player_name.to_string(), item: item_n, room: room_id });
        }
    }
}

async fn cmd_put(handle: &GameHandle, player_name: &str, args: &str) {
    // Syntax: put <item> in <container>
    let lower = args.to_lowercase();
    let Some(in_pos) = lower.find(" in ") else {
        handle.state.read().await.tell_player(player_name, &error_msg("Put what in what? (put <item> in <container>)"), &handle.sessions).await;
        return;
    };
    let item_kw = lower[..in_pos].trim();
    let cont_kw = lower[in_pos + 4..].trim();
    if item_kw.is_empty() || cont_kw.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("Put what in what? (put <item> in <container>)"), &handle.sessions).await;
        return;
    }

    let mut state = handle.state.write().await;
    let player = match state.players.get(player_name) { Some(p) => p, None => return };

    // Locate the container by keyword
    let cont_pos = player.inventory.iter().position(|i| {
        i.name.to_lowercase().contains(cont_kw) || i.template_id.to_lowercase().contains(cont_kw)
    });
    let cont_pos = match cont_pos {
        Some(p) => p,
        None => {
            state.tell_player(player_name, &error_msg("You don't have that container."), &handle.sessions).await;
            return;
        }
    };

    // Verify it's actually a Container type
    let (is_container, capacity) = {
        let tmpl_id = &state.players[player_name].inventory[cont_pos].template_id;
        match state.world.get_item_template(tmpl_id).map(|t| &t.item_type) {
            Some(ItemType::Container) => {
                let cap = state.world.get_item_template(tmpl_id)
                    .and_then(|t| t.container_size)
                    .unwrap_or(10);
                (true, cap)
            }
            _ => (false, 0),
        }
    };
    if !is_container {
        state.tell_player(player_name, &error_msg("That's not a container."), &handle.sessions).await;
        return;
    }

    // Make sure item_kw doesn't match the container itself
    {
        let cont = &state.players[player_name].inventory[cont_pos];
        if cont.name.to_lowercase().contains(item_kw) || cont.template_id.to_lowercase().contains(item_kw) {
            state.tell_player(player_name, &error_msg("You can't put a container inside itself."), &handle.sessions).await;
            return;
        }
    }

    // Find item to put in (different index from container)
    let item_pos = state.players[player_name].inventory.iter()
        .enumerate()
        .find(|(idx, i)| *idx != cont_pos && (i.name.to_lowercase().contains(item_kw) || i.template_id.to_lowercase().contains(item_kw)))
        .map(|(idx, _)| idx);
    let item_pos = match item_pos {
        Some(p) => p,
        None => {
            state.tell_player(player_name, &error_msg("You don't have that item."), &handle.sessions).await;
            return;
        }
    };

    // Check capacity
    if state.players[player_name].inventory[cont_pos].contents.len() >= capacity as usize {
        state.tell_player(player_name, &error_msg("The container is full."), &handle.sessions).await;
        return;
    }

    let player = state.players.get_mut(player_name).unwrap();
    let item = player.inventory.remove(item_pos);
    // cont_pos may have shifted if item_pos < cont_pos
    let real_cont_pos = if item_pos < cont_pos { cont_pos - 1 } else { cont_pos };
    let cont = &mut player.inventory[real_cont_pos];
    let item_n = item.name.clone();
    let cont_n = cont.name.clone();
    cont.contents.push(item);
    state.tell_player(player_name, &success_msg(&format!("You put {} in {}.", item_n, cont_n)), &handle.sessions).await;
}

async fn cmd_give(handle: &GameHandle, player_name: &str, args: &str) {
    // "give <item> to <player>"
    let parts: Vec<&str> = args.splitn(3, ' ').collect();
    if parts.len() < 3 || parts[1].to_lowercase() != "to" {
        handle.state.read().await.tell_player(player_name, &error_msg("Give what to whom? (give <item> to <player>)"), &handle.sessions).await;
        return;
    }
    let item_kw = parts[0];
    let target_kw = parts[2];
    let mut state = handle.state.write().await;
    let target_name = find_player_name(&state.players, target_kw).map(|s| s.to_string());
    let player_room = state.players.get(player_name).map(|p| p.room.clone()).unwrap_or_default();
    let target_room = target_name.as_ref().and_then(|n| state.players.get(n)).map(|p| p.room.clone());

    if target_room.as_deref() != Some(&player_room) {
        state.tell_player(player_name, &error_msg("They're not here."), &handle.sessions).await;
        return;
    }

    let item = state.players.get_mut(player_name).and_then(|p| p.take_item(item_kw));
    match (item, target_name) {
        (None, _) => { state.tell_player(player_name, &error_msg("You don't have that."), &handle.sessions).await; }
        (Some(item), None) => { state.players.get_mut(player_name).unwrap().inventory.push(item); }
        (Some(item), Some(tname)) => {
            let item_n = item.name.clone();
            state.tell_player(player_name, &success_msg(&format!("You give {} to {}.", item_n, tname)), &handle.sessions).await;
            state.tell_player(&tname, &success_msg(&format!("{} gives you {}.", player_name, item_n)), &handle.sessions).await;
            state.players.get_mut(&tname).unwrap().inventory.push(item);
        }
    }
}

async fn cmd_wear(handle: &GameHandle, player_name: &str, args: &str) {
    if args.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("Wear/wield what?"), &handle.sessions).await;
        return;
    }
    let mut state = handle.state.write().await;

    // Take item from inventory
    let item = match state.players.get_mut(player_name).and_then(|p| p.take_item(args)) {
        Some(i) => i,
        None => {
            state.tell_player(player_name, &error_msg("You don't have that."), &handle.sessions).await;
            return;
        }
    };

    // Determine slot from template (borrow world independently)
    let slot_result = state.world.get_item_template(&item.template_id).map(|tmpl| {
        match &tmpl.item_type {
            ItemType::Armor { slot, .. } => Ok(format!("{:?}", slot)),
            ItemType::Weapon { .. } => Ok("MainHand".to_string()),
            ItemType::Accessory { slot } => Ok(format!("{:?}", slot)),
            _ => Err(()),
        }
    });

    match slot_result {
        Some(Ok(slot)) => {
            let item_n = item.name.clone();
            let player = state.players.get_mut(player_name).unwrap();
            if let Some(old) = player.equipment.equip(&slot, item) {
                player.inventory.push(old);
            }
            state.tell_player(player_name, &success_msg(&format!("You equip {}.", item_n)), &handle.sessions).await;
        }
        _ => {
            // Return item and show error
            state.players.get_mut(player_name).unwrap().inventory.push(item);
            state.tell_player(player_name, &error_msg("You can't wear that."), &handle.sessions).await;
        }
    }
}

async fn cmd_remove(handle: &GameHandle, player_name: &str, args: &str) {
    if args.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("Remove what?"), &handle.sessions).await;
        return;
    }
    let mut state = handle.state.write().await;
    let player = match state.players.get_mut(player_name) { Some(p) => p, None => return };
    let kw = args.to_lowercase();
    let slot = player.equipment.slots.iter()
        .find(|(_, v)| v.name.to_lowercase().contains(&kw))
        .map(|(k, _)| k.clone());
    if let Some(slot_key) = slot {
        if let Some(item) = player.equipment.unequip(&slot_key) {
            let item_n = item.name.clone();
            player.inventory.push(item);
            state.tell_player(player_name, &success_msg(&format!("You remove {}.", item_n)), &handle.sessions).await;
        }
    } else {
        state.tell_player(player_name, &error_msg("You're not wearing that."), &handle.sessions).await;
    }
}

async fn cmd_use(handle: &GameHandle, player_name: &str, args: &str) {
    if args.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("Use what?"), &handle.sessions).await;
        return;
    }
    let mut state = handle.state.write().await;

    // Clone item and template data before getting mut player
    let item_info = state.players.get(player_name)
        .and_then(|p| p.find_item(args))
        .map(|i| (i.template_id.clone(), i.name.clone()));

    let Some((tmpl_id, item_display_name)) = item_info else {
        state.tell_player(player_name, &error_msg("You don't have that."), &handle.sessions).await;
        return;
    };

    let effect_info = state.world.get_item_template(&tmpl_id).and_then(|t| {
        match &t.item_type {
            ItemType::Consumable => t.consumable.clone().map(|e| (true, e)),
            _ => None,
        }
    });

    match effect_info {
        None => {
            state.tell_player(player_name, &error_msg("You can't use that."), &handle.sessions).await;
        }
        Some((_, eff)) => {
            let player = state.players.get_mut(player_name).unwrap();
            let mut msg = String::new();
            if let Some(heal) = eff.heal_hp {
                let healed = heal.min(player.stats.max_hp - player.stats.hp);
                player.stats.hp += healed;
                msg.push_str(&heal_text(&format!("You restore {} HP.", healed)));
            }
            if let Some(heal_mp) = eff.heal_mp {
                let healed = heal_mp.min(player.stats.max_mp - player.stats.mp);
                player.stats.mp += healed;
                msg.push_str(&heal_text(&format!(" Restored {} MP.", healed)));
            }
            player.take_item(args);
            state.tell_player(player_name, &msg, &handle.sessions).await;
            // on_use hook
            let room_id = state.players.get(player_name).map(|p| p.room.clone()).unwrap_or_default();
            let item_script = state.world.get_item_template(&tmpl_id).and_then(|t| t.script.clone());
            if let Some(script_name) = item_script {
                let ctx = rhai::Dynamic::from({
                    let mut m = rhai::Map::new();
                    m.insert("player".into(), rhai::Dynamic::from(player_name.to_string()));
                    m.insert("item".into(), rhai::Dynamic::from(item_display_name.clone()));
                    m.insert("template".into(), rhai::Dynamic::from(tmpl_id.clone()));
                    m.insert("room".into(), rhai::Dynamic::from(room_id.clone()));
                    m
                });
                let actions = handle.scripts.call_hook(&script_name, "on_use", ctx);
                state.apply_script_actions(actions, player_name, &room_id, &handle.sessions).await;
            }
        }
    }
}

async fn cmd_buy(handle: &GameHandle, player_name: &str, args: &str) {
    if args.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("Buy what? (use 'list' to see what's for sale)"), &handle.sessions).await;
        return;
    }
    let mut state = handle.state.write().await;
    let room_id = match state.players.get(player_name) { Some(p) => p.room.clone(), None => return };
    let room = match state.world.get_room(&room_id) { Some(r) => r.clone(), None => return };
    if !room.flags.shop {
        state.tell_player(player_name, &error_msg("There's no shop here."), &handle.sessions).await;
        return;
    }
    // Find a merchant NPC in the room
    let merchant_id = room.npcs.iter()
        .find(|id| {
            state.npcs.get(*id)
                .and_then(|n| state.world.get_npc_template(&n.template_id))
                .map(|t| t.behavior == NpcBehavior::Merchant)
                .unwrap_or(false)
        })
        .cloned();
    let merchant_id = match merchant_id {
        Some(id) => id, None => { state.tell_player(player_name, &error_msg("No merchant here."), &handle.sessions).await; return; }
    };
    let shop_items = state.npcs.get(&merchant_id)
        .and_then(|n| state.world.get_npc_template(&n.template_id))
        .map(|t| t.shop_items.clone())
        .unwrap_or_default();
    let kw = args.to_lowercase();
    let shop_item = shop_items.iter()
        .find(|si| si.template.to_lowercase().contains(&kw) ||
            state.world.get_item_template(&si.template).map(|t| t.name.to_lowercase().contains(&kw)).unwrap_or(false));
    match shop_item {
        None => { state.tell_player(player_name, &error_msg("They don't sell that."), &handle.sessions).await; }
        Some(si) => {
            let price = si.price;
            let tmpl_id = si.template.clone();
            // Resolve item name before mutable borrow of players
            let resolved_name = state.world.get_item_template(&tmpl_id)
                .map(|t| t.name.clone())
                .unwrap_or_else(|| tmpl_id.clone());
            let buy_msg = {
                let player = state.players.get_mut(player_name).unwrap();
                if player.coins < price {
                    error_msg(&format!("You can't afford that ({} coins needed).", price))
                } else {
                    player.coins -= price;
                    let item = ItemInstance::new(&tmpl_id, &resolved_name);
                    player.inventory.push(item);
                    success_msg(&format!("You buy {} for {} coins.", resolved_name, price))
                }
            };
            state.tell_player(player_name, &buy_msg, &handle.sessions).await;
        }
    }
}

async fn cmd_sell(handle: &GameHandle, player_name: &str, args: &str) {
    if args.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("Sell what?"), &handle.sessions).await;
        return;
    }
    let mut state = handle.state.write().await;
    let room_id = match state.players.get(player_name) { Some(p) => p.room.clone(), None => return };
    let room = match state.world.get_room(&room_id) { Some(r) => r.clone(), None => return };
    if !room.flags.shop {
        state.tell_player(player_name, &error_msg("There's no shop here."), &handle.sessions).await;
        return;
    }
    let player = state.players.get_mut(player_name).unwrap();
    match player.take_item(args) {
        None => { state.tell_player(player_name, &error_msg("You don't have that."), &handle.sessions).await; }
        Some(item) => {
            let sell_price = state.world.get_item_template(&item.template_id)
                .map(|t| t.value / 2).unwrap_or(1).max(1);
            let item_n = item.name.clone();
            let p = state.players.get_mut(player_name).unwrap();
            p.coins = p.coins.saturating_add(sell_price);
            state.tell_player(player_name, &success_msg(&format!("You sell {} for {} coins.", item_n, sell_price)), &handle.sessions).await;
        }
    }
}

async fn cmd_list(handle: &GameHandle, player_name: &str) {
    let state = handle.state.read().await;
    let room_id = match state.players.get(player_name) { Some(p) => p.room.clone(), None => return };
    let room = match state.world.get_room(&room_id) { Some(r) => r.clone(), None => return };
    if !room.flags.shop {
        state.tell_player(player_name, &error_msg("There's no shop here."), &handle.sessions).await;
        return;
    }
    let merchant_id = room.npcs.iter()
        .find(|id| state.npcs.get(*id)
            .and_then(|n| state.world.get_npc_template(&n.template_id))
            .map(|t| t.behavior == NpcBehavior::Merchant).unwrap_or(false))
        .cloned();
    let shop_items = merchant_id.as_ref()
        .and_then(|id| state.npcs.get(id))
        .and_then(|n| state.world.get_npc_template(&n.template_id))
        .map(|t| t.shop_items.clone())
        .unwrap_or_default();
    if shop_items.is_empty() {
        state.tell_player(player_name, &info_msg("Nothing for sale here."), &handle.sessions).await;
        return;
    }
    let mut out = format!("\r\n{}\r\n", bold("Items for Sale:"));
    for si in &shop_items {
        let name = state.world.get_item_template(&si.template).map(|t| t.name.as_str()).unwrap_or(&si.template);
        out.push_str(&format!("  {:30} {} coins\r\n", item_name(name), yellow(&si.price.to_string())));
    }
    state.tell_player(player_name, &out, &handle.sessions).await;
}

async fn cmd_craft(handle: &GameHandle, player_name: &str, args: &str) {
    // Syntax: craft list  OR  craft <item1> with <item2>
    let args_lower = args.to_lowercase();

    if args_lower.is_empty() || args_lower == "list" {
        let state = handle.state.read().await;
        let player = match state.players.get(player_name) { Some(p) => p, None => return };
        if player.known_recipes.is_empty() {
            state.tell_player(player_name, &info_msg("You don't know any recipes yet. Experiment to discover them!"), &handle.sessions).await;
        } else {
            let mut out = format!("\r\n{}\r\n", bold("Known Recipes:"));
            for r in &player.known_recipes { out.push_str(&format!("  {}\r\n", r)); }
            state.tell_player(player_name, &out, &handle.sessions).await;
        }
        return;
    }

    // Parse "X with Y"
    let Some(with_pos) = args_lower.find(" with ") else {
        handle.state.read().await.tell_player(player_name, &error_msg("Craft what with what? (craft <item> with <item>)"), &handle.sessions).await;
        return;
    };
    let kw1 = args_lower[..with_pos].trim();
    let kw2 = args_lower[with_pos + 6..].trim();
    if kw1.is_empty() || kw2.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("Craft what with what? (craft <item> with <item>)"), &handle.sessions).await;
        return;
    }

    let mut state = handle.state.write().await;
    let player = match state.players.get(player_name) { Some(p) => p, None => return };

    // Find both ingredient items in player's inventory
    let pos1 = player.inventory.iter().position(|i| {
        i.name.to_lowercase().contains(kw1) || i.template_id.to_lowercase().contains(kw1)
    });
    let pos1 = match pos1 {
        Some(p) => p,
        None => {
            state.tell_player(player_name, &error_msg(&format!("You don't have '{}'.", kw1)), &handle.sessions).await;
            return;
        }
    };
    let pos2 = player.inventory.iter()
        .enumerate()
        .find(|(idx, i)| *idx != pos1 && (i.name.to_lowercase().contains(kw2) || i.template_id.to_lowercase().contains(kw2)))
        .map(|(idx, _)| idx);
    let pos2 = match pos2 {
        Some(p) => p,
        None => {
            state.tell_player(player_name, &error_msg(&format!("You don't have '{}'.", kw2)), &handle.sessions).await;
            return;
        }
    };

    let ing_id1 = player.inventory[pos1].template_id.clone();
    let ing_id2 = player.inventory[pos2].template_id.clone();

    // Search all item templates for a recipe whose ingredients are these two items
    let recipe = state.world.item_templates.values()
        .flat_map(|tmpl| tmpl.craft_recipes.iter().map(move |r| (tmpl.id.clone(), tmpl.name.clone(), r.clone())))
        .find(|(_, _, r)| {
            let ids: std::collections::HashSet<&str> = r.ingredients.iter().map(|s| s.as_str()).collect();
            ids.contains(ing_id1.as_str()) && ids.contains(ing_id2.as_str()) && r.ingredients.len() == 2
        });

    // Also search area-local templates
    let recipe = if recipe.is_none() {
        state.world.areas.values()
            .flat_map(|a| a.item_templates.values())
            .flat_map(|tmpl| tmpl.craft_recipes.iter().map(move |r| (tmpl.id.clone(), tmpl.name.clone(), r.clone())))
            .find(|(_, _, r)| {
                let ids: std::collections::HashSet<&str> = r.ingredients.iter().map(|s| s.as_str()).collect();
                ids.contains(ing_id1.as_str()) && ids.contains(ing_id2.as_str()) && r.ingredients.len() == 2
            })
    } else {
        recipe
    };

    let (result_id, result_item_name, recipe) = match recipe {
        Some((id, name, r)) => (id, name, r),
        None => {
            state.tell_player(player_name, &info_msg("Nothing happens. Those items don't seem to combine into anything useful."), &handle.sessions).await;
            return;
        }
    };

    // Check skill requirement
    if let (Some(skill_name), Some(req_level)) = (&recipe.skill_required, recipe.skill_level) {
        let player_skill_level = state.players[player_name].skills.get(skill_name).map(|s| s.level).unwrap_or(0);
        if player_skill_level < req_level {
            state.tell_player(player_name, &error_msg(&format!("You need {} level {} to craft that.", skill_name, req_level)), &handle.sessions).await;
            return;
        }
    }

    // Consume ingredients (remove from inventory)
    let player = state.players.get_mut(player_name).unwrap();
    let (lo, hi) = if pos1 < pos2 { (pos1, pos2) } else { (pos2, pos1) };
    let item_n1 = player.inventory[lo].name.clone();
    let item_n2 = player.inventory[hi].name.clone();
    player.inventory.remove(hi);
    player.inventory.remove(lo);

    // Create result item
    let result_tmpl = state.world.get_item_template(&result_id).cloned();
    let result_name = result_tmpl.as_ref().map(|t| t.name.clone()).unwrap_or(result_item_name);
    let result_instance = ItemInstance::new(&result_id, &result_name);
    let result_display = result_instance.name.clone();

    // Learn recipe if new
    let recipe_label = format!("{} + {} → {}", item_n1, item_n2, result_display);
    let player = state.players.get_mut(player_name).unwrap();
    if !player.known_recipes.contains(&recipe_label) {
        player.known_recipes.push(recipe_label.clone());
    }
    player.inventory.push(result_instance);
    state.tell_player(player_name, &success_msg(&format!("You combine {} and {} to create {}!", item_n1, item_n2, result_display)), &handle.sessions).await;
}

// ─── Talk / Dialogue ──────────────────────────────────────────────────────────

async fn cmd_talk(handle: &GameHandle, player_name: &str, args: &str) {
    let args = args.trim();
    if args.is_empty() {
        handle.state.read().await.tell_player(
            player_name,
            &error_msg("Talk to whom? (talk <npc> [topic])"),
            &handle.sessions,
        ).await;
        return;
    }

    // Split "herald hello" → npc_kw="herald", topic="hello"
    // or "herald" → npc_kw="herald", topic=""
    let (npc_kw, topic) = parse_input(args);
    let npc_kw = npc_kw.to_lowercase();
    let topic  = topic.to_lowercase();

    let state = handle.state.read().await;
    let player = match state.players.get(player_name) { Some(p) => p, None => return };
    let room   = match state.world.get_room(&player.room)  { Some(r) => r, None => return };

    // Find matching NPC in current room
    let npc = room.npcs.iter()
        .find_map(|id| state.npcs.get(id).filter(|n| n.alive && npc_matches(n, &state.world, &npc_kw)));

    let npc = match npc {
        Some(n) => n,
        None => {
            state.tell_player(player_name, &error_msg("You don't see that here."), &handle.sessions).await;
            return;
        }
    };

    let tmpl = match state.world.get_npc_template(&npc.template_id) {
        Some(t) => t,
        None => return,
    };

    if tmpl.dialogue.is_empty() {
        state.tell_player(
            player_name,
            &info_msg(&format!("{} doesn't seem interested in conversation.", npc.name)),
            &handle.sessions,
        ).await;
        return;
    }

    if topic.is_empty() {
        // No topic: show the "hello" response if one exists, then list available topics.
        let greeting = tmpl.dialogue.iter().find(|d| d.trigger == "hello");
        if let Some(g) = greeting {
            let msg = format!(
                "\r\n{} says: \"{}\"\r\n",
                cyan(&npc.name), white(&g.response)
            );
            state.tell_player(player_name, &msg, &handle.sessions).await;
        }
        let topics: Vec<&str> = tmpl.dialogue.iter().map(|d| d.trigger.as_str()).collect();
        state.tell_player(
            player_name,
            &dim(&format!("(You can ask about: {})", topics.join(", "))),
            &handle.sessions,
        ).await;
        return;
    }

    // Match dialogue by trigger substring
    let line = tmpl.dialogue.iter().find(|d| d.trigger.to_lowercase().contains(&topic));
    match line {
        None => {
            state.tell_player(
                player_name,
                &info_msg(&format!("{} doesn't have anything to say about that.", npc.name)),
                &handle.sessions,
            ).await;
        }
        Some(line) => {
            let msg = format!(
                "\r\n{} says: \"{}\"\r\n",
                cyan(&npc.name), white(&line.response)
            );
            state.tell_player(player_name, &msg, &handle.sessions).await;
            state.tell_room_except(&player.room, player_name,
                &dim(&format!("{} talks with {}.", player_name, npc.name)),
                &handle.sessions).await;

            // Fire optional script hook attached to this dialogue line
            let hook = line.script_hook.clone();
            let script_name = tmpl.script.clone();
            drop(state);
            if let (Some(hook_name), Some(script)) = (hook, script_name) {
                let room_id = {
                    let s = handle.state.read().await;
                    s.players.get(player_name).map(|p| p.room.clone()).unwrap_or_default()
                };
                let ctx = rhai::Dynamic::from({
                    let mut m = rhai::Map::new();
                    m.insert("player".into(), rhai::Dynamic::from(player_name.to_string()));
                    m.insert("npc".into(), rhai::Dynamic::from(npc_kw.clone()));
                    m.insert("topic".into(), rhai::Dynamic::from(topic.clone()));
                    m.insert("room".into(), rhai::Dynamic::from(room_id.clone()));
                    m
                });
                let actions = handle.scripts.call_hook(&script, &hook_name, ctx);
                if !actions.is_empty() {
                    let mut s = handle.state.write().await;
                    s.apply_script_actions(actions, player_name, &room_id, &handle.sessions).await;
                }
            }
        }
    }
}

// ─── Combat ───────────────────────────────────────────────────────────────────

async fn cmd_attack(handle: &GameHandle, player_name: &str, target: &str) {
    if target.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("Attack what?"), &handle.sessions).await;
        return;
    }
    let mut state = handle.state.write().await;
    let player = match state.players.get(player_name) { Some(p) => p.clone(), None => return };

    if player.is_in_combat() {
        state.tell_player(player_name, &error_msg("You're already in combat!"), &handle.sessions).await;
        return;
    }
    if !player.stats.is_alive() {
        state.tell_player(player_name, &error_msg("You're dead!"), &handle.sessions).await;
        return;
    }

    let room = match state.world.get_room(&player.room) { Some(r) => r.clone(), None => return };
    if room.flags.safe {
        state.tell_player(player_name, &error_msg("This is a safe area — no fighting allowed here."), &handle.sessions).await;
        return;
    }

    let kw = target.to_lowercase();
    // Find NPC in room
    let npc_id = room.npcs.iter()
        .find(|id| state.npcs.get(*id)
            .map(|n| n.alive && npc_matches(n, &state.world, &kw))
            .unwrap_or(false))
        .cloned();

    match npc_id {
        None => {
            state.tell_player(player_name, &error_msg("You don't see that here."), &handle.sessions).await;
        }
        Some(npc_id) => {
            let npc_name = state.npcs.get(&npc_id).map(|n| n.name.clone()).unwrap_or_default();
            let npc_script = {
                let tid = state.npcs.get(&npc_id).map(|n| n.template_id.clone()).unwrap_or_default();
                state.world.get_npc_template(&tid).and_then(|t| t.script.clone())
            };
            state.players.get_mut(player_name).unwrap().in_combat_with = Some(npc_id.clone());
            state.npcs.get_mut(&npc_id).unwrap().in_combat_with = Some(player_name.to_string());
            state.tell_player(player_name, &bright_red(&format!("You engage {} in combat!", npc_name)), &handle.sessions).await;
            state.tell_room_except(&room.id, player_name,
                &dim(&format!("{} attacks {}!", player_name, npc_name)), &handle.sessions).await;
            let _ = handle.events.send(GameEvent::CombatStart {
                attacker: player_name.to_string(), defender: npc_name.clone(), room: room.id.clone(),
            });
            // on_attack hook
            if let Some(script_name) = npc_script {
                let ctx = rhai::Dynamic::from({
                    let mut m = rhai::Map::new();
                    m.insert("player".into(), rhai::Dynamic::from(player_name.to_string()));
                    m.insert("npc".into(), rhai::Dynamic::from(npc_id));
                    m.insert("npc_name".into(), rhai::Dynamic::from(npc_name));
                    m.insert("room".into(), rhai::Dynamic::from(room.id.clone()));
                    m
                });
                let actions = handle.scripts.call_hook(&script_name, "on_attack", ctx);
                state.apply_script_actions(actions, player_name, &room.id, &handle.sessions).await;
            }
        }
    }
}

async fn cmd_flee(handle: &GameHandle, player_name: &str) {
    let mut state = handle.state.write().await;
    let player = match state.players.get(player_name) { Some(p) => p.clone(), None => return };
    if !player.is_in_combat() {
        state.tell_player(player_name, &error_msg("You're not in combat."), &handle.sessions).await;
        return;
    }

    let flee_chance = state.config.combat.flee_success_chance;
    if crate::combat::attempt_flee(&player.stats, flee_chance) {
        // Clear combat
        if let Some(npc_id) = &player.in_combat_with.clone() {
            if let Some(npc) = state.npcs.get_mut(npc_id) {
                npc.in_combat_with = None;
            }
        }
        state.players.get_mut(player_name).unwrap().in_combat_with = None;

        // Move to a random exit
        let room = state.world.get_room(&player.room).cloned();
        if let Some(room) = room {
            let exits: Vec<_> = room.exits.values().collect();
            if !exits.is_empty() {
                let exit = &exits[rand::thread_rng().gen_range(0..exits.len())];
                let dest = exit.to.clone();
                state.players.get_mut(player_name).unwrap().room = dest;
            }
        }
        state.tell_player(player_name, &yellow("You panic and flee!"), &handle.sessions).await;
        drop(state);
        render_room(handle, player_name).await;
    } else {
        state.tell_player(player_name, &error_msg("You fail to flee!"), &handle.sessions).await;
    }
}

async fn cmd_consider(handle: &GameHandle, player_name: &str, target: &str) {
    let state = handle.state.read().await;
    let player = match state.players.get(player_name) { Some(p) => p, None => return };
    let room = match state.world.get_room(&player.room) { Some(r) => r, None => return };
    let kw = target.to_lowercase();
    let npc = room.npcs.iter()
        .find_map(|id| state.npcs.get(id).filter(|n| npc_matches(n, &state.world, &kw)));
    match npc {
        None => { state.tell_player(player_name, &error_msg("You don't see that here."), &handle.sessions).await; }
        Some(npc) => {
            let tmpl = state.world.get_npc_template(&npc.template_id);
            let npc_level = tmpl.map(|t| t.level).unwrap_or(1);
            let diff = npc_level as i32 - player.level as i32;
            let assessment = match diff {
                -5..=-3 => green("This is an easy target."),
                -2..=-1 => bright_green("You could take them easily."),
                0 => yellow("An even match."),
                1..=2 => yellow("Challenging but doable."),
                3..=5 => red("This will be a tough fight."),
                _ if diff > 5 => bright_red("You would be destroyed!"),
                _ => bright_green("Trivial."),
            };
            state.tell_player(player_name, &format!("You consider {}: {}", npc.name, assessment), &handle.sessions).await;
        }
    }
}

// ─── Character ────────────────────────────────────────────────────────────────

async fn cmd_title(handle: &GameHandle, player_name: &str, args: &str) {
    let mut state = handle.state.write().await;
    let player = match state.players.get_mut(player_name) { Some(p) => p, None => return };
    if args.is_empty() {
        player.title = String::new();
        state.tell_player(player_name, &info_msg("Title cleared."), &handle.sessions).await;
    } else if args.len() > 40 {
        state.tell_player(player_name, &error_msg("Title too long (max 40 chars)."), &handle.sessions).await;
    } else {
        player.title = args.to_string();
        state.tell_player(player_name, &success_msg(&format!("Title set to: {}", args)), &handle.sessions).await;
    }
}

async fn cmd_describe(handle: &GameHandle, player_name: &str, args: &str) {
    let mut state = handle.state.write().await;
    let player = match state.players.get_mut(player_name) { Some(p) => p, None => return };
    if args.is_empty() {
        state.tell_player(player_name, &error_msg("Describe yourself how?"), &handle.sessions).await;
        return;
    }
    if args.len() > 200 {
        state.tell_player(player_name, &error_msg("Description too long (max 200 chars)."), &handle.sessions).await;
        return;
    }
    player.description = args.to_string();
    state.tell_player(player_name, &success_msg("Description updated."), &handle.sessions).await;
}

async fn cmd_alias(handle: &GameHandle, player_name: &str, args: &str) {
    let (alias_name, expansion) = parse_input(args);
    let mut state = handle.state.write().await;
    let player = match state.players.get_mut(player_name) { Some(p) => p, None => return };
    if alias_name.is_empty() {
        if player.aliases.is_empty() {
            state.tell_player(player_name, &info_msg("No aliases defined."), &handle.sessions).await;
        } else {
            let mut out = format!("\r\n{}\r\n", bold("Aliases:"));
            for (k, v) in &player.aliases { out.push_str(&format!("  {:12} → {}\r\n", k, v)); }
            state.tell_player(player_name, &out, &handle.sessions).await;
        }
    } else if expansion.is_empty() {
        let val = player.aliases.get(alias_name).cloned();
        match val {
            None => { state.tell_player(player_name, &error_msg(&format!("No alias '{}'.", alias_name)), &handle.sessions).await; }
            Some(v) => { state.tell_player(player_name, &format!("{} → {}", alias_name, v), &handle.sessions).await; }
        }
    } else {
        if player.aliases.len() >= 30 && !player.aliases.contains_key(alias_name) {
            state.tell_player(player_name, &error_msg("Alias limit reached (max 30)."), &handle.sessions).await;
            return;
        }
        if alias_name.len() > 32 || expansion.len() > 100 {
            state.tell_player(player_name, &error_msg("Alias name too long (max 32) or expansion too long (max 100)."), &handle.sessions).await;
            return;
        }
        player.aliases.insert(alias_name.to_string(), expansion.to_string());
        state.tell_player(player_name, &success_msg(&format!("Alias set: {} → {}", alias_name, expansion)), &handle.sessions).await;
    }
}

async fn cmd_unalias(handle: &GameHandle, player_name: &str, args: &str) {
    let mut state = handle.state.write().await;
    let player = match state.players.get_mut(player_name) { Some(p) => p, None => return };
    if player.aliases.remove(args).is_some() {
        state.tell_player(player_name, &success_msg(&format!("Alias '{}' removed.", args)), &handle.sessions).await;
    } else {
        state.tell_player(player_name, &error_msg(&format!("No alias '{}'.", args)), &handle.sessions).await;
    }
}

async fn cmd_write(handle: &GameHandle, player_name: &str, args: &str) {
    // Syntax: write <item> <text>
    let (item_kw, text) = parse_input(args);
    if item_kw.is_empty() || text.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("Write what? (write <item> <text>)"), &handle.sessions).await;
        return;
    }
    const MAX_WRITE_LEN: usize = 500;
    if text.len() > MAX_WRITE_LEN {
        handle.state.read().await.tell_player(player_name, &error_msg("Text too long (max 500 characters)."), &handle.sessions).await;
        return;
    }
    let mut state = handle.state.write().await;
    let player = match state.players.get(player_name) { Some(p) => p, None => return };

    // Confirm the item is a Book type before mutating
    let item_pos = player.inventory.iter().position(|i| {
        i.name.to_lowercase().contains(&item_kw.to_lowercase()) ||
        i.template_id.to_lowercase().contains(&item_kw.to_lowercase())
    });
    let item_pos = match item_pos {
        Some(p) => p,
        None => {
            state.tell_player(player_name, &error_msg("You don't have that."), &handle.sessions).await;
            return;
        }
    };

    let is_book = {
        let tmpl_id = &state.players[player_name].inventory[item_pos].template_id;
        matches!(state.world.get_item_template(tmpl_id).map(|t| &t.item_type), Some(ItemType::Book { .. }))
    };
    if !is_book {
        state.tell_player(player_name, &error_msg("You can only write in a book."), &handle.sessions).await;
        return;
    }

    let player = state.players.get_mut(player_name).unwrap();
    let item = &mut player.inventory[item_pos];
    item.custom_desc = Some(text.to_string());
    let item_n = item.name.clone();
    state.tell_player(player_name, &success_msg(&format!("You write in {}.", item_n)), &handle.sessions).await;
}

async fn cmd_read(handle: &GameHandle, player_name: &str, args: &str) {
    let state = handle.state.read().await;
    let player = match state.players.get(player_name) { Some(p) => p, None => return };
    let kw = args.to_lowercase();
    if let Some(item) = player.find_item(&kw) {
        if let Some(tmpl) = state.world.get_item_template(&item.template_id) {
            match &tmpl.item_type {
                ItemType::Book { content } => {
                    let text = item.custom_desc.as_deref().unwrap_or(content.as_str());
                    state.tell_player(player_name, &format!("\r\n--- {} ---\r\n{}\r\n---\r\n", bold(&item.name), text), &handle.sessions).await;
                }
                _ => { state.tell_player(player_name, &error_msg("That's not readable."), &handle.sessions).await; }
            }
        }
    } else {
        state.tell_player(player_name, &error_msg("You don't have that."), &handle.sessions).await;
    }
}

async fn cmd_save(handle: &GameHandle, player_name: &str) {
    let state = handle.state.read().await;
    if let Some(player) = state.players.get(player_name) {
        match player.save(&state.config.game.players_path) {
            Ok(_) => state.tell_player(player_name, &success_msg("Character saved."), &handle.sessions).await,
            Err(e) => state.tell_player(player_name, &error_msg(&format!("Save failed: {}", e)), &handle.sessions).await,
        }
    }
}

async fn cmd_quit(handle: &GameHandle, player_name: &str) {
    let state = handle.state.read().await;
    // Save player first
    if let Some(player) = state.players.get(player_name) {
        let _ = player.save(&state.config.game.players_path);
    }
    state.tell_player(player_name, &yellow("\r\nFarewell, adventurer. May your path be clear.\r\n"), &handle.sessions).await;
    // The session will detect channel close and clean up
    handle.sessions.remove(player_name);
    drop(state);
    handle.state.write().await.players.remove(player_name);
}

// ─── Help ─────────────────────────────────────────────────────────────────────

async fn cmd_help(handle: &GameHandle, player_name: &str, topic: &str) {
    let help_text = match topic.to_lowercase().as_str() {
        "commands" | "cmd" | "" => HELP_COMMANDS,
        "combat" => HELP_COMBAT,
        "skills" => HELP_SKILLS,
        "crafting" => HELP_CRAFTING,
        "scripting" => HELP_SCRIPTING,
        "factions" | "rep" => HELP_FACTIONS,
        "time" | "weather" => HELP_TIME,
        _ => "Unknown help topic. Try: commands, combat, skills, crafting, factions, time",
    };
    handle.state.read().await.tell_player(player_name, &format!("\r\n{}", help_text), &handle.sessions).await;
}

const HELP_COMMANDS: &str = "\
\x1b[1mNeoMUD Command Reference\x1b[0m\r\n\
\x1b[2m────────────────────────────────────────────────────────────\x1b[0m\r\n\
Navigation:  n/s/e/w/u/d  ne/nw/se/sw  look  map\r\n\
Information: score  inventory(i)  equipment(eq)  skills  who  time  weather  rep\r\n\
Combat:      kill <target>  flee  consider <target>\r\n\
Items:       get <item>  drop <item>  wear/wield <item>  remove <item>\r\n\
             use <item>  give <item> to <player>  buy/sell/list\r\n\
Chat:        say <msg>  tell <player> <msg>  shout <msg>  emote <action>\r\n\
             chat <msg> (OOC)  whisper <player> <msg>\r\n\
NPC:         talk <npc> [topic]  (talk without a topic lists available topics)\r\n\
Character:   title <text>  describe <text>  alias  save  quit\r\n\
\r\nType 'help <topic>' for more: combat, skills, crafting, factions, time\r\n";

const HELP_COMBAT: &str = "\
\x1b[1mCombat System\x1b[0m\r\n\
\x1b[2m────────────────────────────────────────────────────────────\x1b[0m\r\n\
Combat is tick-based. Each round, you and your opponent exchange blows.\r\n\
  kill <target>  - Engage a target in combat\r\n\
  flee           - Attempt to escape (chance based on DEX)\r\n\
  consider <npc> - Gauge how dangerous an enemy is\r\n\
Status effects (poison, bleed, burn) tick each round.\r\n\
Critical hits (5% chance) deal double damage.\r\n";

const HELP_SKILLS: &str = "\
\x1b[1mSkill System\x1b[0m\r\n\
\x1b[2m────────────────────────────────────────────────────────────\x1b[0m\r\n\
Skills improve through USE, not just leveling. The more you do something,\r\n\
the better you get at it. Mastery tiers: Novice → Apprentice → Journeyman\r\n\
→ Expert → Master → Grandmaster\r\n\
Use 'skills' to see your current abilities and progress.\r\n";

const HELP_CRAFTING: &str = "\
\x1b[1mCrafting System\x1b[0m\r\n\
\x1b[2m────────────────────────────────────────────────────────────\x1b[0m\r\n\
Combine items to discover recipes! Many combinations are unknown until tried.\r\n\
  craft list           - Show known recipes\r\n\
  craft <item> with <item> - Attempt to combine items\r\n\
Some recipes require specific skill levels to succeed.\r\n";

const HELP_SCRIPTING: &str = "\
\x1b[1mScripting (Admin)\x1b[0m\r\n\
\x1b[2m────────────────────────────────────────────────────────────\x1b[0m\r\n\
Rooms, NPCs, and items can have Rhai scripts in world/scripts/.\r\n\
Hooks: on_enter, on_exit, on_command, on_say, on_tick, on_attack, describe\r\n\
Scripts return arrays of action maps.\r\n\
  reload scripts  - Hot-reload all scripts without restart\r\n";

const HELP_FACTIONS: &str = "\
\x1b[1mFaction & Reputation\x1b[0m\r\n\
\x1b[2m────────────────────────────────────────────────────────────\x1b[0m\r\n\
Your actions affect your standing with various factions (-1000 to +1000).\r\n\
Standing tiers: Hated → Hostile → Unfriendly → Neutral → Friendly\r\n\
               → Honored → Revered → Exalted\r\n\
Better standing = lower shop prices, quest access, NPC attitudes.\r\n\
  rep  - Show current faction standings\r\n";

const HELP_TIME: &str = "\
\x1b[1mTime & Weather\x1b[0m\r\n\
\x1b[2m────────────────────────────────────────────────────────────\x1b[0m\r\n\
The world has a 24-hour day/night cycle and seasonal weather.\r\n\
Time is accelerated: ~1 game day passes per real-world hour.\r\n\
Time of day affects:\r\n\
  - Room descriptions (outside rooms have day/night variants)\r\n\
  - NPC behavior (some only appear at night)\r\n\
  - Combat visibility (fog, rain reduce hit chance)\r\n\
  - Events and announcements\r\n";

// ─── Admin Commands ───────────────────────────────────────────────────────────

async fn require_admin(handle: &GameHandle, player_name: &str) -> bool {
    let state = handle.state.read().await;
    let is_admin = state.players.get(player_name).map(|p| p.is_admin).unwrap_or(false);
    if !is_admin {
        state.tell_player(player_name, &error_msg("Permission denied."), &handle.sessions).await;
    }
    is_admin
}

async fn cmd_admin_goto(handle: &GameHandle, player_name: &str, args: &str) {
    if !require_admin(handle, player_name).await { return; }
    if args.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("goto <room_id>"), &handle.sessions).await;
        return;
    }
    let mut state = handle.state.write().await;
    if state.world.get_room(args).is_none() {
        state.tell_player(player_name, &error_msg("Room not found."), &handle.sessions).await;
        return;
    }
    let old_room = state.players.get(player_name).map(|p| p.room.clone()).unwrap_or_default();
    state.tell_room_except(&old_room, player_name, &dim(&format!("{} vanishes.", player_name)), &handle.sessions).await;
    state.players.get_mut(player_name).unwrap().room = args.to_string();
    state.tell_player(player_name, &admin_msg(&format!("Teleported to {}.", args)), &handle.sessions).await;
    drop(state);
    render_room(handle, player_name).await;
}

async fn cmd_admin_spawn(handle: &GameHandle, player_name: &str, args: &str) {
    if !require_admin(handle, player_name).await { return; }
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    if parts.len() < 2 {
        handle.state.read().await.tell_player(player_name, &error_msg("spawn npc|item <template_id>"), &handle.sessions).await;
        return;
    }
    let mut state = handle.state.write().await;
    let room_id = state.players.get(player_name).map(|p| p.room.clone()).unwrap_or_default();
    match parts[0] {
        "npc" => {
            match state.spawn_npc(parts[1], &room_id) {
                Some(id) => state.tell_player(player_name, &admin_msg(&format!("Spawned NPC {} ({})", parts[1], id)), &handle.sessions).await,
                None => state.tell_player(player_name, &error_msg("NPC template not found."), &handle.sessions).await,
            }
        }
        "item" => {
            if let Some(tmpl) = state.world.get_item_template(parts[1]) {
                let item = ItemInstance::new(parts[1], &tmpl.name.clone());
                let item_n = item.name.clone();
                state.items_on_ground.entry(room_id).or_default().push(item);
                state.tell_player(player_name, &admin_msg(&format!("Spawned item: {}", item_n)), &handle.sessions).await;
            } else {
                state.tell_player(player_name, &error_msg("Item template not found."), &handle.sessions).await;
            }
        }
        _ => { state.tell_player(player_name, &error_msg("spawn npc|item <id>"), &handle.sessions).await; }
    }
}

async fn cmd_admin_reload(handle: &GameHandle, player_name: &str, args: &str) {
    if !require_admin(handle, player_name).await { return; }
    match args {
        "scripts" => {
            handle.scripts.reload_all();
            handle.state.read().await.tell_player(player_name, &admin_msg("Scripts reloaded."), &handle.sessions).await;
        }
        "world" => {
            let mut state = handle.state.write().await;
            match crate::world::World::load(&state.config.game.world_path) {
                Ok(new_world) => {
                    state.world = new_world;
                    state.tell_player(player_name, &admin_msg("World reloaded."), &handle.sessions).await;
                }
                Err(e) => { state.tell_player(player_name, &error_msg(&format!("Reload failed: {}", e)), &handle.sessions).await; }
            }
        }
        _ => { handle.state.read().await.tell_player(player_name, &error_msg("reload scripts|world"), &handle.sessions).await; }
    }
}

async fn cmd_admin_teleport(handle: &GameHandle, player_name: &str, args: &str) {
    if !require_admin(handle, player_name).await { return; }
    let (target, room_id) = parse_input(args);
    if target.is_empty() || room_id.is_empty() {
        handle.state.read().await.tell_player(player_name, &error_msg("teleport <player> <room_id>"), &handle.sessions).await;
        return;
    }
    let mut state = handle.state.write().await;
    if state.world.get_room(room_id).is_none() {
        state.tell_player(player_name, &error_msg("Room not found."), &handle.sessions).await;
        return;
    }
    let target_name = find_player_name(&state.players, target).map(|s| s.to_string());
    match target_name {
        None => { state.tell_player(player_name, &error_msg("Player not online."), &handle.sessions).await; }
        Some(tname) => {
            state.players.get_mut(&tname).unwrap().room = room_id.to_string();
            state.tell_player(&tname, &admin_msg(&format!("You have been teleported to {}.", room_id)), &handle.sessions).await;
            state.tell_player(player_name, &admin_msg(&format!("Teleported {} to {}.", tname, room_id)), &handle.sessions).await;
        }
    }
}

async fn cmd_admin_info(handle: &GameHandle, player_name: &str, args: &str) {
    if !require_admin(handle, player_name).await { return; }
    let (subject, name) = parse_input(args);
    let state = handle.state.read().await;
    match subject {
        "room" => {
            let room_id = if name.is_empty() {
                state.players.get(player_name).map(|p| p.room.clone()).unwrap_or_default()
            } else { name.to_string() };
            if let Some(room) = state.world.get_room(&room_id) {
                let info = format!(
                    "\r\n[ROOM INFO] {}\r\n  ID: {}\r\n  Area: {}\r\n  Exits: {:?}\r\n  NPCs: {:?}\r\n  Script: {:?}\r\n  History: {:?}\r\n",
                    room.name, room.id, room.area, room.exit_list(), room.npcs, room.script, room.history
                );
                state.tell_player(player_name, &admin_msg(&info), &handle.sessions).await;
            }
        }
        "player" => {
            if let Some(p) = state.players.get(name) {
                let info = format!(
                    "\r\n[PLAYER INFO] {}\r\n  Room: {}\r\n  Level: {}  XP: {}\r\n  HP: {}/{}\r\n  Combat: {:?}\r\n  Admin: {}\r\n",
                    p.name, p.room, p.level, p.experience, p.stats.hp, p.stats.max_hp, p.in_combat_with, p.is_admin
                );
                state.tell_player(player_name, &admin_msg(&info), &handle.sessions).await;
            } else {
                state.tell_player(player_name, &error_msg("Player not found."), &handle.sessions).await;
            }
        }
        _ => { state.tell_player(player_name, &error_msg("info room|player [name]"), &handle.sessions).await; }
    }
}

async fn cmd_admin_shutdown(handle: &GameHandle, player_name: &str) {
    if !require_admin(handle, player_name).await { return; }
    let state = handle.state.read().await;
    state.tell_all(&bright_red("*** The world is shutting down. Farewell! ***"), &handle.sessions).await;
    drop(state);
    // Save all players
    {
        let state = handle.state.read().await;
        for player in state.players.values() {
            let _ = player.save(&state.config.game.players_path);
        }
    }
    std::process::exit(0);
}

async fn cmd_admin_set(handle: &GameHandle, player_name: &str, args: &str) {
    if !require_admin(handle, player_name).await { return; }
    // "set player <name> <stat> <value>"
    let parts: Vec<&str> = args.splitn(4, ' ').collect();
    if parts.len() < 4 || parts[0] != "player" {
        handle.state.read().await.tell_player(player_name, &error_msg("set player <name> <stat> <value>"), &handle.sessions).await;
        return;
    }
    let mut state = handle.state.write().await;
    let target = parts[1];
    let stat = parts[2];
    let value: i64 = match parts[3].parse() { Ok(v) => v, Err(_) => {
        state.tell_player(player_name, &error_msg("Value must be a number."), &handle.sessions).await;
        return;
    }};
    if let Some(player) = state.players.get_mut(target) {
        match stat {
            "hp" => player.stats.hp = value.max(0).min(i32::MAX as i64) as i32,
            "maxhp" => player.stats.max_hp = value.max(1).min(i32::MAX as i64) as i32,
            "mp" => player.stats.mp = value.max(0).min(i32::MAX as i64) as i32,
            "maxmp" => player.stats.max_mp = value.max(1).min(i32::MAX as i64) as i32,
            "level" => player.level = value.max(1).min(9999) as u32,
            "xp" => player.experience = value.max(0) as u64,
            "coins" => player.coins = value.max(0).min(u32::MAX as i64) as u32,
            "admin" => player.is_admin = value != 0,
            _ => { state.tell_player(player_name, &error_msg(&format!("Unknown stat: {}", stat)), &handle.sessions).await; return; }
        }
        state.tell_player(player_name, &admin_msg(&format!("Set {}.{} = {}", target, stat, value)), &handle.sessions).await;
        state.tell_player(target, &admin_msg(&format!("An admin has adjusted your {}.", stat)), &handle.sessions).await;
    } else {
        state.tell_player(player_name, &error_msg("Player not found."), &handle.sessions).await;
    }
}

// ─── Script Actions ───────────────────────────────────────────────────────────


// ─── Utility ──────────────────────────────────────────────────────────────────

/// Returns true if `kw` matches the NPC's name OR any of its template keywords.
fn npc_matches(
    npc: &crate::entity::ActiveNpc,
    world: &crate::world::World,
    kw: &str,
) -> bool {
    if npc.name.to_lowercase().contains(kw) {
        return true;
    }
    world.get_npc_template(&npc.template_id)
        .map(|t| t.keywords.iter().any(|k| k.to_lowercase().contains(kw)))
        .unwrap_or(false)
}

fn find_player_name<'a>(
    players: &'a std::collections::HashMap<String, Player>,
    keyword: &str,
) -> Option<&'a str> {
    let kw = keyword.to_lowercase();
    players.keys().find(|name| name.to_lowercase().starts_with(&kw)).map(|s| s.as_str())
}

pub fn hash_password(password: &str) -> String {
    use argon2::{
        password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
        Argon2,
    };
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("argon2 hash failed")
        .to_string()
}

pub fn verify_password(stored_hash: &str, password: &str) -> bool {
    use argon2::{
        password_hash::{PasswordHash, PasswordVerifier},
        Argon2,
    };
    PasswordHash::new(stored_hash)
        .ok()
        .map(|hash| Argon2::default().verify_password(password.as_bytes(), &hash).is_ok())
        .unwrap_or(false)
}

