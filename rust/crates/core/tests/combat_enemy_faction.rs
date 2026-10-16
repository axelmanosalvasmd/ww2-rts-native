//! CMB-02: both scripted enemies use the first unpicked core faction.
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};

#[test]
fn cmb02_horde_and_tutorial_choose_first_unpicked_faction_or_germany() {
    use Faction::{Germany, Uk, Usa, Ussr};

    // Prototype e4e579a, shared/sim.js:690-693: IDs [1, 2, 0], fallback 1.
    let cases: &[(&[Faction], Faction)] = &[
        (&[Usa], Germany),
        (&[Ussr], Germany),
        (&[Uk], Germany),
        (&[Germany], Ussr),
        (&[Germany, Usa], Ussr),
        (&[Germany, Ussr], Usa),
        (&[Ussr, Usa], Germany),
        (&[Germany, Ussr, Usa], Germany),
        (&[Uk, Germany, Ussr], Usa),
        (&[Germany, Germany, Uk, Usa, Usa], Ussr),
    ];
    let horde_map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&serde_json::json!({
            "name": "Enemy faction", "w": 60, "h": 60,
            "rows": vec![".".repeat(60); 60],
            "spawns": [{"x":30,"y":5}, {"x":10,"y":54}, {"x":50,"y":54}],
            "defend": [0], "points": [{"x":30,"y":30}]
        }))
        .unwrap(),
    )
    .unwrap();
    let tutorial_map = AuthoritativeMap::from_json(include_bytes!(
        "../../server/src/authoritative-maps/tutorial.json"
    ))
    .unwrap();

    for mode in [Mode::Horde, Mode::Tutorial] {
        for &(factions, expected) in cases {
            let options = MatchOptions {
                seed: 701,
                mode,
                players: factions
                    .iter()
                    .enumerate()
                    .map(|(slot, &faction)| PlayerConfig {
                        name: format!("Commander {slot}"),
                        team: slot as i32,
                        faction,
                    })
                    .collect(),
                shuffle: false,
                army: ArmySize::Standard,
                weather: WeatherChoice::Map,
                weather_seed: Some(1),
                living_weather: false,
                map_key: None,
                tech: false,
                supply: false,
                logistics: false,
                sandbox: false,
            };
            let m = match mode {
                Mode::Horde => Match::new_horde(&horde_map, options),
                Mode::Tutorial => Match::new_tutorial(&tutorial_map, options),
                _ => unreachable!(),
            }
            .unwrap_or_else(|e| panic!("{mode:?}, {factions:?}: {e}"));

            let enemy = factions.len();
            assert_eq!(m.start.players.len(), enemy + 1);
            assert_eq!(m.options.players.len(), enemy + 1);
            assert_eq!(m.world.game.players.len(), enemy + 1);
            for (slot, &faction) in factions.iter().enumerate() {
                assert_eq!(m.start.players[slot].config.faction, faction);
                assert_eq!(m.options.players[slot].faction, faction);
                assert_eq!(
                    m.start.players[slot].config.name,
                    format!("Commander {slot}")
                );
                assert_eq!(m.world.game.players[slot].team, 0);
            }
            let scripted = &m.start.players[enemy].config;
            assert_eq!(scripted.faction, expected, "{mode:?}, {factions:?}");
            assert_eq!(m.options.players[enemy].faction, expected);
            assert_eq!(
                scripted.name,
                if mode == Mode::Horde {
                    "Horde"
                } else {
                    "Enemy"
                }
            );
            assert_eq!(scripted.team, 1);
            assert_eq!(m.world.game.players[enemy].team, 1);
        }
    }
}
