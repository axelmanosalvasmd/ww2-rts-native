use super::*;
use crate::{
    combat,
    economy::Faction,
    economy_state::ArmySize,
    match_combat::{DamageWorld, ExplosionSource},
    match_commands::Command,
    match_end::EndReason,
    match_runtime::PlayerConfig,
    weather::WeatherChoice,
};
use serde_json::json;

fn scene() -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Assault intent", "w":120, "h":80,
            "rows":vec![".".repeat(120);80],
            "spawns":[{"x":18,"y":40},{"x":100,"y":40}],
            "defend":[0], "assaultTime":300,
            "points":[{"x":60,"y":15,"vp":1,"mp":0},{"x":60,"y":65,"vp":0,"mp":2}]
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new_assault(
        &map,
        MatchOptions {
            seed: 71,
            mode: Mode::Assault,
            players: vec![
                PlayerConfig {
                    name: "Defender".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "Attacker".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: true,
            logistics: false,
            sandbox: false,
        },
        0,
    )
    .unwrap()
}

fn bunker(m: &Match) -> u32 {
    m.world
        .game
        .units
        .values()
        .find(|u| m.rules.kinds[u.kind].name == "bunker")
        .unwrap()
        .id
}

fn hurt(m: &mut Match, id: u32, source: ExplosionSource) -> f64 {
    let before = m.world.game.units[&id].hp;
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    DamageWorld {
        world: &mut m.world,
        combat: &mut m.combat,
        grid: &mut m.grid,
        rules: &m.rules,
        logistics: &m.logistics,
        production: &m.production,
        mode: m.options.mode,
        tech_enabled: m.options.tech,
        environment: &mut m.environment,
        environment_seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &order,
        wind_angle: m.start.wind.0,
        wind_strength: m.start.wind.1,
    }
    .hurt(id, source, 1.0, 1, &mut m.firing)
    .unwrap();
    before - m.world.game.units[&id].hp
}

#[test]
fn mode29_bunker_is_visible_immobile_free_and_never_veteran() {
    let mut m = scene();
    let id = bunker(&m);
    let u = &m.world.game.units[&id];
    let at = u.at();
    assert_eq!(u.hp, 3000.0);
    let definition = Catalog::compiled().unwrap().unit("bunker").unwrap();
    assert_eq!(
        definition.get("w").unwrap().get("range").unwrap().number(),
        Some(36.0)
    );
    assert_eq!(combat::veterancy("bunker", 1e9).unwrap(), 0);
    assert_eq!(definition.get("cost").unwrap().number(), Some(0.0));
    assert_eq!(m.rules.kinds[u.kind].speed, 0.0);
    let command = Command::from_json(&json!({"t":"move","orders":[[id,100,100]]})).unwrap();
    m.execute_command(0, &command, false).unwrap();
    assert!(m.world.game.units[&id].path.is_empty());
    for _ in 0..4 {
        m.advance_tick()
            .expect("Assault uses the ordinary connected runner");
    }
    assert_eq!(m.world.game.units[&id].at(), at);
    assert!(
        m.world.game.players[1].visible.contains(&id),
        "distant bunker is visible through fog"
    );
    let empty: Vec<&str> = vec![];
    let context = crate::economy_state::Context {
        mode: crate::economy_state::ModeContext {
            kind: Mode::Assault,
            ..Default::default()
        },
        army: ArmySize::Standard,
        units: &[crate::economy_state::Unit {
            id: id.into(),
            owner: Some(0),
            kind: "bunker",
            hp: 3000.,
            built: 1.,
            queue: &empty,
        }],
        drops: &[],
        points: &[],
        nodes: &[],
        world: None,
    };
    assert_eq!(m.start.economy.population(&context, 0).unwrap(), 0.0);
}

#[test]
fn mode30_bunker_support_and_annihilation_howitzer_damage_use_their_reductions() {
    let mut assault = scene();
    let id = bunker(&assault);
    let source = ExplosionSource::support("artillery").unwrap();
    let base = source.terrain.unwrap_or(source.vehicle);
    assert!((hurt(&mut assault, id, source) - base * 0.05).abs() < 1e-9);
    let howitzer = assault.rules.kind("howitzer").unwrap();
    let source = ExplosionSource::weapon(howitzer, &assault.rules).unwrap();
    let base = source.terrain.unwrap_or(source.vehicle);
    assert!((hurt(&mut assault, id, source) - base).abs() < 1e-9);
    // The same canonical target is used to isolate the mode-dependent damage rule.
    assault.options.mode = Mode::Annihilation;
    assault.world.game.mode = Some(ModeKind::Annihilation);
    assert!((hurt(&mut assault, id, source) - base * 0.1).abs() < 1e-9);
}

#[test]
fn mode31_assault_runner_cleans_bunker_death_and_freezes_the_decided_clock() {
    let mut m = scene();
    let id = bunker(&m);
    let at = m.world.game.units[&id].at();
    assert_eq!(m.assault.unwrap().total, 1);
    assert_eq!(m.start.points.len(), 1, "pure VP objectives are omitted");
    m.advance_tick().unwrap();
    assert!(m.assault.unwrap().time_left < 300.0);
    assert_eq!(
        m.start
            .economy
            .players
            .iter()
            .map(|p| p.vp)
            .collect::<Vec<_>>(),
        vec![0., 0.]
    );
    let source = ExplosionSource {
        infantry: 10000.,
        vehicle: 10000.,
        terrain: Some(10000.),
        suppression: 0.,
        scar: 0.,
        anti_garrison: None,
        weapon_kind: None,
        off_map_support: false,
    };
    hurt(&mut m, id, source);
    let tick = m.advance_tick().unwrap();
    assert!(tick.retired.iter().any(|r| r.unit.id == id));
    assert!(!m.world.game.units.contains_key(&id));
    assert!(
        m.world
            .game
            .units
            .values()
            .any(|u| m.rules.kinds[u.kind].name == "flakpos")
    );
    let finish = m.finish.unwrap();
    assert_eq!(
        (finish.winner, finish.reason, finish.end_at),
        (1, EndReason::Structures, at)
    );
    let clock = m.assault.unwrap().time_left;
    m.advance_tick().unwrap();
    assert_eq!(m.assault.unwrap().time_left, clock);
    assert_eq!(m.finish, Some(finish));
    let mut timeout = scene();
    timeout.assault.as_mut().unwrap().time_left = timeout.rules.movement.tick;
    timeout.advance_tick().unwrap();
    let finish = timeout.finish.unwrap();
    assert_eq!((finish.winner, finish.reason), (0, EndReason::Timer));
    assert_eq!(finish.end_at, Point { x: 120., z: 80. });
}
