use std::collections::HashMap;
use std::path::Path;
use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};
use tracing::{info, warn};

use crate::time::{Weather, GameTime};

// ─── ID Types ───────────────────────────────────────────────────────────────

pub type RoomId = String;
pub type AreaId = String;
pub type NpcTemplateId = String;
pub type ItemTemplateId = String;

// ─── Room ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub id: RoomId,
    pub area: AreaId,
    pub name: String,
    pub description: String,
    /// Alternative descriptions keyed by condition ("night", "rain", "fog", etc.)
    #[serde(default)]
    pub descriptions: HashMap<String, String>,
    pub exits: HashMap<String, Exit>,
    #[serde(default)]
    pub items: Vec<String>,          // item instance IDs
    #[serde(default)]
    pub npcs: Vec<String>,           // NPC instance IDs
    #[serde(default)]
    pub flags: RoomFlags,
    pub script: Option<String>,      // relative to world/scripts/
    #[serde(default)]
    pub history: Vec<String>,        // ring-buffer of past events
    #[serde(default)]
    pub lore: Vec<String>,           // static flavor lore visible on examine
    pub spawn_npcs: Option<Vec<NpcSpawn>>,
    pub spawn_items: Option<Vec<ItemSpawn>>,
}

impl Room {
    /// Return the best description for the current conditions.
    pub fn contextual_description(&self, time: &GameTime, weather: &Weather) -> String {
        let tod = time.time_of_day().to_string();
        // Priority: specific time+weather > time > weather > default
        let key_combo = format!("{}_{}", tod, weather);
        let key_tod = tod.clone();
        let key_weather = weather.to_string();

        self.descriptions.get(&key_combo)
            .or_else(|| self.descriptions.get(&key_tod))
            .or_else(|| self.descriptions.get(&key_weather))
            .unwrap_or(&self.description)
            .clone()
    }

    pub fn record_history(&mut self, event: String) {
        self.history.push(event);
        if self.history.len() > 20 {
            self.history.remove(0);
        }
    }

    pub fn exit_list(&self) -> String {
        if self.exits.is_empty() {
            return "none".to_string();
        }
        let mut dirs: Vec<&str> = self.exits.keys().map(|s| s.as_str()).collect();
        dirs.sort();
        dirs.join(", ")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoomFlags {
    #[serde(default)] pub safe: bool,
    #[serde(default)] pub dark: bool,
    #[serde(default)] pub outside: bool,
    #[serde(default)] pub water: bool,
    #[serde(default)] pub underwater: bool,
    #[serde(default)] pub no_magic: bool,
    #[serde(default)] pub no_recall: bool,
    #[serde(default)] pub indoors: bool,
    #[serde(default)] pub shop: bool,
    #[serde(default)] pub bank: bool,
    #[serde(default)] pub death_trap: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exit {
    pub to: RoomId,
    #[serde(default)] pub locked: bool,
    #[serde(default)] pub hidden: bool,
    pub key: Option<ItemTemplateId>,
    pub description: Option<String>,
}

impl Exit {
    pub fn simple(to: &str) -> Self {
        Exit { to: to.to_string(), locked: false, hidden: false, key: None, description: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcSpawn {
    pub template: NpcTemplateId,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemSpawn {
    pub template: ItemTemplateId,
    pub count: u32,
}

// ─── NPC Template ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcTemplate {
    pub id: NpcTemplateId,
    pub name: String,
    pub short_desc: String,
    pub long_desc: String,
    pub keywords: Vec<String>,
    pub level: u32,
    pub base_hp: i32,
    pub base_mp: i32,
    pub base_strength: u32,
    pub base_dexterity: u32,
    pub base_constitution: u32,
    #[serde(default)]
    pub race: String,
    pub faction: Option<String>,
    pub behavior: NpcBehavior,
    #[serde(default)]
    pub inventory: Vec<ItemTemplateId>,
    #[serde(default)]
    pub shop_items: Vec<ShopItem>,
    pub script: Option<String>,
    pub respawn_minutes: Option<u32>,
    #[serde(default)]
    pub dialogue: Vec<DialogueLine>,
    pub xp_reward: u32,
    pub coin_drop: Option<(u32, u32)>,  // (min, max) coins on death
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum NpcBehavior {
    Passive,
    Aggressive,
    Guard,
    Merchant,
    Wanderer,
    Sentinel,   // stays put, attacks if threatened
    Scripted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShopItem {
    pub template: ItemTemplateId,
    pub price: u32,
    pub stock: Option<u32>,  // None = infinite
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueLine {
    pub trigger: String,   // keyword player must say
    pub response: String,
    pub script_hook: Option<String>,  // optional Rhai hook to call
}

// ─── Item Template ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemTemplate {
    pub id: ItemTemplateId,
    pub name: String,
    pub short_desc: String,
    pub long_desc: String,
    pub keywords: Vec<String>,
    pub item_type: ItemType,
    pub weight: f32,
    pub value: u32,
    #[serde(default)]
    pub flags: ItemFlags,
    pub stats: Option<ItemStats>,
    pub consumable: Option<ConsumableEffect>,
    pub container_size: Option<u32>,
    pub script: Option<String>,
    #[serde(default)]
    pub craft_recipes: Vec<CraftRecipe>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ItemType {
    Weapon { damage_min: i32, damage_max: i32, weapon_type: String },
    Armor  { slot: EquipSlot, armor_class: i32 },
    Accessory { slot: EquipSlot },
    Consumable,
    Container,
    Key,
    Book   { content: String },
    Quest,
    Crafting,
    Coin,
    Misc,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EquipSlot {
    Head, Neck, Shoulders, Chest, Hands, Waist, Legs, Feet,
    FingerLeft, FingerRight, Wrist, Back,
    MainHand, OffHand, BothHands,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ItemFlags {
    #[serde(default)] pub no_drop: bool,
    #[serde(default)] pub no_give: bool,
    #[serde(default)] pub quest: bool,
    #[serde(default)] pub unique: bool,
    #[serde(default)] pub magic: bool,
    #[serde(default)] pub cursed: bool,
    #[serde(default)] pub glow: bool,
    #[serde(default)] pub hum: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemStats {
    pub str_bonus: i32,
    pub dex_bonus: i32,
    pub con_bonus: i32,
    pub int_bonus: i32,
    pub wis_bonus: i32,
    pub cha_bonus: i32,
    pub hp_bonus: i32,
    pub mp_bonus: i32,
    pub hit_bonus: i32,
    pub dam_bonus: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumableEffect {
    pub heal_hp: Option<i32>,
    pub heal_mp: Option<i32>,
    pub buff: Option<String>,
    pub buff_duration: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftRecipe {
    pub ingredients: Vec<ItemTemplateId>,
    pub result: ItemTemplateId,
    pub skill_required: Option<String>,
    pub skill_level: Option<u32>,
}

// ─── Area ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Area {
    pub id: AreaId,
    pub name: String,
    pub description: String,
    pub level_range: (u32, u32),
    pub theme: AreaTheme,
    #[serde(default)]
    pub rooms: HashMap<RoomId, Room>,
    #[serde(default)]
    pub npc_templates: HashMap<NpcTemplateId, NpcTemplate>,
    #[serde(default)]
    pub item_templates: HashMap<ItemTemplateId, ItemTemplate>,
    pub weather: Weather,
    pub weather_zone: bool,  // false = use global weather
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AreaTheme {
    Urban, Forest, Dungeon, Desert, Ocean, Mountain, Cave, Magical, Plains, Swamp,
}

// ─── TOML file format ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AreaFile {
    area: AreaMeta,
    #[serde(default)]
    rooms: Vec<RoomDef>,
    #[serde(default)]
    npc_templates: Vec<NpcTemplate>,
    #[serde(default)]
    item_templates: Vec<ItemTemplate>,
}

#[derive(Debug, Deserialize)]
struct AreaMeta {
    id: String,
    name: String,
    description: String,
    level_range: (u32, u32),
    theme: AreaTheme,
    #[serde(default = "default_weather")]
    weather: Weather,
    #[serde(default)]
    weather_zone: bool,
}

fn default_weather() -> Weather { Weather::Clear }

#[derive(Debug, Deserialize)]
struct RoomDef {
    id: String,
    name: String,
    description: String,
    #[serde(default)]
    descriptions: HashMap<String, String>,
    #[serde(default)]
    exits: HashMap<String, ExitDef>,
    #[serde(default)]
    flags: RoomFlags,
    script: Option<String>,
    #[serde(default)]
    lore: Vec<String>,
    spawn_npcs: Option<Vec<NpcSpawn>>,
    spawn_items: Option<Vec<ItemSpawn>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ExitDef {
    Simple(String),
    Full { to: String, #[serde(default)] locked: bool, #[serde(default)] hidden: bool, key: Option<String>, description: Option<String> },
}

impl ExitDef {
    fn into_exit(self) -> Exit {
        match self {
            ExitDef::Simple(to) => Exit::simple(&to),
            ExitDef::Full { to, locked, hidden, key, description } =>
                Exit { to, locked, hidden, key, description },
        }
    }
}

// ─── World ───────────────────────────────────────────────────────────────────

pub struct World {
    pub areas: HashMap<AreaId, Area>,
    pub npc_templates: HashMap<NpcTemplateId, NpcTemplate>,
    pub item_templates: HashMap<ItemTemplateId, ItemTemplate>,
}

impl World {
    pub fn load(world_path: &str) -> Result<Self> {
        let areas_path = Path::new(world_path).join("areas");
        let mut areas = HashMap::new();
        let mut npc_templates = HashMap::new();
        let mut item_templates = HashMap::new();

        let pattern = areas_path.join("*.toml").to_string_lossy().to_string();
        for entry in glob::glob(&pattern).context("glob failed")? {
            let path = entry.context("glob entry error")?;
            info!("Loading area: {:?}", path);
            match load_area_file(&path) {
                Ok((area, npcs, items)) => {
                    for (k, v) in npcs { npc_templates.insert(k, v); }
                    for (k, v) in items { item_templates.insert(k, v); }
                    areas.insert(area.id.clone(), area);
                }
                Err(e) => warn!("Failed to load {:?}: {}", path, e),
            }
        }

        if areas.is_empty() {
            warn!("No area files found — generating default world");
            let (area, npcs, items) = default_world();
            for (k, v) in npcs { npc_templates.insert(k, v); }
            for (k, v) in items { item_templates.insert(k, v); }
            areas.insert(area.id.clone(), area);
        }

        info!("Loaded {} areas", areas.len());
        Ok(World { areas, npc_templates, item_templates })
    }

    pub fn get_room(&self, room_id: &str) -> Option<&Room> {
        let (area_id, _) = room_id.split_once(':')?;
        self.areas.get(area_id)?.rooms.get(room_id)
    }

    pub fn get_room_mut(&mut self, room_id: &str) -> Option<&mut Room> {
        let (area_id, _) = room_id.split_once(':')?;
        self.areas.get_mut(area_id)?.rooms.get_mut(room_id)
    }

    pub fn get_npc_template(&self, id: &str) -> Option<&NpcTemplate> {
        // Check global templates first, then per-area
        if let Some(t) = self.npc_templates.get(id) {
            return Some(t);
        }
        // Search area-local templates
        let (area_id, _) = id.split_once(':').unwrap_or(("", id));
        self.areas.get(area_id)?.npc_templates.get(id)
    }

    pub fn get_item_template(&self, id: &str) -> Option<&ItemTemplate> {
        if let Some(t) = self.item_templates.get(id) {
            return Some(t);
        }
        let (area_id, _) = id.split_once(':').unwrap_or(("", id));
        self.areas.get(area_id)?.item_templates.get(id)
    }

}

// Used by lib (integration tests) and by main.rs's `#[cfg(test)]` block, but
// the binary's non-test build doesn't reference it directly.
#[allow(dead_code)]
pub fn parse_area_file_str(content: &str) -> Result<(Area, HashMap<String, NpcTemplate>, HashMap<String, ItemTemplate>)> {
    let file: AreaFile = toml::from_str(content)
        .with_context(|| "Parse error")?;
    parse_area_file_inner(file)
}

fn load_area_file(
    path: &Path,
) -> Result<(Area, HashMap<String, NpcTemplate>, HashMap<String, ItemTemplate>)> {
    let content = std::fs::read_to_string(path)?;
    let file: AreaFile = toml::from_str(&content)
        .with_context(|| format!("Parse error in {:?}", path))?;
    parse_area_file_inner(file)
}

fn parse_area_file_inner(
    file: AreaFile,
) -> Result<(Area, HashMap<String, NpcTemplate>, HashMap<String, ItemTemplate>)> {

    let area_id = file.area.id.clone();
    let mut rooms = HashMap::new();
    for rdef in file.rooms {
        let room_id = format!("{}:{}", area_id, rdef.id);
        let exits = rdef.exits.into_iter()
            .map(|(dir, edef)| (dir, edef.into_exit()))
            .collect();
        rooms.insert(room_id.clone(), Room {
            id: room_id,
            area: area_id.clone(),
            name: rdef.name,
            description: rdef.description,
            descriptions: rdef.descriptions,
            exits,
            items: vec![],
            npcs: vec![],
            flags: rdef.flags,
            script: rdef.script,
            history: vec![],
            lore: rdef.lore,
            spawn_npcs: rdef.spawn_npcs,
            spawn_items: rdef.spawn_items,
        });
    }

    let mut npc_templates = HashMap::new();
    for t in file.npc_templates {
        npc_templates.insert(t.id.clone(), t);
    }
    let mut item_templates = HashMap::new();
    for t in file.item_templates {
        item_templates.insert(t.id.clone(), t);
    }

    let area = Area {
        id: area_id.clone(),
        name: file.area.name,
        description: file.area.description,
        level_range: file.area.level_range,
        theme: file.area.theme,
        rooms,
        npc_templates: npc_templates.clone(),
        item_templates: item_templates.clone(),
        weather: file.area.weather,
        weather_zone: file.area.weather_zone,
    };

    Ok((area, npc_templates, item_templates))
}

/// Built-in minimal world when no files are present.
fn default_world() -> (Area, HashMap<String, NpcTemplate>, HashMap<String, ItemTemplate>) {
    use std::collections::HashMap;

    let area_id = "nexus";
    let mut rooms = HashMap::new();

    let mut add_room = |id: &str, name: &str, desc: &str, exits: Vec<(&str, &str)>| {
        let room_id = format!("{}:{}", area_id, id);
        let exits_map = exits.into_iter()
            .map(|(dir, to)| (dir.to_string(), Exit::simple(&format!("{}:{}", area_id, to))))
            .collect();
        rooms.insert(room_id.clone(), Room {
            id: room_id,
            area: area_id.to_string(),
            name: name.to_string(),
            description: desc.to_string(),
            descriptions: HashMap::new(),
            exits: exits_map,
            items: vec![],
            npcs: vec![],
            flags: RoomFlags::default(),
            script: None,
            history: vec![],
            lore: vec![],
            spawn_npcs: None,
            spawn_items: None,
        });
    };

    add_room("entrance", "The Grand Entrance",
        "You stand at the threshold of the Nexus, a shimmering convergence of \
         realities. Paths lead in every direction, and the air hums with \
         potential. Faint inscriptions line the archways overhead.",
        vec![("north", "hub"), ("east", "market"), ("west", "inn")]);

    add_room("hub", "The Central Hub",
        "A vast circular plaza forms the heart of the Nexus. Ancient \
         flagstones, worn smooth by countless footsteps, radiate outward from \
         a towering obelisk of black crystal that pulses with slow blue light.",
        vec![("south", "entrance"), ("north", "library"), ("east", "guild"), ("west", "temple")]);

    add_room("market", "The Bazaar",
        "Colorful stalls crowd this lively marketplace. Merchants hawk their \
         wares in a dozen languages. The smells of spiced food and exotic \
         goods mingle with the press of the crowd.",
        vec![("west", "entrance")]);

    add_room("inn", "The Wanderer's Rest",
        "A comfortable inn with low beams and a crackling fire. Mismatched \
         chairs surround sturdy wooden tables, and the barkeep eyes you with \
         practiced interest. A board near the entrance is covered in notices.",
        vec![("east", "entrance")]);

    add_room("library", "The Archive",
        "Floor-to-ceiling shelves groan under the weight of tomes written in \
         scripts from a hundred worlds. Motes of light drift between the stacks, \
         illuminating titles that shift and change as you watch.",
        vec![("south", "hub")]);

    add_room("guild", "The Adventurers' Guild",
        "Trophy heads and heraldic banners cover every wall. A bulletin board \
         bristles with quest notices. Battle-scarred veterans compare scars at \
         the bar, and a grim-faced registrar waits behind an oaken desk.",
        vec![("west", "hub")]);

    add_room("temple", "The Temple of Echoes",
        "Soft radiance fills this vaulted chamber. Dozens of shrines line the \
         walls, each dedicated to a different deity. The silence here is not \
         empty — it hums with presence.",
        vec![("east", "hub")]);

    let npc_templates: HashMap<String, NpcTemplate> = HashMap::new();
    let item_templates: HashMap<String, ItemTemplate> = HashMap::new();

    let area = Area {
        id: area_id.to_string(),
        name: "The Nexus".to_string(),
        description: "A shimmering hub where many realities converge.".to_string(),
        level_range: (1, 10),
        theme: AreaTheme::Urban,
        rooms,
        npc_templates: npc_templates.clone(),
        item_templates: item_templates.clone(),
        weather: Weather::Clear,
        weather_zone: false,
    };

    (area, npc_templates, item_templates)
}
