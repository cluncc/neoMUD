use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::world::{ItemTemplateId, NpcTemplateId, RoomId};

/// Returns a safe, lowercase filename component for a player name.
/// Rejects names containing anything other than ASCII alphanumerics (no path separators,
/// dots, slashes, null bytes, or other filesystem-hostile characters).
fn safe_name_for_path(name: &str) -> Option<String> {
    if name.is_empty() || name.len() > 32 {
        return None;
    }
    let lower = name.to_lowercase();
    if lower.chars().all(|c| c.is_ascii_alphanumeric()) {
        Some(lower)
    } else {
        None
    }
}

// ─── Stats ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub hp: i32,
    pub max_hp: i32,
    pub mp: i32,
    pub max_mp: i32,
    pub strength: u32,
    pub dexterity: u32,
    pub constitution: u32,
    pub intelligence: u32,
    pub wisdom: u32,
    pub charisma: u32,
    pub armor_class: i32,
    pub hit_bonus: i32,
    pub dam_bonus: i32,
    pub speed: u32,  // affects initiative, movement cost
}

impl Stats {
    pub fn for_class_race(class: &Class, race: &Race, level: u32) -> Self {
        let (str_b, dex_b, con_b, int_b, wis_b, cha_b) = race.stat_bonuses();
        let (str_c, dex_c, con_c, int_c, wis_c, cha_c) = class.primary_stats();
        let multiplier = 1 + level / 5;
        let hp_base = (8 + con_b as i32 + con_c as i32) * multiplier as i32;
        let mp_base = (4 + int_b as i32 + wis_b as i32 + int_c as i32) * multiplier as i32;
        Stats {
            hp: hp_base, max_hp: hp_base,
            mp: mp_base, max_mp: mp_base,
            strength: 10 + str_b + str_c,
            dexterity: 10 + dex_b + dex_c,
            constitution: 10 + con_b + con_c,
            intelligence: 10 + int_b + int_c,
            wisdom: 10 + wis_b + wis_c,
            charisma: 10 + cha_b + cha_c,
            armor_class: 10 + dex_b as i32 / 2,
            hit_bonus: level as i32 / 2,
            dam_bonus: (10 + str_b + str_c) as i32 / 5 - 2,
            speed: 10 + dex_b,
        }
    }

    pub fn is_alive(&self) -> bool { self.hp > 0 }

    #[allow(dead_code)]
    pub fn hp_percent(&self) -> u32 {
        if self.max_hp == 0 { return 0; }
        ((self.hp as f32 / self.max_hp as f32) * 100.0) as u32
    }

    #[allow(dead_code)]
    pub fn condition_string(&self) -> &'static str {
        match self.hp_percent() {
            91..=100 => "is in perfect health",
            76..=90 => "has a few scratches",
            51..=75 => "has some wounds",
            26..=50 => "is badly wounded",
            11..=25 => "is in critical condition",
            1..=10 => "is near death",
            _ => "is dead",
        }
    }
}

// ─── Race ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Race {
    Human,
    Elf,
    Dwarf,
    Halfling,
    Gnome,
    HalfElf,
    HalfOrc,
    Tiefling,
    Aasimar,
}

impl Race {
    /// (str, dex, con, int, wis, cha) bonuses
    pub fn stat_bonuses(&self) -> (u32, u32, u32, u32, u32, u32) {
        match self {
            Race::Human    => (1, 1, 1, 1, 1, 1),
            Race::Elf      => (0, 2, 0, 1, 1, 2),
            Race::Dwarf    => (1, 0, 3, 0, 1, 0),
            Race::Halfling => (0, 3, 1, 0, 0, 2),
            Race::Gnome    => (0, 1, 1, 3, 0, 1),
            Race::HalfElf  => (0, 1, 0, 1, 1, 3),
            Race::HalfOrc  => (3, 0, 2, 0, 0, 0),
            Race::Tiefling => (0, 1, 0, 1, 0, 2),
            Race::Aasimar  => (0, 0, 1, 0, 1, 3),
        }
    }

    pub fn all() -> &'static [Race] {
        &[
            Race::Human, Race::Elf, Race::Dwarf, Race::Halfling,
            Race::Gnome, Race::HalfElf, Race::HalfOrc, Race::Tiefling, Race::Aasimar,
        ]
    }

    pub fn description(&self) -> &str {
        match self {
            Race::Human    => "Versatile and adaptable, humans excel in all fields.",
            Race::Elf      => "Graceful and long-lived, elves are masters of magic and archery.",
            Race::Dwarf    => "Stalwart and resilient, dwarves excel in combat and crafting.",
            Race::Halfling => "Quick and nimble, halflings are natural rogues and wanderers.",
            Race::Gnome    => "Curious and inventive, gnomes are gifted with illusion and artifice.",
            Race::HalfElf  => "Blending human ambition with elven grace, half-elves are natural diplomats.",
            Race::HalfOrc  => "Raw power and fierce determination define the half-orc.",
            Race::Tiefling => "Touched by infernal heritage, tieflings are charismatic and cunning.",
            Race::Aasimar  => "Blessed by celestial blood, aasimar radiate divine grace.",
        }
    }
}

impl std::fmt::Display for Race {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

// ─── Class ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Class {
    Warrior,
    Mage,
    Rogue,
    Cleric,
    Ranger,
    Paladin,
    Druid,
    Bard,
}

impl Class {
    /// (str, dex, con, int, wis, cha) focus bonuses
    pub fn primary_stats(&self) -> (u32, u32, u32, u32, u32, u32) {
        match self {
            Class::Warrior => (3, 1, 2, 0, 0, 0),
            Class::Mage    => (0, 0, 0, 4, 2, 0),
            Class::Rogue   => (1, 4, 0, 1, 0, 0),
            Class::Cleric  => (1, 0, 1, 1, 4, 0),
            Class::Ranger  => (1, 3, 1, 1, 0, 0),
            Class::Paladin => (2, 0, 1, 0, 1, 2),
            Class::Druid   => (0, 1, 1, 0, 4, 0),
            Class::Bard    => (0, 2, 0, 1, 0, 3),
        }
    }

    pub fn all() -> &'static [Class] {
        &[
            Class::Warrior, Class::Mage, Class::Rogue, Class::Cleric,
            Class::Ranger, Class::Paladin, Class::Druid, Class::Bard,
        ]
    }

    pub fn description(&self) -> &str {
        match self {
            Class::Warrior => "Masters of arms and armor, warriors excel in direct combat.",
            Class::Mage    => "Wielders of arcane power, mages reshape reality with spells.",
            Class::Rogue   => "Shadows and cunning define the rogue — strikes from nowhere.",
            Class::Cleric  => "Divine champions who heal allies and smite the undead.",
            Class::Ranger  => "Wilderness hunters skilled in tracking, archery, and survival.",
            Class::Paladin => "Holy warriors who combine martial skill with divine blessing.",
            Class::Druid   => "Servants of nature who command the elements and beasts.",
            Class::Bard    => "Performers and wanderers whose music shapes fate itself.",
        }
    }

    pub fn starting_skills(&self) -> Vec<&'static str> {
        match self {
            Class::Warrior => vec!["sword", "shield", "armor", "bash"],
            Class::Mage    => vec!["magic", "spellcraft", "arcana", "staff"],
            Class::Rogue   => vec!["stealth", "dagger", "pick_locks", "backstab"],
            Class::Cleric  => vec!["mace", "holy", "healing", "shields"],
            Class::Ranger  => vec!["bow", "tracking", "survival", "dual_wield"],
            Class::Paladin => vec!["sword", "holy", "armor", "heal"],
            Class::Druid   => vec!["nature", "wild_shape", "herbs", "staff"],
            Class::Bard    => vec!["music", "charm", "dagger", "lore"],
        }
    }
}

impl std::fmt::Display for Class {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

// ─── Skill ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub level: u32,      // 1-100
    pub uses: u64,       // total uses (for advancement)
    pub mastery: SkillMastery,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SkillMastery {
    Novice,
    Apprentice,
    Journeyman,
    Expert,
    Master,
    Grandmaster,
}

impl Skill {
    pub fn new(name: &str) -> Self {
        Skill { name: name.to_string(), level: 1, uses: 0, mastery: SkillMastery::Novice }
    }

    /// Use the skill; may level up. Returns true if level increased.
    #[allow(dead_code)]
    pub fn use_skill(&mut self) -> bool {
        self.uses += 1;
        let threshold = (self.level as u64 * self.level as u64) * 10 + 100;
        if self.uses % threshold == 0 && self.level < 100 {
            self.level += 1;
            self.mastery = match self.level {
                1..=15 => SkillMastery::Novice,
                16..=30 => SkillMastery::Apprentice,
                31..=50 => SkillMastery::Journeyman,
                51..=70 => SkillMastery::Expert,
                71..=90 => SkillMastery::Master,
                _ => SkillMastery::Grandmaster,
            };
            return true;
        }
        false
    }

    #[allow(dead_code)]
    pub fn effectiveness(&self) -> f32 {
        self.level as f32 / 100.0
    }
}

// ─── Item Instance ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemInstance {
    pub instance_id: String,
    pub template_id: ItemTemplateId,
    pub name: String,
    pub custom_desc: Option<String>,   // player-authored content
    #[serde(default)]
    pub charges: Option<u32>,
    #[serde(default)]
    pub worn: bool,
    pub durability: Option<u32>,
    #[serde(default)]
    pub contents: Vec<ItemInstance>,  // for containers
}

impl ItemInstance {
    pub fn new(template_id: &str, name: &str) -> Self {
        ItemInstance {
            instance_id: Uuid::new_v4().to_string(),
            template_id: template_id.to_string(),
            name: name.to_string(),
            custom_desc: None,
            charges: None,
            worn: false,
            durability: None,
            contents: vec![],
        }
    }
}

// ─── Equipment ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Equipment {
    pub slots: HashMap<String, ItemInstance>,
}

impl Equipment {
    pub fn equip(&mut self, slot: &str, item: ItemInstance) -> Option<ItemInstance> {
        self.slots.insert(slot.to_string(), item)
    }

    pub fn unequip(&mut self, slot: &str) -> Option<ItemInstance> {
        self.slots.remove(slot)
    }

    #[allow(dead_code)]
    pub fn armor_bonus(&self) -> i32 {
        self.slots.len() as i32 * 2
    }
}

// ─── Status Effects ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StatusEffect {
    Poisoned    { stacks: u32, ticks_left: u32 },
    Bleeding    { severity: u32, ticks_left: u32 },
    Stunned     { ticks_left: u32 },
    Blinded     { ticks_left: u32 },
    Burning     { ticks_left: u32 },
    Frozen      { ticks_left: u32 },
    Hasted      { ticks_left: u32 },
    Slowed      { ticks_left: u32 },
    Invisible   { ticks_left: u32 },
    Regenerating{ hp_per_tick: i32, ticks_left: u32 },
    Protected   { bonus: i32, ticks_left: u32 },
    Cursed      { effect: String, ticks_left: u32 },
}

impl StatusEffect {
    pub fn name(&self) -> &str {
        match self {
            StatusEffect::Poisoned    { .. } => "poisoned",
            StatusEffect::Bleeding    { .. } => "bleeding",
            StatusEffect::Stunned     { .. } => "stunned",
            StatusEffect::Blinded     { .. } => "blinded",
            StatusEffect::Burning     { .. } => "burning",
            StatusEffect::Frozen      { .. } => "frozen",
            StatusEffect::Hasted      { .. } => "hasted",
            StatusEffect::Slowed      { .. } => "slowed",
            StatusEffect::Invisible   { .. } => "invisible",
            StatusEffect::Regenerating{ .. } => "regenerating",
            StatusEffect::Protected   { .. } => "protected",
            StatusEffect::Cursed      { .. } => "cursed",
        }
    }

    pub fn tick(&mut self) -> bool {
        // Returns true if expired
        match self {
            StatusEffect::Poisoned    { ticks_left, .. }
            | StatusEffect::Bleeding  { ticks_left, .. }
            | StatusEffect::Stunned   { ticks_left }
            | StatusEffect::Blinded   { ticks_left }
            | StatusEffect::Burning   { ticks_left }
            | StatusEffect::Frozen    { ticks_left }
            | StatusEffect::Hasted    { ticks_left }
            | StatusEffect::Slowed    { ticks_left }
            | StatusEffect::Invisible { ticks_left }
            | StatusEffect::Regenerating { ticks_left, .. }
            | StatusEffect::Protected { ticks_left, .. }
            | StatusEffect::Cursed    { ticks_left, .. } => {
                if *ticks_left == 0 { return true; }
                *ticks_left -= 1;
                *ticks_left == 0
            }
        }
    }
}

// ─── NPC Memory ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcMemory {
    pub player_name: String,
    pub last_interaction: i64,    // unix timestamp
    pub interaction_count: u32,
    pub sentiment: i32,           // -100 hostile .. 100 friendly
    pub notes: Vec<String>,       // what the NPC remembers about this player
}

// ─── Active NPC ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveNpc {
    pub instance_id: String,
    pub template_id: NpcTemplateId,
    pub name: String,
    pub room: RoomId,
    pub stats: Stats,
    pub inventory: Vec<ItemInstance>,
    pub faction: Option<String>,
    pub alive: bool,
    pub in_combat_with: Option<String>,  // player name
    pub memory: Vec<NpcMemory>,
    pub status_effects: Vec<StatusEffect>,
    pub respawn_at_tick: Option<u64>,
}

impl ActiveNpc {
    pub fn memory_of(&self, player: &str) -> Option<&NpcMemory> {
        self.memory.iter().find(|m| m.player_name == player)
    }

    #[allow(dead_code)]
    pub fn update_memory(&mut self, player: &str, sentiment_delta: i32, note: Option<String>) {
        let now = chrono::Utc::now().timestamp();
        if let Some(mem) = self.memory.iter_mut().find(|m| m.player_name == player) {
            mem.last_interaction = now;
            mem.interaction_count += 1;
            mem.sentiment = (mem.sentiment + sentiment_delta).clamp(-100, 100);
            if let Some(n) = note { mem.notes.push(n); if mem.notes.len() > 5 { mem.notes.remove(0); } }
        } else {
            let mut notes = vec![];
            if let Some(n) = note { notes.push(n); }
            self.memory.push(NpcMemory {
                player_name: player.to_string(),
                last_interaction: now,
                interaction_count: 1,
                sentiment: sentiment_delta.clamp(-100, 100),
                notes,
            });
        }
    }
}

// ─── Player ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub name: String,
    pub password_hash: String,
    pub room: RoomId,
    pub stats: Stats,
    pub race: Race,
    pub class: Class,
    pub level: u32,
    pub experience: u64,
    pub xp_to_next: u64,
    pub inventory: Vec<ItemInstance>,
    pub equipment: Equipment,
    pub skills: HashMap<String, Skill>,
    pub coins: u32,
    pub faction_rep: HashMap<String, i32>,   // faction_id → reputation (-1000..1000)
    pub aliases: HashMap<String, String>,
    pub title: String,
    pub description: String,
    pub status_effects: Vec<StatusEffect>,
    pub in_combat_with: Option<String>,       // NPC instance id
    pub is_admin: bool,
    pub created_at: i64,
    pub last_login: i64,
    pub play_time_seconds: u64,
    pub deaths: u32,
    pub kills: u32,
    #[serde(default)]
    pub quest_flags: HashMap<String, bool>,
    #[serde(default)]
    pub known_recipes: Vec<String>,
}

impl Player {
    pub fn new(name: &str, password_hash: &str, race: Race, class: Class, start_room: &str) -> Self {
        let level = 1;
        let stats = Stats::for_class_race(&class, &race, level);
        let skills = class.starting_skills().iter()
            .map(|s| (s.to_string(), Skill::new(s)))
            .collect();
        let now = chrono::Utc::now().timestamp();
        Player {
            name: name.to_string(),
            password_hash: password_hash.to_string(),
            room: start_room.to_string(),
            stats,
            race,
            class,
            level,
            experience: 0,
            xp_to_next: 1000,
            inventory: vec![],
            equipment: Equipment::default(),
            skills,
            coins: 50,
            faction_rep: HashMap::new(),
            aliases: HashMap::new(),
            title: String::new(),
            description: format!("{} looks like a typical adventurer.", name),
            status_effects: vec![],
            in_combat_with: None,
            is_admin: false,
            created_at: now,
            last_login: now,
            play_time_seconds: 0,
            deaths: 0,
            kills: 0,
            quest_flags: HashMap::new(),
            known_recipes: vec![],
        }
    }

    pub fn gain_xp(&mut self, amount: u64) -> bool {
        self.experience += amount;
        if self.experience >= self.xp_to_next {
            self.level_up();
            return true;
        }
        false
    }

    fn level_up(&mut self) {
        self.level += 1;
        self.experience -= self.xp_to_next;
        self.xp_to_next = (self.level as u64 * 1000) + (self.level as u64).pow(2) * 100;
        // Increase HP/MP
        let hp_gain = 5 + self.stats.constitution as i32 / 3;
        let mp_gain = 3 + self.stats.intelligence as i32 / 3;
        self.stats.max_hp += hp_gain;
        self.stats.hp = self.stats.max_hp;
        self.stats.max_mp += mp_gain;
        self.stats.mp = self.stats.max_mp;
        self.stats.hit_bonus += 1;
    }

    pub fn find_item(&self, keyword: &str) -> Option<&ItemInstance> {
        let kw = keyword.to_lowercase();
        self.inventory.iter().find(|i| {
            i.name.to_lowercase().contains(&kw) ||
            i.template_id.to_lowercase().contains(&kw)
        })
    }

    pub fn take_item(&mut self, keyword: &str) -> Option<ItemInstance> {
        let kw = keyword.to_lowercase();
        let pos = self.inventory.iter().position(|i| {
            i.name.to_lowercase().contains(&kw) ||
            i.template_id.to_lowercase().contains(&kw)
        })?;
        Some(self.inventory.remove(pos))
    }

    pub fn reputation_standing(&self, faction: &str) -> ReputationStanding {
        let rep = self.faction_rep.get(faction).copied().unwrap_or(0);
        match rep {
            751..=1000 => ReputationStanding::Exalted,
            501..=750  => ReputationStanding::Revered,
            251..=500  => ReputationStanding::Honored,
            1..=250    => ReputationStanding::Friendly,
            -250..=-1  => ReputationStanding::Unfriendly,
            -500..=-251 => ReputationStanding::Hostile,
            _ if rep <= -501 => ReputationStanding::Hated,
            _ => ReputationStanding::Neutral,
        }
    }

    pub fn adjust_reputation(&mut self, faction: &str, amount: i32) {
        let rep = self.faction_rep.entry(faction.to_string()).or_insert(0);
        *rep = (*rep + amount).clamp(-1000, 1000);
    }

    pub fn is_in_combat(&self) -> bool {
        self.in_combat_with.is_some()
    }

    pub fn save(&self, path: &str) -> anyhow::Result<()> {
        let safe = safe_name_for_path(&self.name)
            .ok_or_else(|| anyhow::anyhow!("Invalid player name: {}", self.name))?;
        let file = format!("{}/{}.json", path, safe);
        // Write atomically via temp file to prevent partial writes
        let tmp = format!("{}.tmp", file);
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, &file)?;
        Ok(())
    }

    pub fn load(path: &str, name: &str) -> anyhow::Result<Self> {
        let safe = safe_name_for_path(name)
            .ok_or_else(|| anyhow::anyhow!("Invalid player name: {}", name))?;
        let file = format!("{}/{}.json", path, safe);
        let content = std::fs::read_to_string(&file)?;
        let player: Player = serde_json::from_str(&content)?;
        // Verify loaded name matches requested name (case-insensitive) to prevent spoofing
        if player.name.to_lowercase() != name.to_lowercase() {
            anyhow::bail!("Player name mismatch in save file");
        }
        Ok(player)
    }

    pub fn exists(path: &str, name: &str) -> bool {
        match safe_name_for_path(name) {
            Some(safe) => std::path::Path::new(&format!("{}/{}.json", path, safe)).exists(),
            None => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReputationStanding {
    Hated,
    Hostile,
    Unfriendly,
    Neutral,
    Friendly,
    Honored,
    Revered,
    Exalted,
}

impl std::fmt::Display for ReputationStanding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
