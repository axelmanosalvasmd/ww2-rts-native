use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    field_works::{FortKind, placement, square_direction},
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, Denial},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::DeliveryState,
    weather::WeatherChoice,
};
use serde_json::json;

fn scene() -> (Match, u32) {
    let map = AuthoritativeMap::from_value(json!({
        "name":"Concrete runtime", "w":60,"h":60,"rows":vec![".".repeat(60);60],
        "heights":vec!["0".repeat(60);60],
        "spawns":[{"x":4,"y":4},{"x":55,"y":55}],"points":[{"x":30,"y":30}]
    }))
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Default,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("p{slot}"),
                    team: slot,
                    faction: Faction::try_from(slot as u8).unwrap(),
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
    let id = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = 30.;
    u.z = 20.;
    m.grid.update(id, u);
    m.start.economy.players[0].mp = 500.;
    (m, id)
}

fn tiers(m: &mut Match, tier: u8) {
    m.options.tech = true;
    m.production =
        crate::production::ProductionState::new(vec![
            crate::economy::TechState::new(tier, 0, 0, 0)
                .unwrap();
            2
        ])
        .unwrap();
    for u in m
        .world
        .game
        .units
        .values()
        .filter(|u| m.rules.kinds[u.kind].building)
    {
        m.production.initialize_building(u.id).unwrap();
    }
}
fn dig(id: u32, queue: bool) -> Command {
    Command::from_json(
        &json!({"t":"dig","ids":[id],"kind":"gate","x":30.,"z":25.,"dir":0.7,"queue":queue}),
    )
    .unwrap()
}
fn relocate(m: &mut Match, id: u32, at: Point) {
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = at.x;
    u.z = at.z;
    m.grid.update(id, u);
}
fn collect(m: &mut Match, delivery: &mut DeliveryState) {
    let frame = delivery
        .capture_with_lists(m, &[false; 2], &[false; 2], &[])
        .unwrap();
    for slot in 0..2 {
        let json = frame.recipient(slot).unwrap().into_json();
        assert!(json["units"].is_array());
    }
}

#[test]
fn base48_exact_tier_defaults_and_all_four_command_families_include_gates() {
    let reference = fixture();
    let (mut m, id) = scene();
    let table = crate::catalog::Catalog::compiled()
        .unwrap()
        .table("TIER")
        .unwrap()
        .object()
        .unwrap();
    assert_eq!(table.len(), reference["TIER"].as_object().unwrap().len());
    for (name, value) in reference["TIER"].as_object().unwrap() {
        assert_eq!(table[name].number(), value.as_f64(), "{name}");
    }
    for kind in &m.rules.kinds {
        let want = reference["TIER"]
            .get(&kind.name)
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u8;
        assert_eq!(
            crate::economy::tier_of(&kind.name).unwrap(),
            want,
            "{}",
            kind.name
        );
    }
    let forts = crate::catalog::Catalog::compiled()
        .unwrap()
        .table("FORTS")
        .unwrap();
    for kind in ["wall", "gate"] {
        assert_eq!(
            forts.get(kind).unwrap().get("tier").unwrap().number(),
            Some(2.)
        );
    }
    tiers(&mut m, 1);
    let before = m.start.economy.players[0].mp;
    let next = m.next_unit_id;
    let commands = [
        json!({"t":"buy", "unit":"mortar"}),
        json!({"t":"build", "kind":"pillbox", "ids":[id], "x":30,"z":25}),
        json!({"t":"dig", "kind":"wall", "ids":[id], "x":30,"z":25}),
        json!({"t":"dig", "kind":"gate", "ids":[id], "x":30,"z":25}),
        json!({"t":"entrench", "fort":"wall", "pattern":"line", "ids":[id], "x":30,"z":25,"x2":40,"z2":25}),
    ];
    for value in commands {
        let command = Command::from_json(&value).unwrap();
        assert_eq!(
            m.execute_command(0, &command, false).unwrap(),
            Some(Denial::Tier),
            "{value}"
        );
        assert_eq!(m.start.economy.players[0].mp, before);
        assert_eq!(m.next_unit_id, next);
        assert!(!m.world.game.units[&id].dig);
    }
    tiers(&mut m, 2);
    assert_eq!(m.execute_command(0, &dig(id, false), false).unwrap(), None);
    assert_eq!(m.start.economy.players[0].mp, before - 150.);
    assert_eq!(m.field_works.jobs()[&id].cells.len(), 3);
    assert_eq!(m.field_works.jobs()[&id].reach, Some(6.));
}

#[test]
fn eng12_paid_gate_jobs_square_three_cells_and_keep_existing_work_encodings() {
    let (mut m, id) = scene();
    let command = dig(id, false);
    assert_eq!(m.execute_command(0, &command, false).unwrap(), None);
    let job = m.field_works.jobs()[&id].clone();
    assert_eq!(job.direction, 0.7);
    assert_eq!(
        job.cells.iter().map(|c| c.cell).collect::<Vec<_>>(),
        [12 * 60 + 14, 12 * 60 + 15, 12 * 60 + 16]
    );
    assert_eq!(m.start.economy.players[0].mp, 350.);
    assert_eq!(m.story[0].mp_spent, 150.);
    let mut delivery = DeliveryState::new(&m).unwrap();
    let frame = delivery.capture(&mut m, &[true; 2], &[false; 2]).unwrap();
    assert_eq!(
        frame.recipient(0).unwrap().fields["works"],
        json!([[-1, 0, 30., 25., 0.7, "gate"]])
    );
    assert_eq!(frame.recipient(1).unwrap().fields["works"], json!([]));
    m.apply_field_work_unit_dt(id, 9.99).unwrap();
    assert_eq!(m.world.game.chars[job.cells[0].cell], b'.');
    m.apply_field_work_unit_dt(id, 0.01).unwrap();
    for _ in 0..2 {
        m.apply_field_work_unit_dt(id, 10.).unwrap();
    }
    assert!(!m.world.game.units[&id].dig);
    assert_eq!(m.story[0].built, 1.);
    for cell in job.cells {
        let gate = &m.world.game.units[&m.building_cells[&cell.cell].unit];
        assert_eq!(m.rules.kinds[gate.kind].name, "gate");
        assert_eq!(gate.built, Some(1.));
        assert_eq!(gate.hp, 400.);
        assert_eq!(gate.rot, 0.);
        assert_eq!(gate.cells.as_deref(), Some([cell.cell as u32].as_slice()));
        assert_eq!(m.commands.units[&gate.id].gate.unwrap().open, None);
    }
    collect(&mut m, &mut delivery);
}

#[test]
fn eng12_queued_gate_checks_tier_then_pays_only_when_it_starts() {
    let (mut m, id) = scene();
    tiers(&mut m, 1);
    assert_eq!(
        m.execute_command(0, &dig(id, true), false).unwrap(),
        Some(Denial::Tier)
    );
    tiers(&mut m, 2);
    assert_eq!(m.execute_command(0, &dig(id, true), false).unwrap(), None);
    assert_eq!(m.start.economy.players[0].mp, 500.);
    assert_eq!(m.commands.units[&id].orders.len(), 1);
    assert!(!m.world.game.units[&id].dig);
    let report = m.advance_tick().unwrap();
    assert!(report.commands.is_empty());
    assert!(m.commands.units[&id].orders.is_empty());
    assert!(m.world.game.units[&id].dig);
    assert!(m.start.economy.players[0].mp < 351.);
    assert_eq!(m.story[0].mp_spent, 150.);
    let mut delivery = DeliveryState::new(&m).unwrap();
    collect(&mut m, &mut delivery);
}

#[test]
fn eng12_ordinary_gate_lifecycle_finishes_and_projects_every_seat_without_fault() {
    for mode in [Mode::Default, Mode::Conquest] {
        let (mut m, id) = scene();
        m.options.mode = mode;
        // Authored ordinary match: one already-held objective wins after the gate finishes.
        m.start.points[0].owner = Some(0);
        m.start.points[0].progress = 1.;
        m.start.points[0].vp = 1.;
        m.start.win_vp = 35.;
        let enemy = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 1 && m.rules.kinds[u.kind].name == "rifle")
            .unwrap()
            .id;
        for unit in m.world.game.units.values_mut() {
            unit.hold_pos = true;
        }
        for control in m.commands.units.values_mut() {
            control.auto_cast = false;
            control.auto_retreat = false;
            control.hold_fire = true;
        }
        tiers(&mut m, 2);
        let first_gate = m.next_unit_id;
        m.enqueue_command(
            crate::orders::OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 1,
            },
            dig(id, false),
        )
        .unwrap();
        let mut delivery = DeliveryState::new(&m).unwrap();
        let selected = (0..2)
            .map(|slot| crate::recipient_snapshot::SelectedSeat {
                slot,
                full: false,
                omit: false,
            })
            .collect::<Vec<_>>();
        let mut saw_paid = false;
        let mut saw_open = false;
        let mut saw_closed = false;
        let mut saw_reopened = false;
        let mut dead = Vec::new();
        let mut finish_tick = None;
        for _ in 0..1000 {
            match m.world.game.tick {
                620 => relocate(&mut m, enemy, Point { x: 41., z: 25. }),
                645 => relocate(&mut m, enemy, Point { x: 90., z: 90. }),
                640 | 670 => {
                    let gate = if m.world.game.tick == 640 {
                        first_gate
                    } else {
                        first_gate + 1
                    };
                    assert_eq!(
                        m.commands.units[&gate].gate.unwrap().open,
                        Some(m.world.game.tick == 670)
                    );
                    m.enqueue_command(
                        crate::orders::OrderKey {
                            tick: m.world.game.tick as u64 + 1,
                            issuer: 0,
                            sequence: dead.len() as u64 + 2,
                        },
                        Command::Destroy {
                            ids: vec![i64::from(gate)],
                            id: None,
                        },
                    )
                    .unwrap();
                    dead.push(gate);
                }
                _ => {}
            }
            let report = m.advance_tick().unwrap();
            saw_paid |= m.field_works.is_active(id);
            for gate in m
                .world
                .game
                .units
                .values()
                .filter(|u| m.rules.kinds[u.kind].name == "gate")
            {
                let open = m.commands.units[&gate.id].gate.unwrap().open;
                saw_open |= open == Some(true);
                saw_closed |= open == Some(false);
                saw_reopened |= saw_closed && open == Some(true) && m.world.game.tick > 645;
                if let Some(open) = open {
                    let blocked = m.world.game.flags[gate.cells.as_ref().unwrap()[0] as usize]
                        & crate::game::MOVE
                        != 0;
                    assert_eq!(blocked, !open);
                }
            }
            for &gate in &dead {
                assert!(!m.world.game.units.contains_key(&gate));
            }
            let frame = delivery
                .capture_selected_with_tick_events(&mut m, &selected, &report.events)
                .unwrap()
                .unwrap();
            for slot in 0..2 {
                let snapshot = frame.recipient(slot).unwrap().into_json();
                for &gate in &dead {
                    assert!(
                        !snapshot["units"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|row| row[0] == gate)
                    );
                }
            }
            assert!(m.tick_state.fault().is_none());
            if m.finish.is_some() {
                let start = *finish_tick.get_or_insert(m.world.game.tick);
                if m.world.game.tick >= start + 8 {
                    break;
                }
            }
        }
        assert!(saw_paid && saw_open && saw_closed && saw_reopened);
        assert_eq!(dead.len(), 2);
        assert!(!m.world.game.units[&id].dig);
        assert_eq!(
            m.world
                .game
                .units
                .values()
                .filter(|u| m.rules.kinds[u.kind].name == "gate")
                .count(),
            1
        );
        assert_eq!(m.finish.as_ref().unwrap().winner, 0);
        assert!(m.world.game.tick >= finish_tick.unwrap() + 8);
    }
}

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../tests/fixtures/gate-lifecycle.json")).unwrap()
}
fn number(value: &serde_json::Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().unwrap(), 16).unwrap())
}

#[test]
fn eng12_original_gate_placement_and_grid_rounding_match_exactly() {
    let fixture = fixture();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    for row in fixture["placements"].as_array().unwrap() {
        let (mut m, _) = scene();
        let world = &mut m.world;
        let cell = 12 * 60 + 15;
        let ch = match row["terrain"].as_str().unwrap() {
            "blocked" | "hidden" => b'H',
            "road" => b'D',
            "path" => b'+',
            "mud" => b'M',
            _ => b'.',
        };
        world.set_cell(cell, ch).unwrap();
        let dir = number(&row["dir"]);
        assert_eq!(
            square_direction(dir).to_bits(),
            number(&row["square"]).to_bits()
        );
        let result = placement(
            world,
            FortKind::Gate,
            Point {
                x: row["x"].as_f64().unwrap(),
                z: row["z"].as_f64().unwrap(),
            },
            dir,
            Some(0),
            |at| row["terrain"] != "hidden" || world.game.cell_of(at.x, at.z) != Some(cell),
        );
        let expected = &row["result"];
        if expected["ok"] == true {
            assert_eq!(
                result.unwrap().iter().map(|c| c.cell).collect::<Vec<_>>(),
                expected["cells"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| c.as_u64().unwrap() as usize)
                    .collect::<Vec<_>>(),
                "{row}"
            );
        } else {
            let reason = match result.unwrap_err() {
                crate::field_works::Error::Refused(reason) => reason.as_str(),
                error => panic!("{error:?}"),
            };
            assert_eq!(reason, expected["reason"].as_str().unwrap(), "{row}");
        }
    }
}

#[test]
fn eng12_original_gate_timer_bucket_hysteresis_and_passage_match_every_step() {
    let fixture = fixture();
    for trace in fixture["traces"].as_array().unwrap() {
        let (mut m, friend) = scene();
        let troop = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 1 && m.rules.kinds[u.kind].name == "rifle")
            .unwrap()
            .id;
        let gate = m.next_unit_id;
        let cell = 15 * 60 + 15;
        m.world
            .set_cell(cell, trace["surface"].as_str().unwrap().as_bytes()[0])
            .unwrap();
        m.place_starting_building(0, "gate", cell).unwrap();
        // Isolated component fixture: exactly the original Gate and two ground actors.
        m.world
            .game
            .units
            .retain(|id, _| [gate, friend, troop].contains(id));
        m.grid.rebuild(&m.world.game.units);
        relocate(&mut m, troop, Point { x: 90., z: 90. });
        relocate(&mut m, friend, Point { x: 90., z: 94. });
        for (index, row) in trace["frames"].as_array().unwrap().iter().enumerate() {
            for (actor, patch) in row["action"]["patch"].as_object().unwrap() {
                let id = match actor.as_str() {
                    "gate" => gate,
                    "troop" => troop,
                    "friend" => friend,
                    _ => panic!("{actor}"),
                };
                let u = m.world.game.units.get_mut(&id).unwrap();
                for (key, value) in patch.as_object().unwrap() {
                    match key.as_str() {
                        "x" => u.x = value.as_f64().unwrap(),
                        "z" => u.z = value.as_f64().unwrap(),
                        "hp" => u.hp = value.as_f64().unwrap(),
                        "owner" => u.owner = value.as_u64().unwrap() as usize,
                        "riding" => u.riding = value.as_u64().unwrap() != 0,
                        "type" => u.kind = m.rules.kind(value.as_str().unwrap()).unwrap(),
                        "air" => u.air = (!value.is_null()).then_some(crate::game::PlaneState::Out),
                        _ => panic!("unrepresented Gate input {key}"),
                    }
                }
                m.grid.update(id, u);
            }
            m.step_gate(gate, row["action"]["dt"].as_f64().unwrap())
                .unwrap();
            let actual = m.commands.units[&gate].gate.unwrap();
            assert_eq!(
                actual.open,
                row["open"].as_bool(),
                "{} {index}",
                trace["name"]
            );
            assert_eq!(
                actual.remaining.to_bits(),
                number(&row["timer"]).to_bits(),
                "{} {index}",
                trace["name"]
            );
            assert_eq!(
                m.world.game.chars[cell],
                row["ch"].as_str().unwrap().as_bytes()[0],
                "{} {index}",
                trace["name"]
            );
            // Gate openness uses the existing bit, with the existing owner stance defaults.
            assert_eq!(
                row["flags"].as_u64().unwrap() & 65536,
                if actual.open == Some(true) { 65536 } else { 0 }
            );
        }
    }
}

#[test]
fn eng12_gate_projection_uses_the_existing_owner_flag_and_recipient_mask() {
    let (mut m, _) = scene();
    let id = m.next_unit_id;
    let cell = 15 * 60 + 15;
    m.place_starting_building(0, "gate", cell).unwrap();
    m.step_gate(id, 0.05).unwrap();
    m.world.game.reveal = true;
    let mut delivery = DeliveryState::new(&m).unwrap();
    let frame = delivery.capture(&mut m, &[true; 2], &[false; 2]).unwrap();
    for slot in 0..2 {
        let projection = frame.recipient(slot).unwrap();
        let row = projection.fields["units"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row[0] == id)
            .unwrap();
        assert_eq!(row.as_array().unwrap().len(), 15);
        assert_eq!(
            row[12].as_u64().unwrap() & 65536,
            if slot == 0 { 65536 } else { 0 }
        );
        assert_eq!(row[14], 1.);
    }
}

#[test]
fn eng12_occupied_paid_gate_waits_and_cancels_without_an_orphan_actor() {
    let (mut m, id) = scene();
    let blocker = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && u.id != id && !m.rules.kinds[u.kind].structure)
        .unwrap()
        .id;
    for cell in [12 * 60 + 14, 12 * 60 + 16] {
        m.world.set_cell(cell, b'T').unwrap();
    }
    relocate(&mut m, blocker, Point { x: 31., z: 25. });
    assert_eq!(m.execute_command(0, &dig(id, false), false).unwrap(), None);
    assert_eq!(m.field_works.jobs()[&id].cells.len(), 1);
    let mut delivery = DeliveryState::new(&m).unwrap();
    for _ in 0..3 {
        m.apply_field_work_unit_dt(id, 10.).unwrap();
        assert!(m.field_works.jobs()[&id].waits > 0);
        collect(&mut m, &mut delivery);
    }
    assert_eq!(
        m.execute_command(
            0,
            &Command::Stop {
                ids: vec![i64::from(id)]
            },
            false
        )
        .unwrap(),
        None
    );
    assert!(!m.field_works.is_active(id));
    assert_eq!(m.start.economy.players[0].mp, 350.);
    m.advance_tick().unwrap();
    collect(&mut m, &mut delivery);
    assert!(
        !m.world
            .game
            .units
            .values()
            .any(|u| m.rules.kinds[u.kind].name == "gate")
    );
}

fn publish_mover(m: &mut Match, kind: &str) -> u32 {
    let id = m.next_unit_id;
    let draft = crate::unit_publication::StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        0,
        m.rules.kind(kind).unwrap(),
        id,
        m.world.game.units.len() as u32,
        m.start.players[0].spawn,
        Some(Point { x: 30., z: 27. }),
    )
    .unwrap();
    m.publish_starting_unit(draft).unwrap();
    id
}

#[test]
fn eng12_open_and_closed_gate_bodies_keep_prototype_traffic_and_separation() {
    // Source local-traffic.js:101 and sim.js:5198-5229 never exempt an open Gate.
    for open in [true, false] {
        for kind in ["rifle", "tank"] {
            for gate_first in [true, false] {
                let (mut m, _) = scene();
                let early = (!gate_first).then(|| publish_mover(&mut m, kind));
                let gate = m.next_unit_id;
                m.place_starting_building(0, "gate", 15 * 60 + 15).unwrap();
                if open {
                    m.step_gate(gate, 0.05).unwrap();
                }
                let id = early.unwrap_or_else(|| publish_mover(&mut m, kind));
                m.world
                    .game
                    .units
                    .retain(|other, _| [gate, id].contains(other));
                m.grid.rebuild(&m.world.game.units);
                relocate(&mut m, id, Point { x: 31., z: 27. });
                let mut mover = m.world.game.units[&id].clone();
                mover.ghost = 0.;
                let steer = crate::traffic::traffic_step(
                    &m.world.game,
                    &m.rules,
                    &m.grid,
                    &mut mover,
                    Some(Point { x: 31., z: 37. }),
                    0.05,
                );
                assert!(
                    steer.blocked,
                    "{kind}, open={open}, gate_first={gate_first}"
                );
                assert!(steer.goal.is_none());

                let min = (m.rules.kinds[mover.kind].radius
                    + m.rules.kinds[m.world.game.units[&gate].kind].radius)
                    * 0.8;
                let distance = min * 0.75;
                let before = 31. - distance;
                relocate(&mut m, id, Point { x: 31., z: before });
                let solid = m.rules.kinds[mover.kind].ground_vehicle();
                let push = (min - distance) / 2. * if solid { 2. } else { 1. };
                crate::separation::separate_crowds(&mut m.world.game, &m.rules, &mut m.grid);
                assert!(
                    (m.world.game.units[&id].z - (before - push)).abs() < 1e-12,
                    "{kind}, open={open}, gate_first={gate_first}"
                );
                assert_eq!(m.world.game.units[&gate].at(), Point { x: 31., z: 31. });
            }
        }
    }
}

#[test]
fn eng12_gate_paths_follow_open_terrain_while_bodies_still_obstruct_passage() {
    let source = fixture();
    assert_eq!(number(&source["passageRuns"][0]["z"]), 37.);
    assert!(number(&source["passageRuns"][1]["z"]) < 31.);
    for surface in *b".D+M" {
        for kind in ["rifle", "tank"] {
            let (mut m, _) = scene();
            let id = publish_mover(&mut m, kind);
            for control in m.commands.units.values_mut() {
                control.auto_cast = false;
                control.auto_retreat = false;
                control.hold_fire = true;
            }
            for x in 0..60 {
                if !(14..=16).contains(&x) {
                    m.world.set_cell(15 * 60 + x, b'K').unwrap();
                }
            }
            for x in 14..=16 {
                m.world.set_cell(15 * 60 + x, surface).unwrap();
            }
            let move_to = Command::from_json(&json!({"t":"move","orders":[[id,30.,37.]]})).unwrap();
            let mut delivery = DeliveryState::new(&m).unwrap();
            // The retained Source control has the same terrain gap with no Gate actors.
            assert_eq!(m.execute_command(0, &move_to, false).unwrap(), None);
            for _ in 0..160 {
                m.advance_tick().unwrap();
                collect(&mut m, &mut delivery);
            }
            assert!(
                m.world.game.units[&id].z > 35.5,
                "no-Gate control: {kind}, surface={}, unit={:?}",
                char::from(surface),
                m.world.game.units[&id]
            );
            m.execute_command(
                0,
                &Command::Stop {
                    ids: vec![i64::from(id)],
                },
                false,
            )
            .unwrap();
            relocate(&mut m, id, Point { x: 30., z: 27. });
            let mut gates = Vec::new();
            for x in 14..=16 {
                gates.push(m.next_unit_id);
                m.place_starting_building(0, "gate", 15 * 60 + x).unwrap();
            }
            let enemy = m
                .world
                .game
                .units
                .values()
                .find(|u| u.owner == 1 && m.rules.kinds[u.kind].name == "rifle")
                .unwrap()
                .id;
            m.world.game.units.get_mut(&enemy).unwrap().hold_pos = true;
            for open in [true, false, true] {
                // Keep mobile bodies outside the occupied-passage radius when changing state.
                m.execute_command(
                    0,
                    &Command::Stop {
                        ids: vec![i64::from(id)],
                    },
                    false,
                )
                .unwrap();
                relocate(&mut m, id, Point { x: 30., z: 27. });
                relocate(
                    &mut m,
                    enemy,
                    Point {
                        x: if open { 90. } else { 41. },
                        z: 31.,
                    },
                );
                for _ in 0..12 {
                    m.advance_tick().unwrap();
                    collect(&mut m, &mut delivery);
                }
                assert!(
                    gates
                        .iter()
                        .all(|gate| m.commands.units[gate].gate.unwrap().open == Some(open))
                );
                for &gate in &gates {
                    let cell = m.world.game.units[&gate].cells.as_ref().unwrap()[0] as usize;
                    assert_eq!(m.world.game.flags[cell] & crate::game::MOVE == 0, open);
                    if open {
                        assert_eq!(m.world.game.chars[cell], surface);
                    }
                }
                // Source findPath uses terrain, so the open cells admit a route despite actor contact.
                assert_gate_route(&m, id, open);
                assert_eq!(m.execute_command(0, &move_to, false).unwrap(), None);
                for _ in 0..160 {
                    m.advance_tick().unwrap();
                    collect(&mut m, &mut delivery);
                    assert!(
                        m.world.game.units[&id].z < 30.,
                        "{kind}, surface={}, open={open}",
                        char::from(surface)
                    );
                    assert!(m.tick_state.fault().is_none());
                }
            }
        }
    }
}

fn assert_gate_route(m: &Match, id: u32, open: bool) {
    use crate::{
        navigation::TerrainView,
        pathfinding::{PathRules, PathStatus, PathTerrain, Pathfinding},
    };
    let g = &m.world.game;
    let near = vec![0; g.flags.len()];
    let hazards = vec![false; g.flags.len()];
    let terrain = PathTerrain {
        navigation: TerrainView {
            width: g.w,
            height: g.h,
            flags: &g.flags,
            heights: g.height.as_deref(),
            world_known: false,
            knowledge_id: 0,
            versions: Default::default(),
            changed: None,
            changes: &[],
        },
        near_walls: &near,
        known_mines: &hazards,
        fires: &hazards,
        bunkers: &[],
        wear: None,
        wetness: 0.,
        roads: true,
    };
    let path = Pathfinding::new()
        .find_path(
            terrain,
            PathRules::compiled().unwrap(),
            &m.rules.kinds[g.units[&id].kind],
            Point { x: 30., z: 27. },
            Point { x: 30., z: 37. },
            None,
        )
        .unwrap();
    assert_eq!(
        path.status,
        if open {
            PathStatus::Found
        } else {
            PathStatus::Unreachable
        }
    );
    if open {
        assert_eq!(path.waypoints.last(), Some(&Point { x: 30., z: 37. }));
    }
}

#[test]
fn base48_required_tiers_admit_buy_build_dig_and_entrench_in_ordinary_matches() {
    for value in [
        json!({"t":"buy","unit":"mortar"}),
        json!({"t":"build","kind":"pillbox","x":30,"z":25}),
        json!({"t":"dig","kind":"wall","x":30,"z":25}),
        json!({"t":"entrench","fort":"wall","pattern":"line","x":30,"z":25,"x2":40,"z2":25}),
    ] {
        let (mut m, id) = scene();
        tiers(&mut m, 2);
        let mut value = value;
        value["ids"] = json!([id]);
        assert_eq!(
            m.execute_command(0, &Command::from_json(&value).unwrap(), false)
                .unwrap(),
            None,
            "{value}"
        );
        m.advance_tick().unwrap();
        assert!(m.story[0].mp_spent > 0., "{value}");
        let mut delivery = DeliveryState::new(&m).unwrap();
        collect(&mut m, &mut delivery);
    }
    let (mut m, _) = scene();
    let command = Command::from_json(&json!({"t":"buy","unit":"ranger"})).unwrap();
    tiers(&mut m, 2);
    assert_eq!(
        m.execute_command(0, &command, false).unwrap(),
        Some(Denial::Tier)
    );
    assert_eq!(m.start.economy.players[0].mp, 500.);
    tiers(&mut m, 3);
    assert_eq!(m.execute_command(0, &command, false).unwrap(), None);
    m.advance_tick().unwrap();
    let mut delivery = DeliveryState::new(&m).unwrap();
    collect(&mut m, &mut delivery);
}

#[test]
fn eng12_unrepresented_modes_and_logistics_cannot_publish_paid_or_waiting_gates() {
    for (mode, game_mode, logistics) in [
        (Mode::Tutorial, Some(crate::game::ModeKind::Tutorial), false),
        (Mode::World, Some(crate::game::ModeKind::World), false),
        (Mode::Horde, Some(crate::game::ModeKind::Horde), false),
        (Mode::Classic, None, false),
        (Mode::Default, Some(crate::game::ModeKind::Classic), false),
        (Mode::Default, None, true),
    ] {
        let (mut m, id) = scene();
        m.options.mode = mode;
        m.world.game.mode = game_mode;
        m.options.logistics = logistics;
        let before = m.next_unit_id;
        assert_eq!(
            m.execute_dig(
                0,
                &[i64::from(id)],
                FortKind::Gate,
                Point { x: 30., z: 25. },
                None,
                false
            )
            .unwrap(),
            Some(Denial::Unsupported)
        );
        assert_eq!(
            m.queue_dig(
                0,
                &[i64::from(id)],
                FortKind::Gate,
                Point { x: 30., z: 25. },
                None,
                false
            )
            .unwrap(),
            Some(Denial::Unsupported)
        );
        assert_eq!(m.next_unit_id, before);
        assert_eq!(m.start.economy.players[0].mp, 500.);
        assert!(!m.field_works.is_active(id));
        assert!(m.commands.units[&id].orders.is_empty());
    }
}
