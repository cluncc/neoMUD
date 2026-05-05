use rand::Rng;

use crate::entity::{Stats, StatusEffect};

// ─── Hit / Damage Calculation ────────────────────────────────────────────────

pub fn roll_hit(attacker_stats: &Stats, defender_stats: &Stats, base_chance: u32) -> (bool, bool) {
    let mut rng = rand::thread_rng();
    let hit_bonus = attacker_stats.hit_bonus + attacker_stats.dexterity as i32 / 3;
    let defense = defender_stats.armor_class + defender_stats.dexterity as i32 / 4;
    let chance = (base_chance as i32 + hit_bonus - defense).clamp(5, 95) as u32;
    let roll = rng.gen_range(1..=100u32);
    let critical = roll <= 5;
    (roll <= chance || critical, critical)
}

pub fn roll_damage(
    attacker_stats: &Stats,
    weapon_min: i32,
    weapon_max: i32,
    critical: bool,
    skill_level: u32,
) -> i32 {
    let mut rng = rand::thread_rng();
    let base = rng.gen_range(weapon_min..=weapon_max.max(weapon_min + 1));
    let str_bonus = attacker_stats.strength as i32 / 3 - 2;
    let dam_bonus = attacker_stats.dam_bonus;
    let skill_bonus = (skill_level as i32 / 20).max(0);
    let total = (base + str_bonus + dam_bonus + skill_bonus).max(1);
    if critical { total * 2 } else { total }
}

pub fn apply_damage(defender_stats: &mut Stats, damage: i32) -> bool {
    defender_stats.hp -= damage;
    !defender_stats.is_alive()
}

/// Tick status effects on a combatant, returning any DoT damage and expired effect names.
pub fn tick_status_effects(effects: &mut Vec<StatusEffect>) -> (i32, Vec<String>) {
    let mut dot_damage = 0i32;
    let mut expired = vec![];

    effects.retain_mut(|effect| {
        match effect {
            StatusEffect::Poisoned { stacks, .. } => dot_damage += *stacks as i32 * 2,
            StatusEffect::Bleeding { severity, .. } => dot_damage += *severity as i32,
            StatusEffect::Burning  { .. } => dot_damage += 3,
            StatusEffect::Regenerating { hp_per_tick, .. } => dot_damage -= *hp_per_tick,
            _ => {}
        }
        let done = effect.tick();
        if done { expired.push(effect.name().to_string()); }
        !done
    });

    (dot_damage, expired)
}

// ─── Flee ────────────────────────────────────────────────────────────────────

pub fn attempt_flee(player_stats: &Stats, base_chance: u32) -> bool {
    let mut rng = rand::thread_rng();
    let bonus = player_stats.dexterity as i32 / 5;
    let chance = (base_chance as i32 + bonus).clamp(10, 80) as u32;
    rng.gen_range(1..=100) <= chance
}

// ─── XP Formula ──────────────────────────────────────────────────────────────

pub fn xp_for_kill(killer_level: u32, target_level: u32, base_xp: u32) -> u64 {
    let level_diff = target_level as i32 - killer_level as i32;
    let multiplier = match level_diff {
        5..  => 2.0,
        3..=4 => 1.5,
        1..=2 => 1.25,
        0 => 1.0,
        -2..=-1 => 0.75,
        -4..=-3 => 0.5,
        _ => 0.1,
    };
    ((base_xp as f64) * multiplier) as u64
}

// ─── Combat Message Templates ─────────────────────────────────────────────────

pub struct AttackMessages;

impl AttackMessages {
    pub fn hit_message(attacker: &str, defender: &str, damage: i32, weapon: &str, critical: bool) -> (String, String, String) {
        let mut rng = rand::thread_rng();
        let severity = match damage {
            1..=5 => 0,
            6..=15 => 1,
            16..=30 => 2,
            _ => 3,
        };
        let hit_words = [
            ["scratch", "clip", "graze", "tap"],
            ["hit", "strike", "slash", "cut"],
            ["slam", "smash", "drive", "pound"],
            ["devastate", "obliterate", "shatter", "maul"],
        ];
        let word = hit_words[severity][rng.gen_range(0..4)];
        let crit_prefix = if critical { "CRITICALLY " } else { "" };
        let _ = weapon; // retained for future weapon-specific messages

        let attacker_msg = format!(
            "You {}{} {} for {} damage!", crit_prefix, word, defender, damage
        );
        let defender_msg = format!(
            "{} {}{}s you for {} damage!", attacker, crit_prefix, word, damage
        );
        let room_msg = format!(
            "{} {}{}s {} for {} damage.", attacker, crit_prefix, word, defender, damage
        );
        (attacker_msg, defender_msg, room_msg)
    }

    pub fn miss_message(attacker: &str, defender: &str) -> (String, String, String) {
        let mut rng = rand::thread_rng();
        let misses = ["miss", "swing wide", "fail to connect", "graze only air"];
        let word = misses[rng.gen_range(0..misses.len())];
        (
            format!("You {} {}.", word, defender),
            format!("{} {}es at you but fails to connect.", attacker, word),
            format!("{} {}es at {} but misses.", attacker, word, defender),
        )
    }
}
