// Spirit Grove — inhabited by ancient nature spirits
// Players can interact with the spirits to gain blessings or learn lore

fn on_enter(ctx) {
    let actions = [];

    // The grove acknowledges the visitor
    if rand_bool(60) {
        let greetings = [
            "The trees lean imperceptibly toward you. You feel observed.",
            "A deep silence falls as you enter. The birds have stopped singing.",
            "Something stirs high in the canopy — too large for any bird.",
            "The light shifts, brightening for a moment, then returns to normal.",
        ];
        actions.push(#{
            action: "tell_player",
            player: ctx.player,
            msg: "\x1b[32m" + greetings[rand_range(0, 3)] + "\x1b[0m"
        });
    }

    actions
}

fn on_command(ctx) {
    let actions = [];

    if ctx.command == "speak" || ctx.command == "address" || (ctx.command == "say" && ("spirit" in ctx.args || "grove" in ctx.args || "tree" in ctx.args)) {
        let player_rep = 0;  // Would come from ctx.faction_rep in a full implementation

        let responses = [
            "A voice like wind through leaves whispers: 'We know you. You walk with light feet. That is... acceptable.'",
            "The trees creak in a pattern that might be words: 'Visitor. You smell of distant roads. You carry conflict. You carry also... curiosity. Both are noted.'",
            "Silence. Then, very slowly, the bark of the nearest tree shifts to form something like a face. It says nothing, but you have been seen.",
        ];

        actions.push(#{
            action: "tell_player",
            player: ctx.player,
            msg: "\x1b[32m" + responses[rand_range(0, 2)] + "\x1b[0m"
        });

        actions.push(#{ action: "adjust_rep", player: ctx.player, faction: "ancient_spirits", amount: 5 });
        return actions;
    }

    if ctx.command == "offer" || ctx.command == "leave" {
        if "flower" in ctx.args || "herb" in ctx.args || "food" in ctx.args {
            actions.push(#{
                action: "tell_player",
                player: ctx.player,
                msg: "\x1b[32mYou place your offering at the base of the great tree. The item sinks slowly into the moss and vanishes. In its place, a small green shoot appears. A gift returned for a gift given.\r\n\r\nThe spirits of the grove are pleased.\x1b[0m"
            });
            actions.push(#{ action: "adjust_rep", player: ctx.player, faction: "ancient_spirits", amount: 20 });
            actions.push(#{ action: "heal_player", player: ctx.player, amount: 15 });
            return actions;
        }
    }

    actions
}

fn on_tick(ctx) {
    let actions = [];

    if rand_bool(2) {
        let ambience = [
            "A sound like distant music drifts down from the highest branches.",
            "The shadows between the roots move briefly, independently of the light.",
            "A perfect circle of mushrooms springs up overnight where there were none before.",
            "One of the great trees lets fall a single leaf, though it is not autumn.",
            "Something vast and patient shifts in the deep roots below.",
        ];
        actions.push(#{
            action: "tell_room",
            room: ctx.room,
            msg: "\x1b[32m" + ambience[rand_range(0, 4)] + "\x1b[0m"
        });
    }

    actions
}
