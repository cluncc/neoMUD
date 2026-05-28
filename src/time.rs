use serde::{Deserialize, Serialize};
use rand::Rng;
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameTime {
    pub tick: u64,          // total ticks elapsed
    pub minute: u32,        // 0-59
    pub hour: u32,          // 0-23
    pub day: u32,           // 1-30
    pub month: u32,         // 1-12 (each has 30 days)
    pub year: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeOfDay {
    DeepNight,   // 0-4
    Dawn,        // 5-6
    Morning,     // 7-11
    Midday,      // 12-13
    Afternoon,   // 14-17
    Dusk,        // 18-19
    Evening,     // 20-21
    Night,       // 22-23
}

impl fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeOfDay::DeepNight => write!(f, "deep night"),
            TimeOfDay::Dawn => write!(f, "dawn"),
            TimeOfDay::Morning => write!(f, "morning"),
            TimeOfDay::Midday => write!(f, "midday"),
            TimeOfDay::Afternoon => write!(f, "afternoon"),
            TimeOfDay::Dusk => write!(f, "dusk"),
            TimeOfDay::Evening => write!(f, "evening"),
            TimeOfDay::Night => write!(f, "night"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl fmt::Display for Season {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Season::Spring => write!(f, "spring"),
            Season::Summer => write!(f, "summer"),
            Season::Autumn => write!(f, "autumn"),
            Season::Winter => write!(f, "winter"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Weather {
    Clear,
    PartlyCloudy,
    Overcast,
    LightRain,
    HeavyRain,
    Thunderstorm,
    Fog,
    LightSnow,
    Blizzard,
    HeatWave,
}

impl Weather {
    pub fn description(&self) -> &str {
        match self {
            Weather::Clear => "The sky is clear and bright.",
            Weather::PartlyCloudy => "Scattered clouds drift lazily overhead.",
            Weather::Overcast => "Heavy clouds cover the sky.",
            Weather::LightRain => "A gentle rain falls.",
            Weather::HeavyRain => "Rain pours down heavily.",
            Weather::Thunderstorm => "Lightning flashes and thunder rumbles.",
            Weather::Fog => "A thick fog obscures your vision.",
            Weather::LightSnow => "Light snowflakes drift down.",
            Weather::Blizzard => "A fierce blizzard rages.",
            Weather::HeatWave => "Oppressive heat shimmers on the air.",
        }
    }

    /// Attempt to transition weather based on season
    pub fn transition(&self, season: &Season) -> Weather {
        let mut rng = rand::thread_rng();
        let roll = rng.gen_range(0..100);

        match season {
            Season::Winter => match roll {
                0..=5 => Weather::Blizzard,
                6..=25 => Weather::LightSnow,
                26..=40 => Weather::Overcast,
                41..=55 => Weather::PartlyCloudy,
                _ => self.clone(),
            },
            Season::Spring => match roll {
                0..=10 => Weather::LightRain,
                11..=20 => Weather::HeavyRain,
                21..=30 => Weather::Overcast,
                31..=60 => Weather::PartlyCloudy,
                61..=85 => Weather::Clear,
                _ => self.clone(),
            },
            Season::Summer => match roll {
                0..=5 => Weather::Thunderstorm,
                6..=15 => Weather::HeatWave,
                16..=30 => Weather::PartlyCloudy,
                31..=80 => Weather::Clear,
                _ => self.clone(),
            },
            Season::Autumn => match roll {
                0..=10 => Weather::HeavyRain,
                11..=25 => Weather::LightRain,
                26..=45 => Weather::Overcast,
                46..=65 => Weather::Fog,
                66..=80 => Weather::PartlyCloudy,
                _ => self.clone(),
            },
        }
    }
}

impl fmt::Display for Weather {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Weather::Clear => write!(f, "clear"),
            Weather::PartlyCloudy => write!(f, "partly cloudy"),
            Weather::Overcast => write!(f, "overcast"),
            Weather::LightRain => write!(f, "light rain"),
            Weather::HeavyRain => write!(f, "heavy rain"),
            Weather::Thunderstorm => write!(f, "thunderstorm"),
            Weather::Fog => write!(f, "foggy"),
            Weather::LightSnow => write!(f, "light snow"),
            Weather::Blizzard => write!(f, "blizzard"),
            Weather::HeatWave => write!(f, "heat wave"),
        }
    }
}

impl GameTime {
    pub fn new() -> Self {
        GameTime {
            tick: 0,
            minute: 0,
            hour: 8,   // start at 8am
            day: 1,
            month: 4,  // start in spring
            year: 1000,
        }
    }

    /// Advance by one tick. Returns (hour_changed, day_changed).
    pub fn advance(&mut self, ticks_per_real_minute: u64) -> (bool, bool) {
        self.tick += 1;
        let mut hour_changed = false;
        let mut day_changed = false;

        // Each tick = 250ms real. ticks_per_real_minute governs game speed.
        // Default: multiplier=60 → 1 real min = 1 game hour
        // At 4 ticks/sec = 240 ticks/min, so 1 tick = 1 game min / 4
        // Actually: 1 tick = 250ms. multiplier=60 means 60 game-mins per real-min
        // So 1 tick advances game time by (60 / 240) = 0.25 game minutes
        // We accumulate fractional minutes → increment minute when we cross 1.0

        // Simpler: every (240/60) = 4 ticks = 1 game minute
        let ticks_per_game_minute = (240 / ticks_per_real_minute).max(1);
        if self.tick % ticks_per_game_minute == 0 {
            self.minute += 1;
            if self.minute >= 60 {
                self.minute = 0;
                self.hour += 1;
                hour_changed = true;
                if self.hour >= 24 {
                    self.hour = 0;
                    self.day += 1;
                    day_changed = true;
                    if self.day > 30 {
                        self.day = 1;
                        self.month += 1;
                        if self.month > 12 {
                            self.month = 1;
                            self.year += 1;
                        }
                    }
                }
            }
        }
        (hour_changed, day_changed)
    }

    pub fn time_of_day(&self) -> TimeOfDay {
        match self.hour {
            0..=4 => TimeOfDay::DeepNight,
            5..=6 => TimeOfDay::Dawn,
            7..=11 => TimeOfDay::Morning,
            12..=13 => TimeOfDay::Midday,
            14..=17 => TimeOfDay::Afternoon,
            18..=19 => TimeOfDay::Dusk,
            20..=21 => TimeOfDay::Evening,
            _ => TimeOfDay::Night,
        }
    }

    pub fn season(&self) -> Season {
        match self.month {
            3..=5 => Season::Spring,
            6..=8 => Season::Summer,
            9..=11 => Season::Autumn,
            _ => Season::Winter,
        }
    }

    pub fn display(&self) -> String {
        let period = if self.hour < 12 { "AM" } else { "PM" };
        let h = if self.hour == 0 { 12 } else if self.hour > 12 { self.hour - 12 } else { self.hour };
        format!(
            "{:02}:{:02} {} on Day {}, Month {} of Year {} ({})",
            h, self.minute, period, self.day, self.month, self.year, self.season()
        )
    }
}

impl Default for GameTime {
    fn default() -> Self {
        Self::new()
    }
}
