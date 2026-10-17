//! Room settings become canonical constructor inputs before room admission or publication.
use super::*;

impl Room {
    pub(super) fn native_match_options(
        &self,
        seed: u64,
        players: Vec<PlayerConfig>,
        map: &Value,
    ) -> Result<MatchOptions, String> {
        let mode = match self.mode.as_str() {
            "conquest" => Mode::Conquest,
            "assault" => Mode::Assault,
            "annihilation" => Mode::Annihilation,
            "classic" => Mode::Classic,
            "horde" => Mode::Horde,
            "tutorial" => Mode::Tutorial,
            "world" => Mode::World,
            _ => return Err("unsupported native mode setting".into()),
        };
        let army = match self.army.as_str() {
            "standard" => ArmySize::Standard,
            "large" => ArmySize::Large,
            "massive" => ArmySize::Massive,
            "endless" => ArmySize::Endless,
            _ => return Err("unsupported native army setting".into()),
        };
        Ok(MatchOptions {
            seed,
            mode,
            players,
            shuffle: true,
            army,
            weather: WeatherChoice::from_reference(Some(&self.weather)),
            weather_seed: None,
            living_weather: true,
            map_key: Some(self.map.clone()),
            tech: self.loaded_tech != Some(false),
            supply: true,
            logistics: self.loaded_logistics != Some(false)
                && logistics_enabled_for(&self.mode, map),
            sandbox: self.sandbox,
        })
    }

    /// Dispatch existing native constructors. Public start admission remains with the caller.
    pub(super) fn create_native_match(
        &self,
        map: &ww2_core::map::AuthoritativeMap,
        options: MatchOptions,
    ) -> Result<Match, String> {
        match options.mode {
            Mode::Default | Mode::Conquest => Match::new(map, options),
            Mode::Assault => Match::new_assault(map, options, self.defender as i32),
            Mode::Annihilation => Match::new_annihilation(map, options),
            Mode::Classic => Match::new_classic(map, options),
            Mode::Horde => Match::new_horde(map, options),
            Mode::Tutorial => Match::new_tutorial(map, options),
            Mode::World => Err("World room admission is deferred for R1".into()),
            Mode::Other => Err("unsupported native mode setting".into()),
        }
    }
}
