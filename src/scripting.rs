/// Rhai scripting engine — every room, NPC, and item can have a script.
///
/// Scripts are hot-reloadable .rhai files. They expose lifecycle hooks:
///   fn on_enter(ctx)      → Array of actions (or [])
///   fn on_exit(ctx)       → Array of actions
///   fn on_command(ctx)    → Array of actions  (intercept unknown cmds)
///   fn on_say(ctx)        → Array of actions
///   fn on_tick(ctx)       → Array of actions  (called each game tick)
///   fn on_attack(ctx)     → Array of actions
///   fn on_die(ctx)        → Array of actions
///   fn on_use(ctx)        → Array of actions  (items)
///   fn on_pickup(ctx)     → Array of actions  (items)
///   fn describe(ctx)      → String (override description)
///
/// Actions returned by scripts:
///   #{ action: "tell_player", player: "name", msg: "..." }
///   #{ action: "tell_room", room: "id", msg: "..." }
///   #{ action: "tell_area", area: "id", msg: "..." }
///   #{ action: "move_player", player: "name", to: "room_id" }
///   #{ action: "move_npc", npc: "id", to: "room_id" }
///   #{ action: "spawn_npc", template: "id", room: "room_id" }
///   #{ action: "spawn_item", template: "id", room: "room_id" }
///   #{ action: "give_item", player: "name", template: "template_id" }
///   #{ action: "heal_player", player: "name", amount: 10 }
///   #{ action: "damage_player", player: "name", amount: 10 }
///   #{ action: "set_flag", target: "player/npc/room", id: "...", flag: "...", value: true }
///   #{ action: "record_history", room: "id", event: "..." }
///   #{ action: "grant_skill", player: "name", skill: "..." }
///   #{ action: "adjust_rep", player: "name", faction: "...", amount: 5 }

use rhai::{Engine, Scope, AST, Dynamic, Map as RhaiMap};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, warn, error};

#[derive(Debug, Clone)]
pub struct ScriptAction {
    pub action: String,
    pub params: HashMap<String, serde_json::Value>,
}

pub struct ScriptEngine {
    engine: Engine,
    cache: Arc<Mutex<HashMap<PathBuf, AST>>>,
    base_path: PathBuf,
}

impl ScriptEngine {
    pub fn new(world_path: &str) -> Self {
        let mut engine = Engine::new();

        // Safety limits
        engine.set_max_operations(50_000);
        engine.set_max_string_size(4096);
        engine.set_max_array_size(1024);
        engine.set_max_map_size(256);

        // Register utility functions available to all scripts
        engine.register_fn("rand_range", |min: i64, max: i64| -> i64 {
            use rand::Rng;
            rand::thread_rng().gen_range(min..=max)
        });
        engine.register_fn("rand_bool", |percent: i64| -> bool {
            use rand::Rng;
            rand::thread_rng().gen_range(0i64..100) < percent
        });
        engine.register_fn("action", |a: &str| -> Dynamic {
            let mut m = RhaiMap::new();
            m.insert("action".into(), Dynamic::from(a.to_string()));
            Dynamic::from(m)
        });

        ScriptEngine {
            engine,
            cache: Arc::new(Mutex::new(HashMap::new())),
            base_path: PathBuf::from(world_path).join("scripts"),
        }
    }

    /// Reload all cached scripts from disk.
    pub fn reload_all(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
        debug!("Script cache cleared — will reload on next call");
    }

    fn load_ast(&self, script_name: &str) -> Option<AST> {
        let path = self.base_path.join(script_name);
        {
            let cache = self.cache.lock().unwrap();
            if let Some(ast) = cache.get(&path) {
                return Some(ast.clone());
            }
        }

        if !path.exists() {
            return None;
        }

        match self.engine.compile_file(path.clone()) {
            Ok(ast) => {
                let mut cache = self.cache.lock().unwrap();
                cache.insert(path, ast.clone());
                Some(ast)
            }
            Err(e) => {
                error!("Script compile error in {:?}: {}", path, e);
                None
            }
        }
    }

    /// Execute a named hook in a script, returning any actions.
    pub fn call_hook(
        &self,
        script_name: &str,
        hook: &str,
        ctx: Dynamic,
    ) -> Vec<ScriptAction> {
        let ast = match self.load_ast(script_name) {
            Some(a) => a,
            None => return vec![],
        };

        let mut scope = Scope::new();
        let result: Result<Dynamic, _> = self.engine.call_fn(&mut scope, &ast, hook, (ctx,));

        match result {
            Ok(val) => parse_script_actions(val),
            Err(e) => {
                // EvalAltResult::ErrorFunctionNotFound is normal — hook just isn't defined
                if !e.to_string().contains("not found") {
                    warn!("Script '{}' hook '{}' error: {}", script_name, hook, e);
                }
                vec![]
            }
        }
    }

    /// Call the `describe` hook to get a custom room/NPC description.
    pub fn call_describe(&self, script_name: &str, ctx: Dynamic) -> Option<String> {
        let ast = self.load_ast(script_name)?;
        let mut scope = Scope::new();
        let result: Result<Dynamic, _> = self.engine.call_fn(&mut scope, &ast, "describe", (ctx,));
        match result {
            Ok(val) => val.try_cast::<String>().filter(|s| !s.is_empty()),
            Err(_) => None,
        }
    }
}

fn parse_script_actions(val: Dynamic) -> Vec<ScriptAction> {
    let mut actions = vec![];

    if let Some(arr) = val.try_cast::<rhai::Array>() {
        for item in arr {
            if let Some(map) = item.try_cast::<RhaiMap>() {
                let action = map.get("action")
                    .and_then(|v| v.clone().try_cast::<String>())
                    .unwrap_or_default();
                if action.is_empty() { continue; }

                let mut params = HashMap::new();
                for (k, v) in &map {
                    if k.as_str() == "action" { continue; }
                    let json_val = dynamic_to_json(v.clone());
                    params.insert(k.to_string(), json_val);
                }
                actions.push(ScriptAction { action, params });
            }
        }
    }
    actions
}

fn dynamic_to_json(val: Dynamic) -> serde_json::Value {
    if let Some(s) = val.clone().try_cast::<String>() {
        return serde_json::Value::String(s);
    }
    if let Some(n) = val.clone().try_cast::<i64>() {
        return serde_json::json!(n);
    }
    if let Some(b) = val.clone().try_cast::<bool>() {
        return serde_json::Value::Bool(b);
    }
    if let Some(f) = val.clone().try_cast::<f64>() {
        return serde_json::json!(f);
    }
    serde_json::Value::Null
}
