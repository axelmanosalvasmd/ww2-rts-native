//! Original authored maps for trusted native match construction.
//! HTTP delivery uses the independent public projection index in `maps`.

use std::{collections::BTreeMap, fmt, sync::OnceLock};
use ww2_core::{
    map::{AuthoritativeMap, MapError},
    match_runtime::{Match, MatchOptions},
};

#[path = "authoritative-map-data.rs"]
mod data;

#[derive(Clone, Debug)]
pub enum AuthoredMapError {
    UnknownMap,
    InvalidMap(MapError),
    Match(String),
}
impl fmt::Display for AuthoredMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownMap => write!(f, "unknown authored map"),
            Self::InvalidMap(error) => write!(f, "authored map: {error}"),
            Self::Match(error) => write!(f, "match startup: {error}"),
        }
    }
}
impl std::error::Error for AuthoredMapError {}

type Catalog = BTreeMap<&'static str, Result<AuthoritativeMap, MapError>>;
static CATALOG: OnceLock<Catalog> = OnceLock::new();

/// Load a validated, immutable original map by its exact catalogue key.
/// The server binary needs no reference checkout or filesystem map access.
pub fn load(name: &str) -> Result<&'static AuthoritativeMap, AuthoredMapError> {
    if !data::MAPS.iter().any(|(key, _)| *key == name) {
        return Err(AuthoredMapError::UnknownMap);
    }
    let maps = CATALOG.get_or_init(|| {
        data::MAPS
            .iter()
            .map(|(key, text)| (*key, AuthoritativeMap::from_json(text.as_bytes())))
            .collect()
    });
    match maps.get(name) {
        Some(Ok(map)) => Ok(map),
        Some(Err(error)) => Err(AuthoredMapError::InvalidMap(error.clone())),
        None => Err(AuthoredMapError::UnknownMap),
    }
}

/// Construct fresh canonical match state from private authoring data.
/// Runtime capability checks remain in the core; this does not start a room.
pub fn create_match(name: &str, options: MatchOptions) -> Result<Match, AuthoredMapError> {
    Match::new(load(name)?, options).map_err(AuthoredMapError::Match)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ww2_core::{
        economy::{Faction, Mode},
        economy_state::ArmySize,
        match_runtime::PlayerConfig,
        weather::WeatherChoice,
    };

    fn options() -> MatchOptions {
        MatchOptions {
            seed: 0x3c1055,
            mode: Mode::Conquest,
            players: vec![PlayerConfig {
                name: "Commander".into(),
                team: 0,
                faction: Faction::Usa,
            }],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: true,
            map_key: None,
            tech: true,
            supply: true,
            logistics: false,
            sandbox: false,
        }
    }

    #[test]
    fn every_original_map_constructs_a_native_match_without_losing_authoring_data() {
        let public: serde_json::Value =
            serde_json::from_str(include_str!("map-catalog.json")).unwrap();
        assert_eq!(data::MAPS.len(), public.as_object().unwrap().len());
        for (key, text) in data::MAPS {
            assert!(public.get(key).is_some());
            let map = load(key).unwrap();
            let original: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(map.source(), &original);
            let game = create_match(key, options()).unwrap();
            assert_eq!(game.world.game.tick, 0);
            assert_eq!(game.start.players.len(), 1);
            assert!(!game.world.game.units.is_empty());
        }
        let minefield = load("el-alamein").unwrap();
        assert_eq!(
            minefield.source()["rows"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|row| row.as_str().unwrap().bytes())
                .filter(|&ch| ch == b'N')
                .count(),
            246
        );
        assert!(!minefield.triggers().is_empty());
    }

    #[test]
    fn fresh_matches_cannot_mutate_the_cached_map_or_each_other() {
        let frozen = load("default").unwrap();
        let mut first = create_match("default", options()).unwrap();
        let second = create_match("default", options()).unwrap();
        let before = second.world.game.chars[0];
        let original = frozen.source().clone();
        first.world.set_cell(0, b'W').unwrap();
        first.world.game.units.values_mut().next().unwrap().hp = 0.0;
        assert_eq!(second.world.game.chars[0], before);
        assert_eq!(load("default").unwrap().source(), &original);
        let third = create_match("default", options()).unwrap();
        assert_eq!(third.world.game.chars[0], before);
        assert!(second.world.game.units.values().all(|unit| unit.hp > 0.0));
        assert!(std::ptr::eq(frozen, load("default").unwrap()));
        for key in [
            "default.json",
            "../default",
            "Default",
            "",
            "authoritative-maps/default",
        ] {
            assert!(matches!(load(key), Err(AuthoredMapError::UnknownMap)));
        }
    }
}
