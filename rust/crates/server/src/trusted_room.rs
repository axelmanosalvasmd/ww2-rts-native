//! Owner-approved, nonpublic R1 hosting policy. Nothing here is client metadata.
use serde_json::{Value, json};
use ww2_core::{match_runtime::Match, random::RANDOM_VERSION, weather::WeatherChoice};

/// Opt-in hosting for one controlled room. Match seeds never cross the wire.
#[derive(Clone, Debug)]
pub struct TrustedRoomConfig {
    pub(crate) code: String,
    pub(crate) seed: u64,
}

impl TrustedRoomConfig {
    pub fn new(code: impl Into<String>, seed: u64) -> Result<Self, String> {
        let code = code.into();
        if !(3..=12).contains(&code.len()) || !code.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err("trusted room code needs 3-12 ASCII letters or digits".into());
        }
        Ok(Self { code, seed })
    }

    pub(crate) fn receipt(&self, state: &Match, epoch: u64) -> Value {
        let weather = match state.options.weather {
            WeatherChoice::Map => "map",
            WeatherChoice::Fixed(kind) => kind.name(),
            WeatherChoice::Random => "random",
        };
        json!({
            "room": self.code, "matchId": epoch,
            "reference": "e4e579a838aabae3b1d9dadbc17a0a87147bc52d",
            "randomVersion": RANDOM_VERSION,
            "options": {
                "seed": state.options.seed, "mode": match state.options.mode {
                    ww2_core::economy::Mode::Assault => "assault",
                    ww2_core::economy::Mode::Annihilation => "annihilation",
                    ww2_core::economy::Mode::Classic => "classic",
                    _ => "conquest",
                },
                "players": state.options.players.iter().map(|p| json!({
                    "name": p.name, "team": p.team, "faction": p.faction as u8
                })).collect::<Vec<_>>(),
                "shuffle": state.options.shuffle, "army": state.options.army.key(),
                "weather": weather,
                "weatherSeed": state.options.weather_seed,
                "livingWeather": state.options.living_weather,
                "mapKey": state.options.map_key, "tech": state.options.tech,
                "supply": state.options.supply, "logistics": state.options.logistics,
                "sandbox": state.options.sandbox
            },
            "derivedWeatherSeed": state.start.weather_seed,
            "environmentSeed": state.start.environment_seed
        })
    }
}
