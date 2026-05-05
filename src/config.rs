use serde::Deserialize;
use anyhow::Result;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub game: GameConfig,
    pub combat: CombatConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
    pub bind_addr: String,
    #[allow(dead_code)]
    pub max_players: usize,
    pub motd: String,
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    #[serde(default = "default_ssh_host_key_path")]
    pub ssh_host_key_path: String,
}

fn default_ssh_port() -> u16 { 2222 }
fn default_ssh_host_key_path() -> String { "data/ssh_host_key".into() }

#[derive(Debug, Clone, Deserialize)]
pub struct GameConfig {
    pub tick_rate_ms: u64,
    pub game_time_multiplier: u64,
    pub world_path: String,
    pub players_path: String,
    pub start_room: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CombatConfig {
    pub base_hit_chance: u32,
    pub round_duration_ticks: u32,
    pub flee_success_chance: u32,
}

impl Config {
    pub fn load(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|_| include_str!("../config.toml").to_string());
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}

impl Default for Config {
    fn default() -> Self {
        toml::from_str(include_str!("../config.toml")).expect("default config invalid")
    }
}
