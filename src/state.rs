use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use dashmap::DashMap;
use uuid::Uuid;

use crate::config::Config;
use crate::entity::{ActiveNpc, ItemInstance, Player, Skill, Stats};
use crate::events::{EventBus, GameEvent, create_event_bus};
use crate::scripting::{ScriptAction, ScriptEngine};
use crate::time::{GameTime, Weather};
use crate::world::{World, NpcBehavior};
use crate::combat::{
    roll_hit, roll_damage, apply_damage, tick_status_effects,
    xp_for_kill, AttackMessages,
};

pub type SessionTx = mpsc::Sender<String>;
pub type SessionMap = Arc<DashMap<String, SessionTx>>;

// ─── Game State ──────────────────────────────────────────────────────────────

pub struct GameState {
    pub world: World,
    pub players: HashMap<String, Player>,    // name → Player
    pub npcs: HashMap<String, ActiveNpc>,    // instance_id → ActiveNpc
    pub items_on_ground: HashMap<String, Vec<crate::entity::ItemInstance>>, // room_id → items
    pub time: GameTime,
    pub area_weather: HashMap<String, Weather>,  // area_id → weather
    pub combat_tick_counter: HashMap<String, u32>, // npc_id → ticks until next round
    pub config: Config,
    tick_count: u64,
}

impl GameState {
    pub async fn new(config: &Config) -> anyhow::Result<Self> {
        let world = World::load(&config.game.world_path)?;
        let time = GameTime::new();
        let mut area_weather = HashMap::new();
        for (id, area) in &world.areas {
            area_weather.insert(id.clone(), area.weather.clone());
        }

        let mut gs = GameState {
            world,
            players: HashMap::new(),
            npcs: HashMap::new(),
            items_on_ground: HashMap::new(),
            time,
            area_weather,
            combat_tick_counter: HashMap::new(),
            config: config.clone(),
            tick_count: 0,
        };

        gs.spawn_world_npcs();
        Ok(gs)
    }

    /// Populate NPCs defined in room spawn lists.
    fn spawn_world_npcs(&mut self) {
        let mut to_spawn: Vec<(String, String, u32)> = vec![]; // (template_id, room_id, count)
        for area in self.world.areas.values() {
            for room in area.rooms.values() {
                if let Some(spawns) = &room.spawn_npcs {
                    for spawn in spawns {
                        to_spawn.push((spawn.template.clone(), room.id.clone(), spawn.count));
                    }
                }
            }
        }
        for (template_id, room_id, count) in to_spawn {
            for _ in 0..count {
                self.spawn_npc(&template_id, &room_id);
            }
        }
    }

    pub fn spawn_npc(&mut self, template_id: &str, room_id: &str) -> Option<String> {
        let template = self.world.get_npc_template(template_id)?.clone();
        let instance_id = Uuid::new_v4().to_string();

        // Build stats from template
        let mut stats = Stats::for_class_race(
            &crate::entity::Class::Warrior,
            &crate::entity::Race::Human,
            template.level,
        );
        stats.max_hp = template.base_hp;
        stats.hp = template.base_hp;
        stats.max_mp = template.base_mp;
        stats.mp = template.base_mp;
        stats.strength = template.base_strength;
        stats.dexterity = template.base_dexterity;
        stats.constitution = template.base_constitution;

        let npc = ActiveNpc {
            instance_id: instance_id.clone(),
            template_id: template_id.to_string(),
            name: template.name.clone(),
            room: room_id.to_string(),
            stats,
            inventory: vec![],
            faction: template.faction.clone(),
            alive: true,
            in_combat_with: None,
            memory: vec![],
            status_effects: vec![],
            respawn_at_tick: None,
        };

        // Add to room
        if let Some(room) = self.world.get_room_mut(room_id) {
            room.npcs.push(instance_id.clone());
        }

        self.npcs.insert(instance_id.clone(), npc);
        Some(instance_id)
    }

    pub fn get_room_weather(&self, room_id: &str) -> Weather {
        let area_id = room_id.split(':').next().unwrap_or("nexus");
        self.area_weather.get(area_id).cloned().unwrap_or(Weather::Clear)
    }

    pub fn players_in_room(&self, room_id: &str) -> Vec<&Player> {
        self.players.values().filter(|p| p.room == room_id).collect()
    }

    #[allow(dead_code)]
    pub fn npcs_in_room(&self, room_id: &str) -> Vec<&ActiveNpc> {
        self.npcs.values().filter(|n| n.room == room_id && n.alive).collect()
    }

    pub fn player_names_in_room(&self, room_id: &str) -> Vec<String> {
        self.players.values()
            .filter(|p| p.room == room_id)
            .map(|p| p.name.clone())
            .collect()
    }

    /// Send a message to all players in a room.
    pub async fn tell_room(&self, room_id: &str, msg: &str, sessions: &SessionMap) {
        for name in self.player_names_in_room(room_id) {
            self.tell_player(&name, msg, sessions).await;
        }
    }

    /// Send a message to all players in a room except one.
    pub async fn tell_room_except(&self, room_id: &str, except: &str, msg: &str, sessions: &SessionMap) {
        for name in self.player_names_in_room(room_id) {
            if name != except {
                self.tell_player(&name, msg, sessions).await;
            }
        }
    }

    pub async fn tell_player(&self, name: &str, msg: &str, sessions: &SessionMap) {
        if let Some(tx) = sessions.get(name) {
            let _ = tx.send(format!("{}\r\n", msg)).await;
        }
    }

    /// Send a message to all players in an area.
    pub async fn tell_area(&self, area_id: &str, msg: &str, sessions: &SessionMap) {
        for player in self.players.values() {
            if player.room.starts_with(&format!("{}:", area_id)) {
                self.tell_player(&player.name, msg, sessions).await;
            }
        }
    }

    /// Send to all logged-in players.
    pub async fn tell_all(&self, msg: &str, sessions: &SessionMap) {
        for name in self.players.keys() {
            self.tell_player(name, msg, sessions).await;
        }
    }

    // ─── Game Tick ───────────────────────────────────────────────────────────

    pub async fn tick(
        &mut self,
        sessions: &SessionMap,
        events: &EventBus,
        scripts: &ScriptEngine,
    ) {
        self.tick_count += 1;
        let multiplier = self.config.game.game_time_multiplier;
        let (hour_changed, day_changed) = self.time.advance(multiplier);

        if hour_changed {
            let _ = events.send(GameEvent::HourChanged {
                hour: self.time.hour,
                time_of_day: self.time.time_of_day().to_string(),
            });
            self.tick_weather(events);
            self.announce_time_of_day(sessions).await;
        }

        if day_changed {
            let _ = events.send(GameEvent::DayChanged {
                day: self.time.day, month: self.time.month, year: self.time.year,
            });
        }

        // Combat ticks
        self.process_combat_ticks(sessions, events, scripts).await;

        // NPC AI + script ticks (every 4 ticks = 1 second)
        if self.tick_count % 4 == 0 {
            self.process_npc_ai(sessions, events).await;
            self.process_script_ticks(sessions, scripts).await;
        }

        // Respawn dead NPCs
        self.process_respawns();

        // Status effect ticks on players
        self.tick_player_effects(sessions).await;
    }

    async fn process_script_ticks(&mut self, sessions: &SessionMap, scripts: &ScriptEngine) {
        let time_str = self.time.time_of_day().to_string();
        let hour = self.time.hour as i64;

        // Collect scripted rooms that currently have players
        let scripted_rooms: Vec<(String, String, Vec<String>)> = {
            let world = &self.world;
            let players = &self.players;
            world.areas.values()
                .flat_map(|a| a.rooms.values())
                .filter_map(|r| {
                    let script_name = r.script.as_ref()?;
                    let occupants: Vec<String> = players.values()
                        .filter(|p| p.room == r.id)
                        .map(|p| p.name.clone())
                        .collect();
                    if occupants.is_empty() { return None; }
                    Some((r.id.clone(), script_name.clone(), occupants))
                })
                .collect()
        };

        for (room_id, script_name, occupants) in scripted_rooms {
            let ctx = {
                let players_arr: rhai::Array = occupants.iter()
                    .map(|n| rhai::Dynamic::from(n.clone()))
                    .collect();
                let mut m = rhai::Map::new();
                m.insert("room".into(), rhai::Dynamic::from(room_id.clone()));
                m.insert("players".into(), rhai::Dynamic::from(players_arr));
                m.insert("time".into(), rhai::Dynamic::from(time_str.clone()));
                m.insert("hour".into(), rhai::Dynamic::from(hour));
                rhai::Dynamic::from(m)
            };
            let ctx_player = occupants.into_iter().next().unwrap_or_default();
            let actions = scripts.call_hook(&script_name, "on_tick", ctx);
            self.apply_script_actions(actions, &ctx_player, &room_id, sessions).await;
        }

        // Collect alive scripted NPCs whose room has players
        let scripted_npcs: Vec<(String, String, String, String)> = {
            let world = &self.world;
            let players = &self.players;
            self.npcs.values()
                .filter(|n| n.alive)
                .filter_map(|n| {
                    let script_name = world.get_npc_template(&n.template_id)?.script.clone()?;
                    let has_players = players.values().any(|p| p.room == n.room);
                    if !has_players { return None; }
                    Some((n.instance_id.clone(), n.name.clone(), n.room.clone(), script_name))
                })
                .collect()
        };

        for (npc_id, npc_name, room_id, script_name) in scripted_npcs {
            let ctx_player = self.players.values()
                .find(|p| p.room == room_id)
                .map(|p| p.name.clone())
                .unwrap_or_default();
            let ctx = {
                let mut m = rhai::Map::new();
                m.insert("npc".into(), rhai::Dynamic::from(npc_id));
                m.insert("npc_name".into(), rhai::Dynamic::from(npc_name));
                m.insert("room".into(), rhai::Dynamic::from(room_id.clone()));
                m.insert("time".into(), rhai::Dynamic::from(time_str.clone()));
                m.insert("hour".into(), rhai::Dynamic::from(hour));
                rhai::Dynamic::from(m)
            };
            let actions = scripts.call_hook(&script_name, "on_tick", ctx);
            self.apply_script_actions(actions, &ctx_player, &room_id, sessions).await;
        }
    }

    /// Execute a list of script actions against the live game state.
    pub async fn apply_script_actions(
        &mut self,
        actions: Vec<ScriptAction>,
        player_name: &str,
        room_id: &str,
        sessions: &SessionMap,
    ) {
        for action in actions {
            match action.action.as_str() {
                "tell_player" => {
                    let p = action.params.get("player").and_then(|v| v.as_str()).unwrap_or(player_name);
                    let msg = action.params.get("msg").and_then(|v| v.as_str()).unwrap_or("");
                    self.tell_player(p, msg, sessions).await;
                }
                "tell_room" => {
                    let r = action.params.get("room").and_then(|v| v.as_str()).unwrap_or(room_id);
                    let msg = action.params.get("msg").and_then(|v| v.as_str()).unwrap_or("");
                    self.tell_room(r, msg, sessions).await;
                }
                "tell_area" => {
                    let area = action.params.get("area").and_then(|v| v.as_str()).unwrap_or("");
                    let msg = action.params.get("msg").and_then(|v| v.as_str()).unwrap_or("");
                    if !area.is_empty() {
                        self.tell_area(area, msg, sessions).await;
                    }
                }
                "move_player" => {
                    let target = action.params.get("player").and_then(|v| v.as_str()).unwrap_or(player_name);
                    let to = action.params.get("to").and_then(|v| v.as_str()).unwrap_or("");
                    if !to.is_empty() && self.world.get_room(to).is_some() {
                        if let Some(player) = self.players.get_mut(target) {
                            player.room = to.to_string();
                        }
                    }
                }
                "move_npc" => {
                    let npc_id = action.params.get("npc").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let to = action.params.get("to").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    if !npc_id.is_empty() && !to.is_empty() && self.world.get_room(&to).is_some() {
                        let old_room = self.npcs.get(&npc_id).map(|n| n.room.clone());
                        if let (Some(old_room), Some(npc)) = (old_room, self.npcs.get_mut(&npc_id)) {
                            npc.room = to.clone();
                            if let Some(room) = self.world.get_room_mut(&old_room) {
                                room.npcs.retain(|id| id != &npc_id);
                            }
                            if let Some(room) = self.world.get_room_mut(&to) {
                                room.npcs.push(npc_id);
                            }
                        }
                    }
                }
                "spawn_npc" => {
                    let template_id = action.params.get("template").and_then(|v| v.as_str()).unwrap_or("");
                    let target_room = action.params.get("room").and_then(|v| v.as_str()).unwrap_or(room_id).to_string();
                    if !template_id.is_empty() {
                        self.spawn_npc(template_id, &target_room);
                    }
                }
                "spawn_item" => {
                    let template_id = action.params.get("template").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let target_room = action.params.get("room").and_then(|v| v.as_str()).unwrap_or(room_id).to_string();
                    if !template_id.is_empty() {
                        if let Some(item_name) = self.world.get_item_template(&template_id).map(|t| t.name.clone()) {
                            let item = ItemInstance::new(&template_id, &item_name);
                            self.items_on_ground.entry(target_room).or_default().push(item);
                        }
                    }
                }
                "give_item" => {
                    let pname = action.params.get("player").and_then(|v| v.as_str()).unwrap_or(player_name).to_string();
                    let template_id = action.params.get("template").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    if !template_id.is_empty() {
                        if let Some(item_name) = self.world.get_item_template(&template_id).map(|t| t.name.clone()) {
                            let item = ItemInstance::new(&template_id, &item_name);
                            let msg = crate::color::success_msg(&format!("You receive {}.", item_name));
                            if let Some(player) = self.players.get_mut(&pname) {
                                player.inventory.push(item);
                            }
                            self.tell_player(&pname, &msg, sessions).await;
                        }
                    }
                }
                "heal_player" => {
                    let target = action.params.get("player").and_then(|v| v.as_str()).unwrap_or(player_name);
                    // Clamp to i32 range BEFORE casting; raw `as i32` truncates and a
                    // script-supplied amount of i64::MAX would wrap to -1, turning a
                    // heal into damage.
                    let amount = action.params.get("amount").and_then(|v| v.as_i64()).unwrap_or(10)
                        .clamp(0, i32::MAX as i64) as i32;
                    if let Some(player) = self.players.get_mut(target) {
                        let headroom = (player.stats.max_hp - player.stats.hp).max(0);
                        let healed = amount.min(headroom);
                        player.stats.hp = player.stats.hp.saturating_add(healed);
                        let msg = crate::color::heal_text(&format!("You are healed for {} HP!", healed));
                        self.tell_player(target, &msg, sessions).await;
                    }
                }
                "damage_player" => {
                    let target = action.params.get("player").and_then(|v| v.as_str()).unwrap_or(player_name);
                    // Clamp to i32 range BEFORE casting; raw `as i32` truncates and a
                    // script-supplied amount of i64::MAX would wrap to a negative
                    // value, healing the player instead of damaging them.
                    let amount = action.params.get("amount").and_then(|v| v.as_i64()).unwrap_or(0)
                        .clamp(0, i32::MAX as i64) as i32;
                    if let Some(player) = self.players.get_mut(target) {
                        player.stats.hp = player.stats.hp.saturating_sub(amount);
                        let msg = crate::color::damage_in(&format!("You take {} damage!", amount));
                        self.tell_player(target, &msg, sessions).await;
                    }
                }
                "record_history" => {
                    let r = action.params.get("room").and_then(|v| v.as_str()).unwrap_or(room_id);
                    let event = action.params.get("event").and_then(|v| v.as_str()).unwrap_or("");
                    if let Some(room) = self.world.get_room_mut(r) {
                        room.record_history(event.to_string());
                    }
                }
                "grant_skill" => {
                    let target = action.params.get("player").and_then(|v| v.as_str()).unwrap_or(player_name);
                    let skill_name = action.params.get("skill").and_then(|v| v.as_str()).unwrap_or("");
                    if !skill_name.is_empty() {
                        if let Some(player) = self.players.get_mut(target) {
                            player.skills.entry(skill_name.to_string()).or_insert_with(|| Skill::new(skill_name));
                            let msg = crate::color::success_msg(&format!("You gain the '{}' skill!", skill_name));
                            self.tell_player(target, &msg, sessions).await;
                        }
                    }
                }
                "adjust_rep" => {
                    let target = action.params.get("player").and_then(|v| v.as_str()).unwrap_or(player_name);
                    let faction = action.params.get("faction").and_then(|v| v.as_str()).unwrap_or("");
                    let amount = action.params.get("amount").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                    if let Some(player) = self.players.get_mut(target) {
                        player.adjust_reputation(faction, amount);
                    }
                }
                "set_flag" => {
                    let target = action.params.get("target").and_then(|v| v.as_str()).unwrap_or("");
                    let id = action.params.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let flag = action.params.get("flag").and_then(|v| v.as_str()).unwrap_or("");
                    let value = action.params.get("value").and_then(|v| v.as_bool()).unwrap_or(false);
                    match target {
                        "player" => {
                            let pname = if id.is_empty() { player_name } else { id };
                            if let Some(player) = self.players.get_mut(pname) {
                                player.quest_flags.insert(flag.to_string(), value);
                            }
                        }
                        "room" => {
                            let rid = if id.is_empty() { room_id } else { id };
                            if let Some(room) = self.world.get_room_mut(rid) {
                                match flag {
                                    "safe"       => room.flags.safe = value,
                                    "dark"       => room.flags.dark = value,
                                    "outside"    => room.flags.outside = value,
                                    "water"      => room.flags.water = value,
                                    "underwater" => room.flags.underwater = value,
                                    "no_magic"   => room.flags.no_magic = value,
                                    "no_recall"  => room.flags.no_recall = value,
                                    "indoors"    => room.flags.indoors = value,
                                    "shop"       => room.flags.shop = value,
                                    "bank"       => room.flags.bank = value,
                                    "death_trap" => room.flags.death_trap = value,
                                    _ => {}
                                }
                            }
                        }
                        "npc" => {
                            if !id.is_empty() {
                                if let Some(npc) = self.npcs.get_mut(id) {
                                    if flag == "alive" { npc.alive = value; }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    fn tick_weather(&mut self, events: &EventBus) {
        let season = self.time.season();
        // Weather transitions roughly once per game-day
        if self.time.hour == 6 || self.time.hour == 18 {
            for (area_id, weather) in self.area_weather.iter_mut() {
                let old = weather.to_string();
                let new_w = weather.transition(&season);
                if *weather != new_w {
                    let _ = events.send(GameEvent::WeatherChanged {
                        area: area_id.clone(),
                        old_weather: old,
                        new_weather: new_w.to_string(),
                    });
                    *weather = new_w;
                }
            }
        }
    }

    async fn announce_time_of_day(&self, sessions: &SessionMap) {
        use crate::time::TimeOfDay;
        use crate::color;
        let tod = self.time.time_of_day();
        let msg = match tod {
            TimeOfDay::Dawn   => Some(color::yellow("The first pale light of dawn creeps over the horizon.")),
            TimeOfDay::Morning=> Some(color::bright_yellow("The sun climbs above the horizon, bathing everything in warm morning light.")),
            TimeOfDay::Midday => Some(color::yellow("The sun reaches its zenith, blazing overhead.")),
            TimeOfDay::Dusk   => Some(color::bright_red("The sun dips below the horizon, painting the sky in shades of orange and crimson.")),
            TimeOfDay::Night  => Some(color::blue("Night settles across the land. Stars emerge in the darkening sky.")),
            TimeOfDay::DeepNight => Some(color::dim("The world is plunged into the stillness of deep night.")),
            _ => None,
        };
        if let Some(m) = msg {
            for (_area_id, area) in &self.world.areas {
                // Only announce in outside rooms
                for (_, room) in &area.rooms {
                    if room.flags.outside {
                        for name in self.player_names_in_room(&room.id) {
                            self.tell_player(&name, &m, sessions).await;
                        }
                    }
                }
            }
        }
    }

    async fn process_combat_ticks(&mut self, sessions: &SessionMap, events: &EventBus, scripts: &ScriptEngine) {
        let round_ticks = self.config.combat.round_duration_ticks;
        let base_hit = self.config.combat.base_hit_chance;

        // Collect combats: npc_id → player_name
        let combats: Vec<(String, String)> = self.npcs.values()
            .filter(|n| n.alive && n.in_combat_with.is_some())
            .map(|n| (n.instance_id.clone(), n.in_combat_with.clone().unwrap()))
            .collect();

        for (npc_id, player_name) in combats {
            let counter = self.combat_tick_counter.entry(npc_id.clone()).or_insert(round_ticks);
            if *counter > 0 {
                *counter -= 1;
                continue;
            }
            *counter = round_ticks;

            // Attacker (NPC) → Defender (Player) round
            let npc = match self.npcs.get(&npc_id) { Some(n) => n.clone(), None => continue };
            let player = match self.players.get_mut(&player_name) { Some(p) => p, None => continue };

            let (hit, crit) = roll_hit(&npc.stats, &player.stats, base_hit);
            if hit {
                let dmg = roll_damage(&npc.stats, 2, 8, crit, 1);
                let (_a_msg, d_msg, _r_msg) = AttackMessages::hit_message(&npc.name, &player_name, dmg, "claws", crit);
                let died = apply_damage(&mut player.stats, dmg);

                self.tell_player(&player_name, &crate::color::damage_in(&d_msg), sessions).await;
                let _ = events.send(GameEvent::CombatHit {
                    attacker: npc.name.clone(),
                    defender: player_name.clone(),
                    damage: dmg,
                    weapon: "claws".into(),
                });

                if died {
                    self.handle_player_death(&player_name, Some(&npc_id.clone()), sessions, events).await;
                }
            } else {
                let (_a, d_msg, _r) = AttackMessages::miss_message(&npc.name, &player_name);
                self.tell_player(&player_name, &crate::color::dim(&d_msg), sessions).await;
            }

            // Counter-attack: player → NPC
            // Skip if player died or fled (in_combat_with cleared by handle_player_death)
            let player = match self.players.get(&player_name) { Some(p) => p.clone(), None => continue };
            if !player.stats.is_alive() || player.in_combat_with.is_none() { continue; }
            let npc = match self.npcs.get_mut(&npc_id) { Some(n) => n, None => continue };
            let (hit2, crit2) = roll_hit(&player.stats, &npc.stats, base_hit);
            if hit2 {
                let weapon_skill = player.skills.get("sword").or_else(|| player.skills.get("dagger"))
                    .map(|s| s.level).unwrap_or(1);
                let dmg2 = roll_damage(&player.stats, 1, 6, crit2, weapon_skill);
                let (a_msg, _d, _r_msg) = AttackMessages::hit_message(&player_name, &npc.name, dmg2, "sword", crit2);
                let npc_name = npc.name.clone();
                let died = apply_damage(&mut npc.stats, dmg2);
                npc.alive = !died;

                self.tell_player(&player_name, &crate::color::damage_out(&a_msg), sessions).await;
                let _ = events.send(GameEvent::CombatHit {
                    attacker: player_name.clone(),
                    defender: npc_name.clone(),
                    damage: dmg2,
                    weapon: "sword".into(),
                });

                if died {
                    self.handle_npc_death(&npc_id, &player_name, sessions, events, scripts).await;
                }
            } else {
                let npc_name_tmp = self.npcs.get(&npc_id).map(|n| n.name.clone()).unwrap_or_else(|| "target".into());
                let (a_msg, _d, _r) = AttackMessages::miss_message(&player_name, &npc_name_tmp);
                self.tell_player(&player_name, &crate::color::dim(&a_msg), sessions).await;
            }
        }
    }

    async fn handle_npc_death(
        &mut self, npc_id: &str, killer_name: &str,
        sessions: &SessionMap, events: &EventBus, scripts: &ScriptEngine,
    ) {
        let npc = match self.npcs.get_mut(npc_id) { Some(n) => n, None => return };
        npc.alive = false;
        npc.in_combat_with = None;
        let npc_name = npc.name.clone();
        let room_id = npc.room.clone();
        let template_id = npc.template_id.clone();
        let (respawn_minutes, npc_script) = {
            let tmpl = self.world.get_npc_template(&template_id);
            let rm = tmpl.and_then(|t| t.respawn_minutes).unwrap_or(5);
            let sc = tmpl.and_then(|t| t.script.clone());
            (rm, sc)
        };
        if let Some(npc) = self.npcs.get_mut(npc_id) {
            npc.respawn_at_tick = Some(self.tick_count + (respawn_minutes as u64 * 240));
        }

        // Remove from room
        if let Some(room) = self.world.get_room_mut(&room_id) {
            room.npcs.retain(|id| id != npc_id);
            room.record_history(format!("{} was slain here.", npc_name));
        }

        // XP and drops — collect messages before awaiting to avoid borrow conflict
        let template = self.world.get_npc_template(&template_id).cloned();
        let mut pending_msgs: Vec<String> = vec![];
        let mut level_up_lvl: Option<u32> = None;
        if let Some(tmpl) = template {
            if let Some(player) = self.players.get_mut(killer_name) {
                let xp = xp_for_kill(player.level, tmpl.level, tmpl.xp_reward);
                let did_level = player.gain_xp(xp);
                player.kills += 1;
                player.in_combat_with = None;
                let new_level = player.level;
                if let Some((min, max)) = tmpl.coin_drop {
                    let coins = { use rand::Rng; let mut rng = rand::thread_rng(); rng.gen_range(min..=max) };
                    if coins > 0 {
                        player.coins += coins;
                        pending_msgs.push(crate::color::yellow(&format!("You loot {} coins from {}.", coins, npc_name)));
                    }
                }
                pending_msgs.push(crate::color::bright_green(&format!("You have slain {}! (+{} xp)", npc_name, xp)));
                if did_level {
                    pending_msgs.push(crate::color::bright_yellow(&format!("*** You have reached level {}! ***", new_level)));
                    level_up_lvl = Some(new_level);
                }
            }
        }
        for msg in pending_msgs {
            self.tell_player(killer_name, &msg, sessions).await;
        }
        if let Some(lvl) = level_up_lvl {
            let _ = events.send(GameEvent::LevelUp { player: killer_name.to_string(), new_level: lvl });
        }

        let _ = events.send(GameEvent::NpcDied {
            npc: npc_name.clone(), room: room_id.clone(),
            killer: Some(killer_name.to_string()),
        });
        self.combat_tick_counter.remove(npc_id);

        // on_die script hook
        if let Some(script_name) = npc_script {
            let ctx = {
                let mut m = rhai::Map::new();
                m.insert("player".into(), rhai::Dynamic::from(killer_name.to_string()));
                m.insert("npc".into(), rhai::Dynamic::from(npc_id.to_string()));
                m.insert("npc_name".into(), rhai::Dynamic::from(npc_name));
                m.insert("room".into(), rhai::Dynamic::from(room_id.clone()));
                rhai::Dynamic::from(m)
            };
            let actions = scripts.call_hook(&script_name, "on_die", ctx);
            self.apply_script_actions(actions, killer_name, &room_id, sessions).await;
        }
    }

    async fn handle_player_death(
        &mut self, player_name: &str, killer_npc_id: Option<&str>,
        sessions: &SessionMap, events: &EventBus,
    ) {
        let killer_name = killer_npc_id
            .and_then(|id| self.npcs.get(id))
            .map(|n| n.name.clone());

        let player = match self.players.get_mut(player_name) { Some(p) => p, None => return };
        player.deaths += 1;
        player.in_combat_with = None;
        // Restore to 1 HP at start room (simple respawn)
        player.stats.hp = 1;
        let respawn_room = self.config.game.start_room.clone();
        let old_room = player.room.clone();
        player.room = respawn_room.clone();

        self.tell_player(player_name,
            &crate::color::bright_red("You have been slain!\r\n\r\nYou feel your spirit torn from your body..."),
            sessions).await;

        if let Some(ref _kn) = killer_name {
            if let Some(npc) = killer_npc_id.and_then(|id| self.npcs.get_mut(id)) {
                npc.in_combat_with = None;
            }
        }

        self.tell_player(player_name,
            &crate::color::yellow("\r\nYou awaken in a familiar place, grateful to be alive."),
            sessions).await;

        let _ = events.send(GameEvent::PlayerDied {
            player: player_name.to_string(),
            room: old_room,
            killer: killer_name,
        });
    }

    async fn process_npc_ai(&mut self, sessions: &SessionMap, events: &EventBus) {
        // All random decisions are made in a sync block BEFORE any .await
        // (ThreadRng is !Send and cannot be held across await points)
        let (wanderer_moves, combat_pairs): (Vec<(String, String)>, Vec<(String, String, String)>) = {
            use rand::Rng;
            let mut rng = rand::thread_rng();

            let wanderers: Vec<String> = self.npcs.values()
                .filter(|n| n.alive && n.in_combat_with.is_none())
                .filter(|n| matches!(
                    self.world.get_npc_template(&n.template_id).map(|t| &t.behavior),
                    Some(NpcBehavior::Wanderer)
                ))
                .map(|n| n.instance_id.clone())
                .collect();

            let mut moves = vec![];
            for npc_id in wanderers {
                if rng.gen_range(0u32..100) < 20 {
                    let npc_room = self.npcs.get(&npc_id).map(|n| n.room.clone());
                    if let Some(room_id) = npc_room {
                        let exits: Vec<String> = self.world.get_room(&room_id)
                            .map(|r| r.exits.values().map(|e| e.to.clone()).collect())
                            .unwrap_or_default();
                        if !exits.is_empty() {
                            let idx = rng.gen_range(0..exits.len());
                            moves.push((npc_id, exits[idx].clone()));
                        }
                    }
                }
            }

            let aggressive: Vec<(String, String)> = self.npcs.values()
                .filter(|n| n.alive && n.in_combat_with.is_none())
                .filter(|n| matches!(
                    self.world.get_npc_template(&n.template_id).map(|t| &t.behavior),
                    Some(NpcBehavior::Aggressive)
                ))
                .map(|n| (n.instance_id.clone(), n.room.clone()))
                .collect();

            let mut pairs = vec![];
            for (npc_id, room_id) in aggressive {
                if let Some(player_name) = self.players.values()
                    .filter(|p| p.room == room_id && !p.is_in_combat() && p.stats.is_alive())
                    .map(|p| p.name.clone())
                    .next()
                {
                    pairs.push((npc_id, room_id, player_name));
                }
            }

            (moves, pairs)
        }; // rng dropped here — no ThreadRng held across await

        // Apply wanderer moves
        for (npc_id, new_room) in wanderer_moves {
            let old_room = self.npcs.get(&npc_id).map(|n| n.room.clone()).unwrap_or_default();
            if let Some(r) = self.world.get_room_mut(&old_room) { r.npcs.retain(|id| id != &npc_id); }
            if let Some(r) = self.world.get_room_mut(&new_room) { r.npcs.push(npc_id.clone()); }
            if let Some(n) = self.npcs.get_mut(&npc_id) { n.room = new_room; }
        }

        // Start combat for aggressive NPCs
        for (npc_id, room_id, player_name) in combat_pairs {
            if let Some(npc) = self.npcs.get_mut(&npc_id) {
                npc.in_combat_with = Some(player_name.clone());
            }
            if let Some(player) = self.players.get_mut(&player_name) {
                player.in_combat_with = Some(npc_id.clone());
            }
            let npc_name = self.npcs.get(&npc_id).map(|n| n.name.clone()).unwrap_or_default();
            self.tell_player(&player_name,
                &crate::color::bright_red(&format!("{} attacks you!", npc_name)),
                sessions).await;
            let _ = events.send(GameEvent::CombatStart {
                attacker: npc_name, defender: player_name, room: room_id,
            });
        }
    }

    fn process_respawns(&mut self) {
        let to_respawn: Vec<String> = self.npcs.values()
            .filter(|n| !n.alive)
            .filter(|n| n.respawn_at_tick.map(|t| t <= self.tick_count).unwrap_or(false))
            .map(|n| n.instance_id.clone())
            .collect();

        for npc_id in to_respawn {
            if let Some(npc) = self.npcs.get_mut(&npc_id) {
                let template_id = npc.template_id.clone();
                let room_id = npc.room.clone();
                // Reset stats
                if let Some(tmpl) = self.world.get_npc_template(&template_id) {
                    let tmpl = tmpl.clone();
                    npc.stats.hp = tmpl.base_hp;
                    npc.stats.max_hp = tmpl.base_hp;
                    npc.stats.mp = tmpl.base_mp;
                    npc.stats.max_mp = tmpl.base_mp;
                    npc.alive = true;
                    npc.respawn_at_tick = None;
                    npc.in_combat_with = None;
                    npc.status_effects.clear();
                }
                if let Some(room) = self.world.get_room_mut(&room_id) {
                    if !room.npcs.contains(&npc_id) {
                        room.npcs.push(npc_id);
                    }
                }
            }
        }
    }

    async fn tick_player_effects(&mut self, sessions: &SessionMap) {
        let names: Vec<String> = self.players.keys().cloned().collect();
        for name in names {
            let player = match self.players.get_mut(&name) { Some(p) => p, None => continue };
            let (dot, expired) = tick_status_effects(&mut player.status_effects);
            if dot > 0 {
                player.stats.hp -= dot;
                self.tell_player(&name,
                    &crate::color::red(&format!("You take {} damage from your afflictions!", dot)),
                    sessions).await;
            } else if dot < 0 {
                let heal = (-dot).min(player.stats.max_hp - player.stats.hp);
                player.stats.hp += heal;
                if heal > 0 {
                    self.tell_player(&name,
                        &crate::color::green(&format!("You regenerate {} HP.", heal)),
                        sessions).await;
                }
            }
            for eff in &expired {
                self.tell_player(&name,
                    &crate::color::dim(&format!("You are no longer {}.", eff)),
                    sessions).await;
            }
        }
    }
}

// ─── GameHandle: clone-friendly wrapper ──────────────────────────────────────

#[derive(Clone)]
pub struct GameHandle {
    pub state: Arc<RwLock<GameState>>,
    pub sessions: SessionMap,
    pub events: EventBus,
    pub scripts: Arc<ScriptEngine>,
    pub config: Config,
    /// Maps player name → name of the last player who sent them a tell.
    pub last_tell: Arc<DashMap<String, String>>,
}

impl GameHandle {
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let state = Arc::new(RwLock::new(GameState::new(&config).await?));
        let sessions = Arc::new(DashMap::new());
        let events = create_event_bus(1024);
        let scripts = Arc::new(ScriptEngine::new(&config.game.world_path));
        let last_tell = Arc::new(DashMap::new());
        Ok(GameHandle { state, sessions, events, scripts, config, last_tell })
    }

    pub async fn tick(&self) {
        let mut state = self.state.write().await;
        state.tick(&self.sessions, &self.events, &self.scripts).await;
    }
}
