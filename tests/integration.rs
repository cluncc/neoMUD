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
    fn legacy_sha256_accepted() {
        // Pre-computed SHA-256 of "oldpassword"
        let sha256_hex = sha256_hex("oldpassword");
        assert_eq!(sha256_hex.len(), 64);
        assert!(verify_password(&sha256_hex, "oldpassword"));
    }

    #[test]
    fn legacy_sha256_wrong_password_rejected() {
        let sha256_hex = sha256_hex("oldpassword");
        assert!(!verify_password(&sha256_hex, "notoldpassword"));
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

    fn sha256_hex(s: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(s.as_bytes());
        hex::encode(h.finalize())
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
