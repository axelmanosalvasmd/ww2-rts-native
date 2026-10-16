use super::{
    tests::{acts, fixture, run, unit},
    *,
};

fn ghost(id: u32, kind: &str, x: f64) -> KnownBuilding {
    KnownBuilding {
        id,
        kind: kind.into(),
        owner: 1,
        at: Point { x, z: 20.0 },
    }
}
fn classic(mode: Mode) -> crate::commander_ai::Input {
    let mut i = fixture(
        (1..=6)
            .map(|id| unit(id, "rifle", 40.0 + id as f64))
            .collect(),
    );
    i.tick = 4000;
    i.points.clear();
    i.players[1].spawn = Point { x: 140.0, z: 30.0 };
    let c = i.runtime.as_mut().unwrap();
    c.mode = mode;
    c.known_buildings = vec![ghost(100, "barracks", 130.0)];
    i
}
fn look(
    input: crate::commander_ai::Input,
    sightings: &[Sighting],
    state: &mut State,
    level: Level,
) -> Vec<Value> {
    plan(
        &Observation::new(input).unwrap(),
        sightings,
        state,
        level,
        &mut MatchRandom::new(51),
    )
    .unwrap()
}
#[test]
fn intent_ai55_classic_and_world_rush_centroid_engineer_escape_and_spending_release() {
    for mode in [Mode::Classic, Mode::World] {
        for level in [Level::Normal, Level::Hard] {
            let mut i = classic(mode);
            let mut hq = unit(20, "hq", 60.0);
            hq.at.z = 20.0;
            i.units.push(hq);
            i.units.push(unit(21, "engineer", 70.0));
            let mut enemy = unit(30, "rifle", 90.0);
            enemy.owner = 1;
            i.units.push(enemy);
            let mut second_enemy = unit(31, "rifle", 86.0);
            second_enemy.owner = 1;
            i.units.push(second_enemy);
            i.units.push(unit(32, "barracks", 50.0));
            let c = i.runtime.as_mut().unwrap();
            c.units.insert(
                20,
                UnitFacts {
                    built: 1.0,
                    ..UnitFacts::default()
                },
            );
            c.units.insert(
                21,
                UnitFacts {
                    built: 1.0,
                    ..UnitFacts::default()
                },
            );
            c.units.insert(
                32,
                UnitFacts {
                    built: 1.0,
                    ..UnitFacts::default()
                },
            );
            c.build_reserve = 500.0;
            c.fort_reserve = 500.0;
            c.support.insert("artillery".into(), 0.0);
            c.munitions = 2000.0;
            i.manpower = 2000.0;
            let sight = Sighting {
                id: 30,
                kind: "rifle".into(),
                owner: 1,
                at: Point { x: 90.0, z: 20.0 },
                time: 199.0,
                worth: 100.0,
            };
            let second = Sighting {
                id: 31,
                at: Point { x: 86.0, z: 20.0 },
                ..sight.clone()
            };
            let reports = [sight.clone(), second];
            let commands = look(i.clone(), &reports, &mut State::default(), level);
            for id in 1..=6 {
                assert!(acts(&commands, "amove", id), "{mode:?} {commands:?}");
            }
            assert!(
                commands
                    .iter()
                    .any(|c| c["t"] == "move" && c["orders"] == json!([[21, 10.0, 20.0]]))
            );
            assert!(
                commands
                    .iter()
                    .any(|c| c["t"] == "support" && c["kind"] == "artillery")
            );
            assert!(commands.iter().any(|c| {
                c["t"] == "amove"
                    && c["orders"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|o| o[0] == 1 && o[1] == 88.0 && o[2] == 20.0)
            }));
            let mut banked = i.clone();
            banked.manpower = 500.0;
            let rushed = look(banked.clone(), &reports, &mut State::default(), level);
            let quiet = look(banked, &[], &mut State::default(), level);
            assert!(rushed.iter().any(|c| c["t"] == "buy"), "{rushed:?}");
            assert!(!quiet.iter().any(|c| c["t"] == "buy"), "{quiet:?}");
            // The exact five-second and four-minute limits stop rush defense.
            let observation = Observation::new(i.clone()).unwrap();
            let all: Vec<_> = observation
                .input()
                .units
                .iter()
                .filter(|u| u.owner == 0)
                .collect();
            let mine: Vec<_> = all.iter().copied().filter(|u| u.kind == "rifle").collect();
            let mut old = sight.clone();
            old.time = 195.0;
            assert!(
                !crate::commander_classic_rules::plan(
                    &observation,
                    observation.input().runtime.as_ref().unwrap(),
                    &mine,
                    &all,
                    &[&old],
                    &mut None,
                    Skill::for_level(level)
                )
                .unwrap()
                .rush
            );
            i.tick = 4800;
            let commands = look(
                i,
                &[Sighting {
                    time: 239.0,
                    ..sight
                }],
                &mut State::default(),
                level,
            );
            assert!(
                !commands
                    .iter()
                    .any(|c| c["t"] == "move" && c["orders"] == json!([[21, 10.0, 20.0]]))
            );
        }
    }
}
#[test]
fn intent_ai55_base_attack_value_weather_threshold_and_blind_recon() {
    for mode in [Mode::Classic, Mode::World] {
        let i = classic(mode);
        let sightings = [Sighting {
            id: 90,
            kind: "rifle".into(),
            owner: 1,
            at: Point { x: 150.0, z: 20.0 },
            time: 199.0,
            worth: 2000.0,
        }];
        let commands = look(i.clone(), &sightings, &mut State::default(), Level::Normal);
        assert!(!acts(&commands, "amove", 1));
        let mut stale = sightings[0].clone();
        stale.time = 170.0;
        assert!(acts(
            &look(i.clone(), &[stale], &mut State::default(), Level::Normal),
            "amove",
            1
        ));
        let mut bad_weather = i.clone();
        bad_weather.runtime.as_mut().unwrap().caution = 1.43;
        assert!(!acts(
            &look(bad_weather, &[], &mut State::default(), Level::Normal),
            "amove",
            1
        ));
        let mut blind = i;
        blind.runtime.as_mut().unwrap().known_buildings.clear();
        blind.manpower = 2000.0;
        blind.runtime.as_mut().unwrap().munitions = 2000.0;
        blind
            .runtime
            .as_mut()
            .unwrap()
            .support
            .insert("recon".into(), 0.0);
        let commands = look(blind, &[], &mut State::default(), Level::Normal);
        assert!(
            commands
                .iter()
                .any(|c| c["t"] == "support" && c["kind"] == "recon" && c["x"] == 140.0)
        );
    }
}
#[test]
fn intent_ai55_two_raiders_prefer_cars_remember_target_and_avoid_recent_guards() {
    let mut i = classic(Mode::Classic);
    i.units[0] = unit(1, "armoredcar", 42.0);
    i.runtime
        .as_mut()
        .unwrap()
        .known_buildings
        .push(ghost(101, "depot", 120.0));
    let observation = Observation::new(i.clone()).unwrap();
    let all: Vec<_> = observation.input().units.iter().collect();
    let context = observation.input().runtime.as_ref().unwrap();
    let mut raid = None;
    let p = crate::commander_classic_rules::plan(
        &observation,
        context,
        &all,
        &all,
        &[],
        &mut raid,
        Skill::for_level(Level::Normal),
    )
    .unwrap();
    let r = raid.as_ref().unwrap();
    assert_eq!(r.ids, [1, 6]);
    assert_eq!(p.attacks.len(), 2);
    // A live raid persists even after a guard is delivered; losing its depot releases it.
    let guard = Sighting {
        id: 90,
        kind: "rifle".into(),
        owner: 1,
        at: r.at,
        time: 199.0,
        worth: 100.0,
    };
    crate::commander_classic_rules::plan(
        &observation,
        context,
        &all,
        &all,
        &[&guard],
        &mut raid,
        Skill::for_level(Level::Normal),
    )
    .unwrap();
    assert!(raid.is_some());
    let mut no_raid = None;
    let p = crate::commander_classic_rules::plan(
        &observation,
        context,
        &all,
        &all,
        &[&guard],
        &mut no_raid,
        Skill::for_level(Level::Normal),
    )
    .unwrap();
    assert!(p.attacks.is_empty());
    assert!(no_raid.is_none());
    i.runtime
        .as_mut()
        .unwrap()
        .known_buildings
        .retain(|b| b.id != 101);
    let observation = Observation::new(i).unwrap();
    let all: Vec<_> = observation.input().units.iter().collect();
    crate::commander_classic_rules::plan(
        &observation,
        observation.input().runtime.as_ref().unwrap(),
        &all,
        &all,
        &[],
        &mut raid,
        Skill::for_level(Level::Normal),
    )
    .unwrap();
    assert!(raid.is_none());
}
#[test]
fn intent_ai55_counters_use_seen_tanks_and_six_infantry_at_adaptive_levels() {
    for level in [Level::Normal, Level::Hard] {
        let mut i = classic(Mode::Classic);
        let mut hq = unit(20, "hq", 20.0);
        hq.hp = 1000.0;
        i.units.push(unit(14, "engineer", 40.0));
        i.units.push(hq);
        i.units.push(unit(21, "barracks", 30.0));
        i.units.push(unit(22, "motorpool", 35.0));
        for id in 20..=22 {
            i.runtime.as_mut().unwrap().units.insert(
                id,
                UnitFacts {
                    built: 1.0,
                    ..UnitFacts::default()
                },
            );
        }
        let tank = Sighting {
            id: 90,
            kind: "tank".into(),
            owner: 1,
            at: Point { x: 145.0, z: 20.0 },
            time: 199.0,
            worth: 300.0,
        };
        let mut state = State::default();
        look(i.clone(), &[tank], &mut state, level);
        assert_eq!(state.decision.as_ref().unwrap().buy, "at");
        // Keep five rifles, one MG, a tank, mortar, medic, sniper and car so earlier priorities are satisfied.
        i.units.retain(|u| u.id != 6);
        for (id, kind) in [
            (6, "mg"),
            (7, "tank"),
            (8, "mortar"),
            (9, "medic"),
            (10, "sniper"),
            (11, "armoredcar"),
            (12, "howitzer"),
            (13, "tank"),
        ] {
            i.units.push(unit(id, kind, 40.0));
        }
        i.units.sort_by_key(|u| u.id);
        let sightings: Vec<_> = (90..96)
            .map(|id| Sighting {
                id,
                kind: "rifle".into(),
                owner: 1,
                at: Point { x: 145.0, z: 20.0 },
                time: 199.0,
                worth: 100.0,
            })
            .collect();
        let mut state = State::default();
        look(i, &sightings, &mut state, level);
        assert_eq!(state.decision.as_ref().unwrap().buy, "mg");
    }
    let mut i = classic(Mode::Classic);
    i.runtime
        .as_mut()
        .unwrap()
        .known_buildings
        .push(ghost(101, "depot", 120.0));
    let mut state = State::default();
    run(i, &mut state, Level::Easy);
    assert!(state.raid.is_none());
}

#[test]
fn ai_batch7_private_raid_and_assault_orders_survive_ordinary_match_and_every_seat_snapshot() {
    use crate::{
        economy::Faction,
        economy_state::ArmySize,
        map::AuthoritativeMap,
        match_commands::Command,
        match_runtime::{Match, MatchOptions, PlayerConfig},
        orders::OrderKey,
        recipient_snapshot::{DeliveryState, SelectedSeat},
        unit_publication::StagedUnit,
        weather::WeatherChoice,
    };
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"AI batch seven lifecycle","w":160,"h":80,"rows":vec![".".repeat(160);80],
            "spawns":[{"x":4,"y":4},{"x":155,"y":75}],"points":[{"x":20,"y":30}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 7552,
            mode: Mode::Conquest,
            players: (0..2)
                .map(|team| PlayerConfig {
                    name: format!("Seat {team}"),
                    team,
                    faction: Faction::Usa,
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
        },
    )
    .unwrap();
    for (i, kind) in ["rifle", "rifle", "mg"].iter().enumerate() {
        let id = m.next_unit_id;
        let draft = StagedUnit::ordinary(
            &m.world.game,
            &m.rules,
            0,
            m.rules.kind(kind).unwrap(),
            id,
            i as u32,
            m.start.players[0].spawn,
            Some(Point {
                x: 31.0 + i as f64 * 4.0,
                z: 41.0,
            }),
        )
        .unwrap();
        m.publish_starting_unit(draft).unwrap();
    }
    m.start.win_vp = 6.0;
    m.start.points[0].owner = Some(0);
    let mut state = State::default();
    let mut delivery = DeliveryState::new(&m).unwrap();
    let mut sequence = 0;
    let mut emitted = 0;
    let mut finish = None;
    let mut saw_members = false;
    let mut saw_raid = false;
    for _ in 0..500 {
        if m.world.game.tick % 2 == 0 && m.finish.is_none() {
            let observation = m.opening_commander_observation(0).unwrap();
            let mut input = observation.input().clone();
            // A detached Classic plan exercises private raid memory. It grants no Classic room admission.
            input.runtime.as_mut().unwrap().mode = Mode::Classic;
            input.runtime.as_mut().unwrap().known_buildings = vec![ghost(1000, "depot", 100.0)];
            let observation = Observation::new(input).unwrap();
            let context = observation.input().runtime.as_ref().unwrap();
            let all: Vec<_> = observation
                .input()
                .units
                .iter()
                .filter(|u| u.owner == 0)
                .collect();
            let mine: Vec<_> = all
                .iter()
                .copied()
                .filter(|u| {
                    !Catalog::compiled()
                        .unwrap()
                        .unit(&u.kind)
                        .unwrap()
                        .get("structure")
                        .and_then(Rule::boolean)
                        .unwrap_or(false)
                })
                .collect();
            let adaptive = crate::commander_classic_rules::plan(
                &observation,
                context,
                &mine,
                &all,
                &[],
                &mut state.raid,
                Skill::for_level(Level::Normal),
            )
            .unwrap();
            saw_raid |= state.raid.is_some();
            let now = m.world.game.tick as f64 / 20.0;
            let sit = Situation {
                kind: SituationKind::Push,
                until: 100.0,
                point: None,
                threat: None,
                at: Some(Point { x: 100.0, z: 41.0 }),
                counter: None,
                safe: None,
                failed_at: 0,
            };
            let empty = BTreeSet::new();
            let operation = crate::commander_assault::plan(
                &observation,
                context,
                &state.mind,
                Some(&sit),
                Skill::for_level(Level::Normal),
                now,
                &mut state.assault,
                crate::commander_assault::Options {
                    busy: &empty,
                    recovering: &empty,
                    sightings: &[],
                    support_at: None,
                    handoff: false,
                    region: None,
                },
            )
            .unwrap();
            saw_members |= !operation.claims.is_empty();
            // Raid and operation passes share the same command handle. Claimed members keep their staged orders.
            let controlled = state.assault.controlled(now);
            let raid_orders: Vec<_> = adaptive
                .attacks
                .iter()
                .filter(|(id, _)| !controlled.contains(id))
                .map(|(id, p)| json!([id, p.x, p.z]))
                .collect();
            let mut commands = operation.commands;
            if !raid_orders.is_empty() {
                commands.push(json!({"t":"amove","orders":raid_orders}));
            }
            for value in commands {
                let command = Command::from_json(&value).unwrap();
                m.enqueue_command(
                    OrderKey {
                        tick: m.world.game.tick as u64 + 1,
                        issuer: 0,
                        sequence,
                    },
                    command,
                )
                .unwrap();
                sequence += 1;
                emitted += 1;
            }
        }
        let report = m.advance_tick().unwrap();
        assert!(
            report.commands.iter().all(|c| c.denial.is_none()),
            "{:?}",
            report.commands
        );
        assert_eq!(m.tick_state.fault(), None);
        let selected = [0, 1].map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        });
        let frame = delivery
            .capture_selected_with_tick_events(&mut m, &selected, &report.events)
            .unwrap()
            .unwrap();
        for slot in 0..2 {
            assert_eq!(frame.recipient(slot).unwrap().into_json()["t"], "s");
        }
        if m.finish.is_some() {
            let end = *finish.get_or_insert(m.world.game.tick);
            if m.world.game.tick >= end + 8 {
                break;
            }
        }
    }
    assert!(emitted > 0 && saw_members && saw_raid);
    assert!(m.finish.is_some());
    assert!(m.world.game.tick >= finish.unwrap() + 8);
    assert_eq!(m.tick_state.fault(), None);
}

#[test]
fn intent_ai55_visible_empty_ground_keeps_recent_reports_and_exact_value_gate() {
    let mut input = classic(Mode::Classic);
    let own_value: f64 = input
        .units
        .iter()
        .map(|u| worth(u, Catalog::compiled().unwrap().unit(&u.kind).unwrap()))
        .sum();
    let make = |value| Sighting {
        id: 99,
        kind: "rifle".into(),
        owner: 1,
        at: Point { x: 46.0, z: 20.0 },
        time: 199.0,
        worth: value,
    };
    // This old position is currently visible and empty, but still counts for the thirty-second strategic window.
    let equal = make(own_value / 1.1);
    assert!(!acts(
        &look(
            input.clone(),
            std::slice::from_ref(&equal),
            &mut State::default(),
            Level::Normal
        ),
        "amove",
        1
    ));
    let lower = Sighting {
        worth: equal.worth - 1.0,
        ..equal
    };
    assert!(acts(
        &look(
            input.clone(),
            &[lower],
            &mut State::default(),
            Level::Normal
        ),
        "amove",
        1
    ));
    input.runtime.as_mut().unwrap().known_buildings = vec![ghost(100, "depot", 46.0)];
    let mut state = State::default();
    look(input, &[make(10.0)], &mut state, Level::Normal);
    assert!(
        state.raid.is_none(),
        "the recent guard report survives an empty current cell"
    );
}

#[test]
fn intent_ai55_world_recon_uses_only_discovered_enemy_homes() {
    let mut input = classic(Mode::World);
    let c = input.runtime.as_mut().unwrap();
    c.known_buildings.clear();
    c.munitions = 2000.0;
    c.support.insert("recon".into(), 0.0);
    c.hidden_spawns.insert(1);
    assert!(
        !look(input.clone(), &[], &mut State::default(), Level::Normal)
            .iter()
            .any(|c| c["t"] == "support" && c["kind"] == "recon")
    );
    input.players.push(crate::commander_ai::PlayerObservation {
        team: 2,
        out: false,
        spawn: Point { x: 150.0, z: 20.0 },
    });
    let commands = look(input, &[], &mut State::default(), Level::Normal);
    assert!(
        commands
            .iter()
            .any(|c| c["t"] == "support" && c["kind"] == "recon" && c["x"] == 150.0)
    );
}

#[test]
fn ai_batch7_tactics_caller_survives_ordinary_match_and_every_seat_snapshot() {
    use crate::{
        economy::Faction,
        economy_state::ArmySize,
        map::AuthoritativeMap,
        match_commands::Command,
        match_runtime::{Match, MatchOptions, PlayerConfig},
        orders::OrderKey,
        recipient_snapshot::{DeliveryState, SelectedSeat},
        unit_publication::StagedUnit,
        weather::WeatherChoice,
    };
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"AI batch seven tactics lifecycle","w":160,"h":80,
            "rows":vec![".".repeat(160);80],
            "spawns":[{"x":4,"y":4},{"x":155,"y":75}],
            "points":[{"x":20,"y":30},{"x":55,"y":30,"mp":0,"vp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 7553,
            mode: Mode::Conquest,
            players: (0..2)
                .map(|team| PlayerConfig {
                    name: format!("Seat {team}"),
                    team,
                    faction: Faction::Usa,
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
        },
    )
    .unwrap();
    for (i, kind) in ["rifle", "rifle", "mg"].iter().enumerate() {
        let id = m.next_unit_id;
        let draft = StagedUnit::ordinary(
            &m.world.game,
            &m.rules,
            0,
            m.rules.kind(kind).unwrap(),
            id,
            i as u32,
            m.start.players[0].spawn,
            Some(Point {
                x: 31.0 + i as f64 * 4.0,
                z: 41.0,
            }),
        )
        .unwrap();
        m.publish_starting_unit(draft).unwrap();
    }
    m.start.win_vp = 30.0;
    m.start.points[0].owner = Some(0);
    m.start.points[1].owner = Some(1);
    let mut state = State::default();
    let mut random = MatchRandom::new(7553);
    let mut delivery = DeliveryState::new(&m).unwrap();
    let mut sequence = 0;
    let mut planned = 0;
    let mut emitted = 0;
    let mut saw_assemble = false;
    let mut saw_phase_change = false;
    let mut finish = None;
    for _ in 0..800 {
        if m.world.game.tick % 2 == 0 && m.finish.is_none() {
            let observation = m.opening_commander_observation(0).unwrap();
            let commands = plan(&observation, &[], &mut state, Level::Normal, &mut random).unwrap();
            planned += 1;
            if let Some(operation) = &state.assault.operation {
                saw_assemble |= operation.phase == crate::commander_assault::Phase::Assemble
                    && !operation.members.is_empty();
                saw_phase_change |= saw_assemble
                    && operation.phase != crate::commander_assault::Phase::Assemble
                    && !operation.members.is_empty();
            }
            for value in commands {
                m.enqueue_command(
                    OrderKey {
                        tick: m.world.game.tick as u64 + 1,
                        issuer: 0,
                        sequence,
                    },
                    Command::from_json(&value).unwrap(),
                )
                .unwrap();
                sequence += 1;
                emitted += 1;
            }
        }
        let report = m.advance_tick().unwrap();
        assert!(
            report
                .commands
                .iter()
                .all(|command| command.denial.is_none()),
            "{:?}",
            report.commands
        );
        assert_eq!(m.tick_state.fault(), None);
        let selected = [0, 1].map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        });
        let frame = delivery
            .capture_selected_with_tick_events(&mut m, &selected, &report.events)
            .unwrap()
            .unwrap();
        for slot in 0..2 {
            assert_eq!(frame.recipient(slot).unwrap().into_json()["t"], "s");
        }
        if m.finish.is_some() {
            let end = *finish.get_or_insert(m.world.game.tick);
            if m.world.game.tick >= end + 8 {
                break;
            }
        }
    }
    assert!(planned > 0 && emitted > 0);
    assert!(saw_assemble && saw_phase_change);
    assert!(m.finish.is_some());
    assert!(m.world.game.tick >= finish.unwrap() + 8);
    assert_eq!(m.tick_state.fault(), None);
}
