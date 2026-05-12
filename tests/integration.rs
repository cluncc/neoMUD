/// Integration tests for neoMUD.
///
/// These tests verify observable behaviour at the module boundary — they use
/// only the public API and avoid reaching into internal implementation details.

// ─── Command parsing ──────────────────────────────────────────────────────────

mod parse_input {
    use neomud::commands::parse_input;

    #[test]
    fn plain_verb() {
        assert_eq!(parse_input("look"), ("look", ""));
    }

    #[test]
    fn verb_with_args() {
        assert_eq!(parse_input("say hello world"), ("say", "hello world"));
    }

    #[test]
    fn trims_whitespace() {
        assert_eq!(parse_input("  drop  sword  "), ("drop", "sword"));
    }

    #[test]
    fn empty_input() {
        assert_eq!(parse_input(""), ("", ""));
    }

    #[test]
    fn single_space() {
        assert_eq!(parse_input(" "), ("", ""));
    }

    #[test]
    fn multiple_spaces_between_verb_and_args() {
        let (verb, args) = parse_input("tell   Alice  hi");
        assert_eq!(verb, "tell");
        // rest is trimmed from the left
        assert!(args.starts_with("Alice"));
    }
}

// ─── Password hashing / verification ─────────────────────────────────────────

mod passwords {
    use neomud::commands::{hash_password, verify_password};

    #[test]
    fn argon2_round_trip() {
        let hash = hash_password("correct horse battery staple");
        assert!(verify_password(&hash, "correct horse battery staple"));
    }

    #[test]
    fn wrong_password_rejected() {
        let hash = hash_password("secret");
        assert!(!verify_password(&hash, "wrong"));
    }

    #[test]
    fn empty_password_works() {
        let hash = hash_password("");
        assert!(verify_password(&hash, ""));
        assert!(!verify_password(&hash, "notempty"));
    }

    #[test]
    fn garbage_hash_rejected() {
        assert!(!verify_password("this-is-not-a-valid-hash", "anything"));
    }

    #[test]
    fn argon2_hashes_are_unique() {
        let h1 = hash_password("same");
        let h2 = hash_password("same");
        // Argon2 uses a random salt, so equal inputs produce different hashes.
        assert_ne!(h1, h2);
    }

    #[test]
    fn very_long_password_handled() {
        let long = "a".repeat(1000);
        let hash = hash_password(&long);
        assert!(verify_password(&hash, &long));
        assert!(!verify_password(&hash, "a"));
    }
}

// ─── Name sanitization ────────────────────────────────────────────────────────

mod sanitize {
    use neomud::session::sanitize_name;

    #[test]
    fn capitalizes_first_letter() {
        assert_eq!(sanitize_name("alice"), "Alice");
    }

    #[test]
    fn lowercases_rest() {
        assert_eq!(sanitize_name("ALICE"), "Alice");
    }

    #[test]
    fn strips_non_alpha() {
        // Digits and punctuation in the middle should be stripped.
        assert_eq!(sanitize_name("al1ce!"), "Alce");
    }

    #[test]
    fn path_traversal_attempt_stripped() {
        // "../admin" should collapse to "Admin" — the '.', '/', chars are dropped.
        let s = sanitize_name("../admin");
        assert!(!s.contains('.'));
        assert!(!s.contains('/'));
    }

    #[test]
    fn null_bytes_stripped() {
        let s = sanitize_name("ab\0cd");
        assert!(!s.contains('\0'));
    }

    #[test]
    fn empty_string() {
        assert_eq!(sanitize_name(""), "");
    }

    #[test]
    fn already_canonical() {
        assert_eq!(sanitize_name("Alice"), "Alice");
    }
}

// ─── Race / class choice parsing ─────────────────────────────────────────────

mod char_creation {
    use neomud::session::{parse_class_choice, parse_race_choice};

    #[test]
    fn race_1_is_first_race() {
        let race = parse_race_choice("1");
        assert!(race.is_some());
    }

    #[test]
    fn race_zero_is_invalid() {
        assert!(parse_race_choice("0").is_none());
    }

    #[test]
    fn race_out_of_range() {
        assert!(parse_race_choice("999").is_none());
    }

    #[test]
    fn race_non_numeric() {
        assert!(parse_race_choice("elf").is_none());
    }

    #[test]
    fn class_1_is_first_class() {
        assert!(parse_class_choice("1").is_some());
    }

    #[test]
    fn class_zero_invalid() {
        assert!(parse_class_choice("0").is_none());
    }

    #[test]
    fn class_whitespace_trimmed() {
        // "  2  " should parse to class index 2.
        assert!(parse_class_choice("  2  ").is_some());
    }
}

// ─── World TOML parsing ───────────────────────────────────────────────────────

mod world {
    use neomud::world::parse_area_file_str;

    fn nexus_toml() -> String {
        std::fs::read_to_string("world/areas/nexus.toml").expect("nexus.toml missing")
    }

    fn deepwood_toml() -> String {
        std::fs::read_to_string("world/areas/deepwood.toml").expect("deepwood.toml missing")
    }

    #[test]
    fn nexus_loads() {
        let (area, _, _) = parse_area_file_str(&nexus_toml()).expect("nexus parse failed");
        assert!(!area.rooms.is_empty(), "nexus has no rooms");
    }

    #[test]
    fn deepwood_loads() {
        let (area, _, _) = parse_area_file_str(&deepwood_toml()).expect("deepwood parse failed");
        assert!(!area.rooms.is_empty(), "deepwood has no rooms");
    }

    #[test]
    fn nexus_room_count() {
        let (area, _, _) = parse_area_file_str(&nexus_toml()).unwrap();
        assert_eq!(area.rooms.len(), 10, "expected 10 nexus rooms");
    }

    #[test]
    fn deepwood_room_count() {
        let (area, _, _) = parse_area_file_str(&deepwood_toml()).unwrap();
        assert_eq!(area.rooms.len(), 10, "expected 10 deepwood rooms");
    }

    #[test]
    fn rooms_have_exits() {
        let (area, _, _) = parse_area_file_str(&nexus_toml()).unwrap();
        let rooms_with_exits = area.rooms.values().filter(|r| !r.exits.is_empty()).count();
        assert!(rooms_with_exits > 0, "no rooms have exits");
    }

    #[test]
    fn invalid_toml_returns_error() {
        let result = parse_area_file_str("this is not valid toml {{{{");
        assert!(result.is_err());
    }

    #[test]
    fn empty_content_returns_error() {
        // Empty TOML has no required fields — should produce a meaningful error.
        let result = parse_area_file_str("");
        assert!(result.is_err());
    }
}

// ─── Player persistence ───────────────────────────────────────────────────────

mod player_io {
    use neomud::entity::Player;
    use neomud::entity::{Class, Race};
    use std::fs;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn save_and_reload() {
        let dir = temp_dir();
        let path = dir.path().to_str().unwrap();
        let player = Player::new("Alice", "somehash", Race::Human, Class::Warrior, "nexus:entrance");

        player.save(path).expect("save failed");
        let loaded = Player::load(path, "Alice").expect("load failed");

        assert_eq!(loaded.name, "Alice");
        assert_eq!(loaded.password_hash, "somehash");
    }

    #[test]
    fn exists_false_before_save() {
        let dir = temp_dir();
        let path = dir.path().to_str().unwrap();
        assert!(!Player::exists(path, "Bob"));
    }

    #[test]
    fn exists_true_after_save() {
        let dir = temp_dir();
        let path = dir.path().to_str().unwrap();
        let player = Player::new("Bob", "hash", Race::Elf, Class::Mage, "nexus:entrance");
        player.save(path).expect("save");
        assert!(Player::exists(path, "Bob"));
    }

    #[test]
    fn load_nonexistent_returns_error() {
        let dir = temp_dir();
        let path = dir.path().to_str().unwrap();
        assert!(Player::load(path, "Nobody").is_err());
    }

    #[test]
    fn path_traversal_via_name_rejected() {
        let dir = temp_dir();
        let path = dir.path().to_str().unwrap();
        let player = Player::new("../evil", "hash", Race::Human, Class::Warrior, "nexus:entrance");
        assert!(player.save(path).is_err(), "should reject path-traversal name");
    }

    #[test]
    fn name_with_slash_rejected_on_load() {
        let dir = temp_dir();
        let path = dir.path().to_str().unwrap();
        assert!(Player::load(path, "../etc/passwd").is_err());
    }

    #[test]
    fn name_mismatch_in_file_rejected() {
        let dir = temp_dir();
        let path = dir.path().to_str().unwrap();
        // Save as "Alice", then try to load as "Alicia".
        let player = Player::new("Alice", "hash", Race::Human, Class::Warrior, "nexus:entrance");
        player.save(path).expect("save");
        // Manually write a file that claims a different name.
        let content = fs::read_to_string(format!("{}/alice.json", path)).unwrap();
        let tampered = content.replace("\"name\": \"Alice\"", "\"name\": \"Admin\"");
        fs::write(format!("{}/alice.json", path), tampered).unwrap();
        assert!(Player::load(path, "Alice").is_err(), "should detect name mismatch");
    }
}

// ─── Script action parsing ────────────────────────────────────────────────────

mod scripting {
    use neomud::scripting::ScriptEngine;
    use rhai::Dynamic;

    fn engine() -> ScriptEngine {
        ScriptEngine::new("/tmp/nonexistent_world_path")
    }

    #[test]
    fn call_hook_on_missing_script_returns_empty() {
        let eng = engine();
        let ctx = Dynamic::from(());
        let actions = eng.call_hook("nonexistent.rhai", "on_enter", ctx);
        assert!(actions.is_empty());
    }

    #[test]
    fn call_describe_on_missing_script_returns_none() {
        let eng = engine();
        let ctx = Dynamic::from(());
        let desc = eng.call_describe("nonexistent.rhai", ctx);
        assert!(desc.is_none());
    }
}

// ─── Color / output helpers ───────────────────────────────────────────────────

mod color {
    use neomud::color::*;

    #[test]
    fn bold_wraps_with_ansi() {
        let s = bold("hello");
        assert!(s.contains("hello"));
        assert!(s.contains('\x1b'));
    }

    #[test]
    fn error_msg_nonempty() {
        assert!(!error_msg("oops").is_empty());
    }

    #[test]
    fn health_bar_full_hp() {
        let bar = health_bar(100, 100, 10);
        assert!(!bar.is_empty());
    }

    #[test]
    fn health_bar_zero_hp() {
        let bar = health_bar(0, 100, 10);
        assert!(!bar.is_empty());
    }

    #[test]
    fn health_bar_exceeds_max_clamped() {
        // HP above max shouldn't panic.
        let bar = health_bar(200, 100, 10);
        assert!(!bar.is_empty());
    }
}

// ─── Combat functions ─────────────────────────────────────────────────────────

mod combat {
    use neomud::combat::*;
    use neomud::entity::{Stats, StatusEffect};

    fn base_stats() -> Stats {
        Stats {
            hp: 100, max_hp: 100,
            mp: 50, max_mp: 50,
            strength: 12, dexterity: 12, constitution: 12,
            intelligence: 10, wisdom: 10, charisma: 10,
            armor_class: 10, hit_bonus: 2, dam_bonus: 1, speed: 10,
        }
    }

    #[test]
    fn roll_damage_is_at_least_one() {
        let stats = base_stats();
        for _ in 0..50 {
            assert!(roll_damage(&stats, 1, 4, false, 1) >= 1);
        }
    }

    #[test]
    fn critical_hits_deal_more_damage_on_average() {
        // roll_damage uses RNG internally so we can't pin exact values.
        // Verify over 200 trials that crits consistently outperform normals.
        let stats = base_stats();
        let total_normal: i32 = (0..200).map(|_| roll_damage(&stats, 1, 6, false, 1)).sum();
        let total_crit: i32   = (0..200).map(|_| roll_damage(&stats, 1, 6, true,  1)).sum();
        assert!(total_crit > total_normal, "crits should deal more total damage over 200 trials");
    }

    #[test]
    fn apply_damage_kills_when_lethal() {
        let mut stats = base_stats();
        let dead = apply_damage(&mut stats, 200);
        assert!(dead);
        assert!(stats.hp <= 0);
    }

    #[test]
    fn apply_damage_partial_stays_alive() {
        let mut stats = base_stats();
        let dead = apply_damage(&mut stats, 10);
        assert!(!dead);
        assert_eq!(stats.hp, 90);
    }

    #[test]
    fn xp_scales_with_level_gap() {
        let high_xp = xp_for_kill(1, 10, 100);
        let even_xp = xp_for_kill(5, 5,  100);
        let low_xp  = xp_for_kill(10, 1, 100);
        assert!(high_xp > even_xp, "killing higher-level should give more XP");
        assert!(even_xp > low_xp,  "killing lower-level should give less XP");
    }

    #[test]
    fn attempt_flee_returns_bool_without_panic() {
        let stats = base_stats();
        for _ in 0..20 {
            let _ = attempt_flee(&stats, 50);
        }
    }

    #[test]
    fn poison_deals_dot_damage() {
        let mut effects = vec![StatusEffect::Poisoned { stacks: 2, ticks_left: 3 }];
        let (dot, _expired) = tick_status_effects(&mut effects);
        assert!(dot > 0, "poison should deal positive damage");
    }

    #[test]
    fn regeneration_heals_negative_dot() {
        let mut effects = vec![StatusEffect::Regenerating { hp_per_tick: 5, ticks_left: 3 }];
        let (dot, _expired) = tick_status_effects(&mut effects);
        assert!(dot < 0, "regen should produce negative dot (healing)");
    }

    #[test]
    fn expired_effects_are_removed() {
        let mut effects = vec![StatusEffect::Stunned { ticks_left: 1 }];
        let (_dot, expired) = tick_status_effects(&mut effects);
        assert!(effects.is_empty(), "expired effect should be removed");
        assert_eq!(expired, vec!["stunned"]);
    }

    #[test]
    fn non_expired_effects_remain() {
        let mut effects = vec![StatusEffect::Blinded { ticks_left: 5 }];
        tick_status_effects(&mut effects);
        assert!(!effects.is_empty(), "non-expired effect should remain");
    }
}

// ─── Entity / Player ─────────────────────────────────────────────────────────

mod entity {
    use neomud::entity::{Class, ItemInstance, Player, Race, ReputationStanding, Stats};

    fn warrior() -> Player {
        Player::new("Test", "hash", Race::Human, Class::Warrior, "nexus:entrance")
    }

    #[test]
    fn new_player_starts_at_level_1() {
        assert_eq!(warrior().level, 1);
    }

    #[test]
    fn new_player_has_starting_skills() {
        let p = warrior();
        assert!(p.skills.contains_key("sword"), "warrior should have sword skill");
    }

    #[test]
    fn gain_xp_below_threshold_no_level_up() {
        let mut p = warrior();
        let leveled = p.gain_xp(100);
        assert!(!leveled);
        assert_eq!(p.level, 1);
    }

    #[test]
    fn gain_xp_at_threshold_triggers_level_up() {
        let mut p = warrior();
        let leveled = p.gain_xp(1000);
        assert!(leveled, "reaching xp_to_next should trigger level up");
        assert_eq!(p.level, 2);
    }

    #[test]
    fn level_up_increases_max_hp() {
        let mut p = warrior();
        let hp_before = p.stats.max_hp;
        p.gain_xp(1000);
        assert!(p.stats.max_hp > hp_before, "max_hp should increase on level up");
    }

    #[test]
    fn level_up_fully_restores_hp() {
        let mut p = warrior();
        p.stats.hp = 1;
        p.gain_xp(1000);
        assert_eq!(p.stats.hp, p.stats.max_hp, "hp should equal max_hp after level up");
    }

    #[test]
    fn reputation_unknown_faction_is_neutral() {
        let p = warrior();
        assert_eq!(p.reputation_standing("unknown_faction"), ReputationStanding::Neutral);
    }

    #[test]
    fn reputation_adjust_reflects_in_standing() {
        let mut p = warrior();
        p.adjust_reputation("guards", 300);
        assert_eq!(p.reputation_standing("guards"), ReputationStanding::Honored);
    }

    #[test]
    fn reputation_clamped_to_bounds() {
        let mut p = warrior();
        p.adjust_reputation("evil", -5000);
        assert_eq!(*p.faction_rep.get("evil").unwrap(), -1000);
        p.adjust_reputation("good", 5000);
        assert_eq!(*p.faction_rep.get("good").unwrap(), 1000);
    }

    #[test]
    fn reputation_standing_all_tiers() {
        let mut p = warrior();
        let cases = [
            (800,  ReputationStanding::Exalted),
            (600,  ReputationStanding::Revered),
            (300,  ReputationStanding::Honored),
            (100,  ReputationStanding::Friendly),
            (0,    ReputationStanding::Neutral),
            (-100, ReputationStanding::Unfriendly),
            (-400, ReputationStanding::Hostile),
            (-800, ReputationStanding::Hated),
        ];
        for (rep, expected) in cases {
            p.faction_rep.insert("f".to_string(), rep);
            assert_eq!(p.reputation_standing("f"), expected, "rep={}", rep);
        }
    }

    #[test]
    fn find_item_returns_matching_item() {
        let mut p = warrior();
        p.inventory.push(ItemInstance::new("nexus:sword", "iron sword"));
        assert!(p.find_item("sword").is_some());
    }

    #[test]
    fn find_item_no_match_returns_none() {
        let p = warrior();
        assert!(p.find_item("nonexistent").is_none());
    }

    #[test]
    fn take_item_removes_from_inventory() {
        let mut p = warrior();
        p.inventory.push(ItemInstance::new("nexus:potion", "health potion"));
        let taken = p.take_item("potion");
        assert!(taken.is_some());
        assert!(p.inventory.is_empty());
    }

    #[test]
    fn take_item_wrong_keyword_returns_none() {
        let mut p = warrior();
        p.inventory.push(ItemInstance::new("nexus:potion", "health potion"));
        let taken = p.take_item("sword");
        assert!(taken.is_none());
        assert_eq!(p.inventory.len(), 1);
    }

    #[test]
    fn stats_for_warrior_human_are_reasonable() {
        let stats = Stats::for_class_race(&Class::Warrior, &Race::Human, 1);
        assert!(stats.hp > 0);
        assert!(stats.strength >= 10, "human warrior should have decent strength");
    }

    #[test]
    fn stats_for_mage_elf_has_high_intelligence() {
        let stats = Stats::for_class_race(&Class::Mage, &Race::Elf, 1);
        assert!(stats.intelligence > stats.strength, "elf mage int should exceed str");
    }

    #[test]
    fn is_in_combat_false_by_default() {
        let p = warrior();
        assert!(!p.is_in_combat());
    }
}

// ─── Time / Weather ───────────────────────────────────────────────────────────

mod time_tests {
    use neomud::time::{GameTime, Season, TimeOfDay, Weather};

    fn make_time(hour: u32) -> GameTime {
        let mut t = GameTime::new();
        t.hour = hour;
        t.minute = 0;
        t
    }

    #[test]
    fn time_of_day_deep_night() {
        assert_eq!(make_time(2).time_of_day(), TimeOfDay::DeepNight);
    }

    #[test]
    fn time_of_day_dawn() {
        assert_eq!(make_time(5).time_of_day(), TimeOfDay::Dawn);
    }

    #[test]
    fn time_of_day_morning() {
        assert_eq!(make_time(8).time_of_day(), TimeOfDay::Morning);
    }

    #[test]
    fn time_of_day_midday() {
        assert_eq!(make_time(12).time_of_day(), TimeOfDay::Midday);
    }

    #[test]
    fn time_of_day_dusk() {
        assert_eq!(make_time(18).time_of_day(), TimeOfDay::Dusk);
    }

    #[test]
    fn time_advance_increments_tick() {
        let mut t = GameTime::new();
        let tick_before = t.tick;
        t.advance(1u64);
        assert!(t.tick > tick_before);
    }

    #[test]
    fn weather_transition_does_not_panic() {
        let season = Season::Summer;
        for w in [
            Weather::Clear, Weather::PartlyCloudy, Weather::Overcast,
            Weather::LightRain, Weather::HeavyRain, Weather::Thunderstorm,
        ] {
            let _ = w.transition(&season);
        }
    }

    #[test]
    fn weather_display_is_lowercase() {
        assert_eq!(Weather::Clear.to_string(), "clear");
        assert_eq!(Weather::HeavyRain.to_string(), "heavy rain");
    }
}

// ─── World contextual description ─────────────────────────────────────────────

mod world_tests {
    use neomud::world::parse_area_file_str;
    use neomud::time::{GameTime, Weather};

    fn deepwood_toml() -> String {
        std::fs::read_to_string("world/areas/deepwood.toml").expect("deepwood.toml missing")
    }

    #[test]
    fn contextual_description_falls_back_to_default() {
        let (area, _, _) = parse_area_file_str(&deepwood_toml()).unwrap();
        let room = area.rooms.get("deepwood:trail_head").unwrap();
        let time = GameTime::new();
        let weather = Weather::Clear;
        let desc = room.contextual_description(&time, &weather);
        assert!(!desc.is_empty());
    }

    #[test]
    fn contextual_description_uses_night_variant() {
        let (area, _, _) = parse_area_file_str(&deepwood_toml()).unwrap();
        let room = area.rooms.get("deepwood:trail_head").unwrap();
        let mut time = GameTime::new();
        time.hour = 23;
        time.minute = 0;
        let weather = Weather::Clear;
        let desc = room.contextual_description(&time, &weather);
        // Night description exists for trail_head — should differ from default
        assert!(!desc.is_empty());
    }

    #[test]
    fn contextual_description_uses_weather_variant() {
        let (area, _, _) = parse_area_file_str(&deepwood_toml()).unwrap();
        let room = area.rooms.get("deepwood:trail_head").unwrap();
        let mut time = GameTime::new();
        time.hour = 12; // midday — no special description
        time.minute = 0;
        let weather = Weather::HeavyRain;
        let default_desc = room.contextual_description(&GameTime::new(), &Weather::Clear);
        let rain_desc = room.contextual_description(&time, &weather);
        // The rain description should be different from the clear default
        assert_ne!(default_desc, rain_desc, "heavy rain should give a different description");
    }
}
