// The Standing Stones — a magical clearing with responsive environment
// The stones react to player presence, time, and weather

fn on_enter(ctx) {
    let actions = [];

    // The stones react to arrival
    let reactions = [
        "The standing stones seem to lean slightly toward you as you enter the clearing.",
        "The air in the clearing hums faintly as you cross the perimeter of stones.",
        "For a moment, the carvings on the nearest stone glow with pale blue light.",
        "The moss of the clearing seems to deepen in color as you step onto it.",
    ];
    actions.push(#{
        action: "tell_player",
        player: ctx.player,
        msg: "\x1b[2m" + reactions[rand_range(0, 3)] + "\x1b[0m"
    });

    // Record who visited
    actions.push(#{
        action: "record_history",
        room: ctx.room,
        event: ctx.player + " entered the stone circle."
    });

    actions
}

fn describe(ctx) {
    if ctx.time == "night" || ctx.time == "deep night" {
        return "By night, the stone circle is transformed. The moss glows with foxfire in blue-green patterns that trace the same symbols as the carvings on the stones. The stones themselves pulse slowly, like sleeping things breathing. The clearing is unnervingly beautiful.";
    }

    if ctx.weather == "thunderstorm" {
        return "Lightning strikes nearby with alarming frequency, and the standing stones are drawing it — each strike hits a stone and is absorbed without damage. The carvings blaze with light after each strike. The air smells of ozone and something older.";
    }

    if ctx.time == "dawn" {
        return "At dawn, the standing stones cast long shadows that point inward, all toward the center of the circle. For a few minutes at sunrise, the shadows form a perfect geometric pattern. Then the sun rises further and the moment passes.";
    }

    ""
}

fn on_command(ctx) {
    let actions = [];

    if ctx.command == "touch" || ctx.command == "place hand" {
        let stone_words = ["stone", "rock", "carving", "rune", "inscription"];
        let is_stone = false;
        for word in stone_words {
            if word in ctx.args {
                is_stone = true;
                break;
            }
        }

        if is_stone {
            let responses = [
                "The stone is warm to the touch — warmer than the sun alone could explain. Your hand tingles.\r\n\r\nYou feel a momentary connection to something vast and patient.",
                "The carvings trace themselves against your palm like a message. You don't understand the language, but you understand the feeling: welcome.",
                "The stone vibrates faintly under your hand. You hear, just at the edge of perception, a deep harmonic tone.",
            ];
            let chosen = responses[rand_range(0, 2)];
            actions.push(#{
                action: "tell_player",
                player: ctx.player,
                msg: "\x1b[36m" + chosen + "\x1b[0m"
            });
            // Small chance of granting a magical skill
            if rand_bool(15) {
                actions.push(#{
                    action: "tell_player",
                    player: ctx.player,
                    msg: "\x1b[93mThe contact leaves a residue of old magic. You feel more attuned to the natural world.\r\n+Skill: nature\x1b[0m"
                });
                actions.push(#{ action: "grant_skill", player: ctx.player, skill: "nature" });
                actions.push(#{ action: "adjust_rep", player: ctx.player, faction: "ancient_spirits", amount: 25 });
            }
            return actions;
        }
    }

    if ctx.command == "meditate" || ctx.command == "pray" || ctx.command == "sit" {
        let hour = ctx.hour;
        if hour >= 0 && hour <= 5 || hour == 20 || hour == 21 {
            actions.push(#{
                action: "tell_player",
                player: ctx.player,
                msg: "\x1b[35mYou sit in the center of the circle and close your eyes. At this hour, the energies of the place are strong. You feel them move through you like wind through grass. When you open your eyes, you feel restored.\x1b[0m"
            });
            actions.push(#{ action: "heal_player", player: ctx.player, amount: 20 });
            actions.push(#{ action: "adjust_rep", player: ctx.player, faction: "ancient_spirits", amount: 10 });
        } else {
            actions.push(#{
                action: "tell_player",
                player: ctx.player,
                msg: "\x1b[2mYou sit in the center of the circle and attempt to meditate. The energies are quiescent now. You feel peaceful but nothing more.\x1b[0m"
            });
        }
        return actions;
    }

    actions
}

fn on_tick(ctx) {
    let actions = [];

    if rand_bool(3) {
        let ambience = [
            "The standing stones emit a subsonic hum that you feel more than hear.",
            "The moss ripples gently, as though breathing.",
            "A mote of foxfire drifts up from the ground and spirals skyward.",
            "The carvings on the nearest stone shift almost imperceptibly.",
            "The clearing is perfectly still; even the wind dies down here.",
        ];
        actions.push(#{
            action: "tell_room",
            room: ctx.room,
            msg: "\x1b[2m" + ambience[rand_range(0, 4)] + "\x1b[0m"
        });
    }

    actions
}
