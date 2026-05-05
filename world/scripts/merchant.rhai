// Merchant NPC script
// Called when players interact with merchant NPCs

fn on_enter(ctx) {
    let actions = [];

    // Merchant greets visitors
    if rand_bool(40) {  // 40% chance to greet
        let greetings = [
            "Welcome! Take a look at my wares.",
            "Fresh stock just in! Don't miss out.",
            "Best prices in the Nexus, I guarantee it.",
            "Ah, a customer! Please, have a look.",
        ];
        let greeting = greetings[rand_range(0, 3)];
        actions.push(#{
            action: "tell_room",
            room: ctx.room,
            msg: "\x1b[33m" + ctx.npc + " says, \"" + greeting + "\"\x1b[0m"
        });
    }

    actions
}

fn on_say(ctx) {
    let actions = [];
    let msg = ctx.message.to_lower();

    // React to keywords in player speech
    if "discount" in msg || "deal" in msg || "cheap" in msg {
        actions.push(#{
            action: "tell_player",
            player: ctx.player,
            msg: "\x1b[33m" + ctx.npc + " snorts. \"My prices are already the best around. You won't find better.\"\x1b[0m"
        });
    } else if "help" in msg || "advice" in msg {
        actions.push(#{
            action: "tell_player",
            player: ctx.player,
            msg: "\x1b[33m" + ctx.npc + " says, \"Use 'list' to see what I'm selling, 'buy <item>' to purchase.\"\x1b[0m"
        });
    } else if "stock" in msg || "have" in msg {
        actions.push(#{
            action: "tell_player",
            player: ctx.player,
            msg: "\x1b[33m" + ctx.npc + " gestures at the shelves. \"Type 'list' to see what's available.\"\x1b[0m"
        });
    }

    actions
}

fn on_tick(ctx) {
    let actions = [];

    // Occasional ambient speech
    if rand_bool(2) {  // 2% chance per tick
        let phrases = [
            "is tallying up the day's receipts.",
            "rearranges some items on display.",
            "hums a tuneless merchant's song.",
            "eyes passing travelers with practiced calculation.",
        ];
        actions.push(#{
            action: "tell_room",
            room: ctx.room,
            msg: "\x1b[2m" + ctx.npc + " " + phrases[rand_range(0, 3)] + "\x1b[0m"
        });
    }

    actions
}
