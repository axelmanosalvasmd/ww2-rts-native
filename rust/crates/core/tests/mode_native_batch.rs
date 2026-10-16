//! Native mode behavior checks. No captured Source states are installed.
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{ModeKind, Point},
    map::AuthoritativeMap,
    match_commands::{Command, MoveOrder},
    match_end::EndReason,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    tutorial_steps::TutorialStep,
    weather::WeatherChoice,
};

fn map(name: &str) -> AuthoritativeMap {
    let bytes: &[u8] = match name {
        "tutorial" => include_bytes!("../../server/src/authoritative-maps/tutorial.json"),
        "default" => include_bytes!("../../server/src/authoritative-maps/default.json"),
        "hill-112" => include_bytes!("../../server/src/authoritative-maps/hill-112.json"),
        _ => panic!("unknown test map"),
    };
    AuthoritativeMap::from_json(bytes).unwrap()
}
fn options(mode: Mode, factions: &[Faction]) -> MatchOptions {
    MatchOptions {
        seed: 1821,
        mode,
        players: factions
            .iter()
            .enumerate()
            .map(|(i, &faction)| PlayerConfig {
                name: format!("Commander {i}"),
                team: i as i32,
                faction,
            })
            .collect(),
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: None,
        living_weather: false,
        map_key: None,
        tech: false,
        supply: false,
        logistics: false,
        sandbox: false,
    }
}
fn tutorial() -> Match {
    Match::new_tutorial(
        &map("tutorial"),
        options(Mode::Tutorial, &[Faction::Germany]),
    )
    .unwrap()
}
fn go(m: &mut Match, at: Point, attack_move: bool) {
    let orders = m
        .world
        .game
        .units
        .values()
        .filter(|u| u.owner == 0 && u.hp > 0.0 && !m.rules.kinds[u.kind].structure)
        .map(|u| MoveOrder {
            unit: i64::from(u.id),
            at,
        })
        .collect();
    assert_eq!(
        m.execute_command(
            0,
            &Command::Move {
                orders,
                control_ids: None,
                attack_move,
                queue: false,
                together: false,
                face: None,
            },
            false
        )
        .unwrap(),
        None
    );
}
fn reach(m: &mut Match, step: TutorialStep, budget: usize) {
    for _ in 0..budget {
        if m.tutorial_state().unwrap().step() == step {
            return;
        }
        m.advance_tick().unwrap();
    }
    assert_eq!(
        m.tutorial_state().unwrap().step(),
        step,
        "lesson did not advance at tick {}",
        m.world.game.tick
    );
}

#[test]
fn mode16_17_tutorial_roster_uses_shared_home_and_first_unused_enemy_faction() {
    for factions in [
        vec![Faction::Germany],
        vec![Faction::Ussr],
        vec![Faction::Usa],
        vec![Faction::Uk],
        vec![Faction::Germany, Faction::Ussr],
        vec![Faction::Germany, Faction::Ussr, Faction::Usa],
        vec![Faction::Uk; 5],
    ] {
        let enemy_faction = [Faction::Germany, Faction::Ussr, Faction::Usa]
            .into_iter()
            .find(|f| !factions.contains(f))
            .unwrap_or(Faction::Germany);
        let n = factions.len();
        let m = Match::new_tutorial(&map("tutorial"), options(Mode::Tutorial, &factions)).unwrap();
        assert_eq!(m.options.players.len(), n + 1);
        assert_eq!(m.tutorial_state().unwrap().enemy(), n);
        for (slot, p) in m.start.players.iter().enumerate() {
            let expected = if slot == n {
                Point { x: 137.0, z: 77.0 }
            } else {
                Point { x: 11.0, z: 43.0 }
            };
            assert_eq!(p.spawn, expected);
            assert_eq!(p.config.team, if slot == n { 1 } else { 0 });
            assert_eq!(m.world.game.players[slot].team, p.config.team);
            assert_eq!(m.start.economy.players[slot].team, p.config.team);
            if slot < n {
                assert_eq!(p.config.faction, factions[slot]);
            }
        }
        assert_eq!(m.options.players[n].name, "Enemy");
        assert_eq!(m.options.players[n].faction, enemy_faction);
        assert!(!m.world.game.units.values().any(|u| u.owner == n));
        assert_eq!(m.start.economy.players[n].mp, 0.0);
    }
}

#[test]
fn mode58_tutorial_starts_without_bases_and_recruits_at_home() {
    let mut configured = options(Mode::Tutorial, &[Faction::Germany]);
    configured.tech = true;
    configured.supply = true;
    configured.logistics = true;
    let mut m = Match::new_tutorial(&map("tutorial"), configured).unwrap();
    assert_eq!(m.world.game.mode, Some(ModeKind::Tutorial));
    assert!(!m.options.tech && !m.options.supply && !m.options.logistics);
    assert!(m.building_cells.is_empty());
    let mut kinds = m
        .world
        .game
        .units
        .values()
        .map(|u| {
            assert_eq!(u.owner, 0);
            m.rules.kinds[u.kind].name.as_str()
        })
        .collect::<Vec<_>>();
    kinds.sort();
    assert_eq!(kinds, ["mg", "rifle", "rifle"]);
    let wallet = m.start.economy.players[0].mp;
    let bought = m.recorded_buy(0, "rifle", None).unwrap();
    assert_eq!(m.world.game.units.len(), 4);
    assert_eq!(m.world.game.units[&bought.unit].owner, 0);
    assert_eq!(
        m.rules.kinds[m.world.game.units[&bought.unit].kind].name,
        "rifle"
    );
    assert_eq!(
        m.start.economy.players[0].mp,
        wallet - bought.purchase.request.spend.mp
    );
    assert!(m.building_cells.is_empty());
}

#[test]
fn mode59_tutorial_tick_pays_attacker_base_income_without_vp() {
    let mut m = tutorial();
    let before = m.start.economy.players[0].mp;
    m.advance_tick().unwrap();
    assert_eq!(m.world.game.tick, 1);
    assert_eq!(m.start.economy.players[0].inc, 5.0);
    assert_eq!(m.start.economy.players[0].mp, before + 0.25);
    assert_eq!(m.start.economy.players[0].vp, 0.0);
    assert!(m.finish.is_none());
}

#[test]
fn mode62_64_65_live_orchard_recruit_tank_progression() {
    let mut m = tutorial();
    go(&mut m, Point { x: 31.0, z: 45.0 }, false);
    reach(&mut m, TutorialStep::Orchard, 2000);
    let sentries = m.tutorial_state().unwrap().spawned().to_vec();
    assert_eq!(sentries.len(), 2);
    for (id, at) in sentries
        .iter()
        .zip([Point { x: 49.0, z: 29.0 }, Point { x: 49.0, z: 59.0 }])
    {
        let u = &m.world.game.units[id];
        assert_eq!(u.at(), at);
        assert_eq!(u.hp, 60.0);
        assert_eq!(m.rules.kinds[u.kind].name, "rifle");
        assert!(!m.commands.units[id].auto_retreat);
    }
    m.advance_tick().unwrap();
    assert_eq!(m.tutorial_state().unwrap().step(), TutorialStep::Orchard);
    go(&mut m, Point { x: 49.0, z: 45.0 }, true);
    reach(&mut m, TutorialStep::Crossroads, 6000);
    assert!(
        sentries
            .iter()
            .all(|id| m.world.game.units.get(id).is_none_or(|u| u.hp <= 0.0))
    );
    let crossroad = m.start.points[0].at;
    go(&mut m, crossroad, false);
    reach(&mut m, TutorialStep::Recruit, 6000);
    assert!(m.start.economy.players[0].mp >= 250.0);
    m.advance_tick().unwrap();
    assert_eq!(m.tutorial_state().unwrap().step(), TutorialStep::Recruit);
    let wallet = m.start.economy.players[0].mp;
    let bought = m.recorded_buy(0, "at", None).unwrap();
    assert_eq!(
        m.start.economy.players[0].mp,
        wallet - bought.purchase.request.spend.mp
    );
    reach(&mut m, TutorialStep::Tank, 20);
    let tank = m.tutorial_state().unwrap().spawned()[0];
    let u = &m.world.game.units[&tank];
    assert_eq!(m.rules.kinds[u.kind].name, "tank");
    assert_eq!(u.at(), Point { x: 107.0, z: 43.0 });
    assert_eq!(u.amove, Some(Point { x: 83.0, z: 45.0 }));
    assert!(!m.commands.units[&tank].auto_retreat);
    go(&mut m, Point { x: 83.0, z: 45.0 }, true);
    reach(&mut m, TutorialStep::Trench, 6000);
    assert!(m.world.game.units.get(&tank).is_none_or(|u| u.hp <= 0.0));
    assert!(m.finish.is_none());
    // Trench entry is observed here; its clearance remains the retained failing journey.
    for &id in m.tutorial_state().unwrap().spawned() {
        assert!(ww2_core::vision::in_trench(
            &m.world.game,
            &m.rules,
            &m.world.game.units[&id]
        ));
    }
}

#[test]
fn mode31_assault_timer_phase_finishes_once_and_freezes_clock() {
    let mut m = Match::new_assault(
        &map("hill-112"),
        options(Mode::Assault, &[Faction::Germany, Faction::Ussr]),
        0,
    )
    .unwrap();
    let clock = m.assault.unwrap().time_left;
    assert!(clock > 1.0);
    assert!(m.apply_assault_victory(clock - 0.5).unwrap().is_none());
    assert_eq!(m.assault.unwrap().time_left, 0.5);
    let finish = m.apply_assault_victory(0.5).unwrap().unwrap();
    assert_eq!(finish.winner, 0);
    assert_eq!(finish.reason, EndReason::Timer);
    assert_eq!(
        finish.end_at,
        Point {
            x: m.world.game.w as f64,
            z: m.world.game.h as f64
        }
    );
    let timeline = m.timeline.clone();
    assert!(m.apply_assault_victory(30.0).unwrap().is_none());
    assert_eq!(m.assault.unwrap().time_left, 0.0);
    assert_eq!(m.finish, Some(finish));
    assert_eq!(m.timeline, timeline);
}

#[test]
fn mode32_assault_authoring_rejects_each_invalid_field() {
    let original = map("hill-112").source().clone();
    for (field, value) in [
        ("assaultTime", serde_json::json!(299)),
        ("assaultTime", serde_json::json!(3601)),
        ("assaultTime", serde_json::json!(900.5)),
        ("defend", serde_json::json!([])),
        ("defend", serde_json::json!([0, 0])),
        ("defend", serde_json::json!([0, 1, 2, 3])),
        ("defend", serde_json::json!([99])),
    ] {
        let mut source = original.clone();
        source[field] = value.clone();
        assert!(
            AuthoritativeMap::from_json(&serde_json::to_vec(&source).unwrap()).is_err(),
            "accepted {field}={value}"
        );
    }
    for time in [300, 3600] {
        let mut source = original.clone();
        source["assaultTime"] = serde_json::json!(time);
        assert!(AuthoritativeMap::from_json(&serde_json::to_vec(&source).unwrap()).is_ok());
    }
    let mut too_few = original.clone();
    for i in 0..3 {
        too_few["spawns"][i]["assault"] = serde_json::json!(true);
    }
    assert!(AuthoritativeMap::from_json(&serde_json::to_vec(&too_few).unwrap()).is_err());
    for value in [serde_json::json!(1), serde_json::json!("true")] {
        let mut source = original.clone();
        source["spawns"][0]["assault"] = value;
        assert!(AuthoritativeMap::from_json(&serde_json::to_vec(&source).unwrap()).is_err());
    }
}
