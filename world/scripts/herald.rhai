// The Nexus Herald script
// A mysterious, ancient figure with deep knowledge of the Nexus

fn on_enter(ctx) {
    let actions = [];

    // Herald notices new arrivals and sometimes greets them
    if rand_bool(30) {
        let greetings = [
            "The Herald's gaze settles on you briefly, as though cataloguing your arrival.",
            "The Herald inclines their head a fraction of an inch in acknowledgment.",
            "The Herald turns toward you, then back to whatever they were contemplating.",
        ];
        actions.push(#{
            action: "tell_player",
            player: ctx.player,
            msg: "\x1b[2m" + greetings[rand_range(0, 2)] + "\x1b[0m"
        });
    }

    // Record their presence in room history
    actions.push(#{
        action: "record_history",
        room: ctx.room,
        event: ctx.player + " passed through the Hub."
    });

    actions
}

fn on_say(ctx) {
    let actions = [];
    let msg = ctx.message.to_lower();

    if "herald" in msg || "who are you" in msg || "what are you" in msg {
        actions.push(#{
            action: "tell_player",
            player: ctx.player,
            msg: "\x1b[36mThe Herald says, quietly, 'I am what remains when purpose outlasts memory. I was appointed. By whom, I no longer recall. To what end — that, I remember.'\x1b[0m"
        });
    } else if "purpose" in msg || "what do you do" in msg {
        actions.push(#{
            action: "tell_player",
            player: ctx.player,
            msg: "\x1b[36mThe Herald says, 'I watch. I witness. When events of significance occur in the Nexus, I remember them. Should you wish to know what has happened here, I can tell you.'\x1b[0m"
        });
    } else if "history" in msg || "remember" in msg || "what happened" in msg {
        // Give the player some room history
        actions.push(#{
            action: "tell_player",
            player: ctx.player,
            msg: "\x1b[36mThe Herald says, 'Much has passed through here. Travelers. Merchants. The occasional catastrophe. What specifically would you like to know?'\x1b[0m"
        });
    } else if "dangerous" in msg || "threat" in msg || "safe" in msg {
        actions.push(#{
            action: "tell_player",
            player: ctx.player,
            msg: "\x1b[36mThe Herald says, 'The Nexus itself is protected. But what travelers bring with them... I cannot always prevent that.'\x1b[0m"
        });
    }

    actions
}

fn on_tick(ctx) {
    let actions = [];

    // The Herald occasionally makes cryptic observations
    if rand_bool(1) {
        let observations = [
            "The Herald gazes at the obelisk as though listening to it.",
            "The Herald turns their head slightly, as though hearing something distant.",
            "For a moment, the Herald's eyes glow faintly silver.",
            "The Herald traces a finger along one of the runes in the flagstones.",
        ];
        actions.push(#{
            action: "tell_room",
            room: ctx.room,
            msg: "\x1b[2m" + observations[rand_range(0, 3)] + "\x1b[0m"
        });
    }

    actions
}
