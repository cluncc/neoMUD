use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameEvent {
    // Player movement
    PlayerEnterRoom { player: String, room: String, from_dir: Option<String> },
    PlayerLeaveRoom { player: String, room: String, to_dir: Option<String> },
    // Communication
    PlayerSay { player: String, room: String, message: String },
    PlayerShout { player: String, area: String, message: String },
    PlayerEmote { player: String, room: String, action: String },
    GlobalMessage { message: String },
    // Combat
    CombatStart { attacker: String, defender: String, room: String },
    CombatHit { attacker: String, defender: String, damage: i32, weapon: String },
    CombatMiss { attacker: String, defender: String },
    CombatEnd { winner: String, loser: String, loser_died: bool },
    // World
    ItemPickedUp { player: String, item: String, room: String },
    ItemDropped { player: String, item: String, room: String },
    NpcSpawned { npc: String, room: String },
    NpcDied { npc: String, room: String, killer: Option<String> },
    PlayerDied { player: String, room: String, killer: Option<String> },
    PlayerRespawned { player: String, room: String },
    // Time/weather
    HourChanged { hour: u32, time_of_day: String },
    DayChanged { day: u32, month: u32, year: u32 },
    WeatherChanged { area: String, old_weather: String, new_weather: String },
    // Skills / progression
    SkillGained { player: String, skill: String, new_level: u32 },
    LevelUp { player: String, new_level: u32 },
    // World events
    RoomEvent { room: String, event: String, data: String },
    AreaEvent { area: String, event: String, data: String },
    // Admin
    PlayerConnected { player: String },
    PlayerDisconnected { player: String },
    Shutdown,
}

pub type EventBus = broadcast::Sender<GameEvent>;

pub fn create_event_bus(capacity: usize) -> EventBus {
    broadcast::channel(capacity).0
}
