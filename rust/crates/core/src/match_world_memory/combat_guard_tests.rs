use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{LOCAL_DEFENDER_OWNER, Point, Unit},
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    random::MatchRandom,
    unit_spawn::initial_unit,
    vision::{los, vision_of},
    weather::WeatherChoice,
};
use serde_json::Value;

fn world_match() -> Match {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/world-contact.json")).unwrap();
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&fixture["map"]).unwrap()).unwrap();
    let mut m = Match::new_world(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::World,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("p{slot}"),
                    team: slot,
                    faction: Faction::Usa,
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(123),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
        MatchRandom::new(701),
    )
    .unwrap();
    // Isolate the observers while retaining the constructor's real World bindings.
    m.world.game.units.clear();
    m
}

fn unit(m: &Match, name: &str, id: u32, owner: usize, at: Point) -> Unit {
    initial_unit(
        &m.rules,
        Catalog::compiled().unwrap(),
        m.rules.kind(name).unwrap(),
        id,
        owner,
        at,
    )
    .unwrap()
}

fn local_visible(m: &Match) -> &[u32] {
    m.world_actor_view(LOCAL_DEFENDER_OWNER).unwrap().visible
}

#[test]
fn intent_cmb133_world_guards_refresh_visibility_by_los_building_and_range() {
    const GUARD: u32 = 1;
    const ENEMY: u32 = 2;
    let mut m = world_match();
    let guard_at = Point { x: 21.0, z: 21.0 };
    let enemy_at = Point { x: 31.0, z: 21.0 };
    let guard = unit(&m, "rifle", GUARD, LOCAL_DEFENDER_OWNER, guard_at);
    let enemy = unit(&m, "rifle", ENEMY, 0, enemy_at);
    m.world.game.units.insert(GUARD, guard);
    m.world.game.units.insert(ENEMY, enemy);
    m.grid.rebuild(&m.world.game.units);

    assert!(los(
        &m.world.game,
        &m.rules,
        (guard_at.x, guard_at.z),
        (enemy_at.x, enemy_at.z),
    ));
    m.refresh_world_visibility().unwrap();
    assert_eq!(local_visible(&m), &[ENEMY]);

    let blocker = m.world.game.cell_of(25.0, 21.0).unwrap();
    m.world.set_cell(blocker, b'H').unwrap();
    assert!(!los(
        &m.world.game,
        &m.rules,
        (guard_at.x, guard_at.z),
        (enemy_at.x, enemy_at.z),
    ));

    // sim.js:4450-4453: only armed or lookout buildings bypass guard LOS.
    for (name, built, sees_enemy) in [
        ("rifle", None, false),
        ("barracks", Some(1.0), false),
        ("pillbox", Some(0.5), false),
        ("tower", Some(0.5), false),
        ("pillbox", Some(1.0), true),
        ("tower", Some(1.0), true),
    ] {
        let mut guard = unit(&m, name, GUARD, LOCAL_DEFENDER_OWNER, guard_at);
        guard.built = built;
        assert!(vision_of(&m.world.game, &m.rules, &guard) > enemy_at.x - guard_at.x);
        m.world.game.units.insert(GUARD, guard);
        m.grid.rebuild(&m.world.game.units);
        m.refresh_world_visibility().unwrap();
        let expected: &[u32] = if sees_enemy { &[ENEMY] } else { &[] };
        assert_eq!(local_visible(&m), expected, "{name}, built={built:?}");
    }

    let range = vision_of(&m.world.game, &m.rules, &m.world.game.units[&GUARD]);
    m.world.game.units.get_mut(&ENEMY).unwrap().x = guard_at.x + range * 2.0;
    m.grid.rebuild(&m.world.game.units);
    m.refresh_world_visibility().unwrap();
    assert!(
        local_visible(&m).is_empty(),
        "lost enemies must not stay visible"
    );
}
