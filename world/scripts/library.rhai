// The Nexus Archive — a room that responds to player actions
// The library has memory and reacts to repeated visits

fn on_enter(ctx) {
    let actions = [];

    // Welcome message on entry
    let welcomes = [
        "A floating mote of light drifts toward you as you enter, as if to offer guidance.",
        "The hush of the Archive settles around you like a cloak.",
        "Somewhere deeper in the stacks, pages rustle without wind.",
    ];
    actions.push(#{
        action: "tell_player",
        player: ctx.player,
        msg: "\x1b[2m" + welcomes[rand_range(0, 2)] + "\x1b[0m"
    });

    // Record visit
    actions.push(#{
        action: "record_history",
        room: ctx.room,
        event: ctx.player + " visited the Archive."
    });

    actions
}

fn describe(ctx) {
    // Dynamic description based on time
    if ctx.time == "night" || ctx.time == "deep night" {
        return "The Archive is eerily beautiful at night. The magical motes that illuminate the stacks burn brighter in darkness, and the shadows between the shelves seem to hold whispered conversations. The scholars who were here during the day have been replaced by nocturnal researchers who work in focused silence.";
    }

    if ctx.time == "dawn" {
        return "The Archive's lights are dimming as dawn approaches — the magical illumination apparently cycles with the sun. Scholars who worked through the night are gathering their notes, and the morning shift is beginning to arrive, fresh cups of tea steaming in their hands.";
    }

    // Default
    ""
}

fn on_command(ctx) {
    let actions = [];

    // Handle 'search' command in the library
    if ctx.command == "search" {
        let topics = ["magic", "history", "creatures", "maps", "nexus", "lore"];
        let topic = ctx.args.to_lower();

        if topic == "" {
            actions.push(#{
                action: "tell_player",
                player: ctx.player,
                msg: "\x1b[36mYou browse the shelves. To search for something specific, try: search <topic>\x1b[0m"
            });
        } else if "nexus" in topic || "obelisk" in topic {
            actions.push(#{
                action: "tell_player",
                player: ctx.player,
                msg: "\x1b[36mYou find a section dedicated to the Nexus's history. Several volumes discuss the obelisk, most concluding that it predates everything else by an unknowable margin. One marginalia-covered book suggests the obelisk is not stone at all, but a living thing.\r\n\r\nSkill improved: lore\x1b[0m"
            });
            actions.push(#{ action: "grant_skill", player: ctx.player, skill: "lore" });
        } else if "creatures" in topic || "bestiary" in topic || "monsters" in topic {
            actions.push(#{
                action: "tell_player",
                player: ctx.player,
                msg: "\x1b[36mYou find a cracked-spine bestiary. The illustrations are vivid and detailed. You spend some time reading the section on forest predators.\r\n\r\nSkill improved: survival\x1b[0m"
            });
            actions.push(#{ action: "grant_skill", player: ctx.player, skill: "survival" });
        } else {
            actions.push(#{
                action: "tell_player",
                player: ctx.player,
                msg: "\x1b[2mYou search the shelves for information about '" + ctx.args + "' but don't find anything conclusive. Perhaps try a different topic.\x1b[0m"
            });
        }
        return actions;
    }

    actions  // return empty — command not consumed
}

fn on_tick(ctx) {
    let actions = [];

    if rand_bool(1) {
        let ambience = [
            "A mote of light drifts past, trailing a faint scent of old paper.",
            "Somewhere in the stacks, a book thumps to the floor. No one moves to pick it up.",
            "The hum of magical preservation spells rises briefly, then fades.",
            "Pages flutter at the end of a nearby aisle, though there's no wind.",
        ];
        actions.push(#{
            action: "tell_room",
            room: ctx.room,
            msg: "\x1b[2m" + ambience[rand_range(0, 3)] + "\x1b[0m"
        });
    }

    actions
}
