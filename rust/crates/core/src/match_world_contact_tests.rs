//! Replay only the contact component boundary after a real World constructor.
//! Recorded movement and wetness are inputs, not proof of constructor or whole-tick parity.
use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{LOCAL_DEFENDER_OWNER, Point},
    map::AuthoritativeMap,
    match_commands::UnitControl,
    match_runtime::{MatchOptions, PlayerConfig},
    random::MatchRandom,
    unit_spawn::initial_unit,
    weather::WeatherChoice,
};
use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_str(include_str!("../tests/fixtures/world-contact.json")).unwrap()
}
fn real(v: &Value) -> f64 {
    let bits = u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap();
    let n = f64::from_bits(bits);
    assert_eq!(n.to_bits(), v["value"].as_f64().unwrap().to_bits());
    n
}
fn at(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn pose(v: &Value) -> Pose {
    Pose {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
        rot: v["rot"].as_f64().unwrap(),
    }
}
fn make(f: &Value, case: &Value) -> (Match, u32, usize) {
    let mut raw = f["map"].clone();
    let c = case["initial"]["c"].as_u64().unwrap() as usize;
    let ch = case["inputs"]["ch"].as_str().unwrap_or(".");
    let y = c / 512;
    let mut row = raw["rows"][y].as_str().unwrap().to_owned();
    row.replace_range(c % 512..c % 512 + 1, ch);
    raw["rows"][y] = json!(row);
    if case["inputs"]["height"].as_i64() == Some(-1) {
        let mut heights = vec!["0".repeat(512); 512];
        heights[y].replace_range(c % 512..c % 512 + 1, "a");
        raw["heights"] = json!(heights);
    }
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&raw).unwrap()).unwrap();
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
            weather_seed: None,
            living_weather: false,
            map_key: None,
            tech: true,
            supply: false,
            logistics: false,
            sandbox: false,
        },
        MatchRandom::new(701),
    )
    .unwrap();
    let boundary = &case["contacts"][0];
    let id = boundary["unit"]["id"].as_u64().unwrap() as u32;
    let owner = match boundary["unit"]["owner"].as_i64().unwrap() {
        -1 => LOCAL_DEFENDER_OWNER,
        n if n >= 0 => n as usize,
        _ => panic!("only Source -1 is a canonical local defender"),
    };
    let kind = m
        .rules
        .kind(boundary["unit"]["type"].as_str().unwrap())
        .unwrap();
    // Restore the declared actor boundary without changing native wind, seed or streams.
    let mut u = initial_unit(
        &m.rules,
        Catalog::compiled().unwrap(),
        kind,
        id,
        owner,
        at(&case["initial"]["pose"]),
    )
    .unwrap();
    u.path = boundary["unit"]["path"]
        .as_array()
        .unwrap()
        .iter()
        .map(at)
        .collect();
    let mut control = UnitControl::new_for_mode(&u, &m.rules, Mode::World).unwrap();
    control.guard_home = case["initial"]["guardHome"]
        .as_object()
        .map(|_| at(&case["initial"]["guardHome"]));
    m.commands.units.insert(id, control);
    m.world.game.units.insert(id, u);
    m.next_unit_id = m.next_unit_id.max(id.checked_add(1).unwrap());
    m.grid = crate::grid::SpatialGrid::new(&m.world.game.units, 16.0);
    m.world.game.wear.as_mut().unwrap()[c] = real(&boundary["wear"]) as f32;
    m.world.surface_wear[c] = real(&boundary["surfaceWear"]) as f32;
    m.world.cell_state[c] = boundary["state"].as_u64().unwrap() as u8;
    if let Some(owner) = case["inputs"]["mineOwner"].as_i64() {
        m.world.mines.insert(c, owner as i32);
    }
    assert_eq!(
        m.world.game.chars[c],
        boundary["ch"].as_str().unwrap().as_bytes()[0]
    );
    assert_eq!(
        m.world.game.flags[c],
        boundary["flags"].as_u64().unwrap() as u16
    );
    assert_eq!(
        m.world.game.height.as_ref().map_or(0, |h| h[c]),
        boundary["height"].as_i64().unwrap() as i8
    );
    m.world.take_journal();
    m.world.take_new_cells();
    (m, id, c)
}
fn movement_input(m: &mut Match, id: u32, row: &Value) -> Pose {
    m.world.game.tick = row["tick"].as_i64().unwrap();
    m.world.game.wet = row["wx"].get("wet").and_then(Value::as_f64);
    let unit = m.world.game.units.get_mut(&id).unwrap();
    unit.x = real(&row["unit"]["x"]);
    unit.z = real(&row["unit"]["z"]);
    let before = pose(&row["before"]);
    assert_eq!(
        math::hypot(unit.x - before.x, unit.z - before.z).to_bits(),
        real(&row["distance"]).to_bits()
    );
    before
}
fn cell_row(row: &crate::world::CellRow) -> Value {
    let data = &row.data;
    assert!(
        !data.mine
            && data.section.is_none()
            && data.rail.is_none()
            && data.station.is_none()
            && data.trench_front.is_none()
            && data.mine_material.is_none()
    );
    json!([row.cell, char::from(row.ch).to_string(), row.height, row.state,
        {"ground": char::from(data.ground).to_string(), "object": char::from(data.object).to_string(),
         "material": data.material, "groundMaterial": data.ground_material,
         "groundWear": f64::from(data.ground_wear)}])
}

#[test]
fn canonical_world_contacts_match_original_commander_and_local_boundaries() {
    let f = fixture();
    assert_eq!(f["cases"].as_array().unwrap().len(), 12);
    let cases = f["cases"].as_array().unwrap();
    for case in cases[..10].iter().chain(&cases[11..]) {
        let (mut m, id, c) = make(&f, case);
        let random = m.start.random.snapshot();
        let wind = m.start.wind;
        let seed = m.start.environment_seed;
        let mut prior_dust = case["initial"]["dust"].as_f64();
        let mut shots = Vec::new();
        for pair in case["contacts"].as_array().unwrap().as_chunks::<2>().0 {
            let input = &pair[0];
            let expected = &pair[1];
            let before = movement_input(&mut m, id, input);
            let terrain = m.world.game.terrain_version;
            let versions = m.world.versions;
            // The workload mover's premature dust must not survive an infantry no-op or wet/mud contact.
            m.world.game.units.get_mut(&id).unwrap().dust = Some(999.0);
            m.apply_moved_ground_contact_emitting(id, before, prior_dust, &mut |effect| {
                shots.push(effect.to_json());
            })
            .unwrap();
            assert_eq!(
                m.world.game.wear.as_ref().unwrap()[c].to_bits(),
                expected["wearF32Bits"].as_u64().unwrap() as u32,
                "{} wear",
                case["name"]
            );
            assert_eq!(
                f64::from(m.world.game.wear.as_ref().unwrap()[c]).to_bits(),
                real(&expected["wear"]).to_bits()
            );
            assert_eq!(
                m.world.surface_wear[c].to_bits(),
                expected["surfaceWearF32Bits"].as_u64().unwrap() as u32
            );
            assert_eq!(
                m.world.cell_state[c],
                expected["state"].as_u64().unwrap() as u8
            );
            assert_eq!(
                char::from(m.world.game.chars[c]).to_string(),
                expected["ch"]
            );
            assert_eq!(
                m.world.game.flags[c],
                expected["flags"].as_u64().unwrap() as u16
            );
            assert_eq!(json!(m.world.journal()), expected["journal"]);
            let rows = json!(m.world.new_cells().iter().map(cell_row).collect::<Vec<_>>());
            assert!(
                super::tests::exact_json_numbers(&rows, &expected["newCells"]),
                "{} ordered complete cell rows",
                case["name"]
            );
            assert_eq!(
                u64::from(m.world.game.terrain_version - terrain),
                expected["versions"]["terrain"].as_u64().unwrap()
                    - input["versions"]["terrain"].as_u64().unwrap()
            );
            for (delta, key) in [
                (
                    m.world.versions.obstruction - versions.obstruction,
                    "obstruction",
                ),
                (m.world.versions.infantry - versions.infantry, "infantry"),
                (m.world.versions.vehicle - versions.vehicle, "vehicle"),
            ] {
                assert_eq!(
                    delta,
                    expected["versions"][key].as_u64().unwrap()
                        - input["versions"][key].as_u64().unwrap()
                );
            }
            assert_eq!(m.world.versions.naval, versions.naval);
            let unit = &m.world.game.units[&id];
            assert_eq!(unit.dust, expected["unit"]["dust"].as_f64());
            assert_eq!(unit.hp.to_bits(), real(&expected["unit"]["hp"]).to_bits());
            assert_eq!(unit.x.to_bits(), real(&expected["unit"]["x"]).to_bits());
            assert_eq!(unit.z.to_bits(), real(&expected["unit"]["z"]).to_bits());
            let path = json!(
                unit.path
                    .iter()
                    .map(|p| json!({"x":p.x,"z":p.z}))
                    .collect::<Vec<_>>()
            );
            assert!(super::tests::exact_json_numbers(
                &path,
                &expected["unit"]["path"]
            ));
            assert_eq!(
                json!(
                    m.world
                        .mines
                        .iter()
                        .map(|(c, owner)| json!([c, owner]))
                        .collect::<Vec<_>>()
                ),
                expected["mines"]
            );
            assert!(super::tests::exact_json_numbers(
                &json!(shots),
                &expected["shots"]
            ));
            assert_eq!(m.start.random.snapshot(), random);
            assert_eq!(m.start.wind, wind);
            assert_eq!(m.start.environment_seed, seed);
            prior_dust = unit.dust;
        }
    }
}

#[test]
fn world_contact_refusals_preserve_state_and_restore_precontact_dust() {
    let f = fixture();
    for boundary in [
        "hostile-mine",
        "owner",
        "runtime",
        "mode",
        "air",
        "rail",
        "naval",
        "structure",
        "operative",
        "garrison",
        "riding",
        "control",
        "logistics",
        "negative-mine",
        "capacity",
        "shape",
    ] {
        let index = match boundary {
            "hostile-mine" => 10,
            _ => 2,
        };
        let case = &f["cases"][index];
        let (mut m, id, c) = make(&f, case);
        let before = movement_input(&mut m, id, &case["contacts"][0]);
        match boundary {
            "owner" => m.world.game.units.get_mut(&id).unwrap().owner = LOCAL_DEFENDER_OWNER - 1,
            "runtime" => m.world_runtime = None,
            "mode" => m.world.game.mode = None,
            "air" => {
                m.world.game.units.get_mut(&id).unwrap().air = Some(crate::game::PlaneState::Base)
            }
            "rail" => {
                let kind = m.world.game.units[&id].kind;
                m.rules.kinds[kind].rail = true;
            }
            "naval" => {
                let kind = m.world.game.units[&id].kind;
                m.rules.kinds[kind].naval = true;
            }
            "structure" => {
                let kind = m.world.game.units[&id].kind;
                m.rules.kinds[kind].structure = true;
            }
            "operative" => m.commands.units.get_mut(&id).unwrap().operative = true,
            "garrison" => m.world.game.units.get_mut(&id).unwrap().garrison = Some(c),
            "riding" => m.world.game.units.get_mut(&id).unwrap().riding = true,
            "control" => {
                m.commands.units.remove(&id);
            }
            "logistics" => m.options.logistics = true,
            "negative-mine" => {
                m.world.set_cell(c, b'N').unwrap();
                m.world.mines.insert(c, -2);
            }
            "capacity" => {
                m.world.game.wear.as_mut().unwrap()[c] = 0.9999;
                m.world.game.terrain_version = u32::MAX;
            }
            "shape" => {
                m.world.game.flags.pop();
            }
            _ => {}
        }
        m.world.game.units.get_mut(&id).unwrap().dust = Some(-5.0);
        let world = format!("{:?}", m.world);
        let controls = format!("{:?}", m.commands.units);
        let queue = m.commands.queue.snapshot();
        let random = m.start.random.snapshot();
        m.world.game.units.get_mut(&id).unwrap().dust = Some(999.0);
        let error = m
            .apply_moved_ground_contact(id, before, Some(-5.0))
            .unwrap_err();
        if boundary == "hostile-mine" {
            assert!(error.contains("not connected"));
        }
        assert_eq!(
            format!("{:?}", m.world),
            world,
            "{boundary}: contact must not publish terrain, actors, rows, versions or clock"
        );
        assert_eq!(format!("{:?}", m.commands.units), controls);
        assert_eq!(m.commands.queue.snapshot(), queue);
        assert_eq!(m.start.random.snapshot(), random);
    }
    // The same sentinel is not an ordinary commander, even for an infantry no-op.
    let ordinary = super::tests::fixture();
    let (mut m, _, _) = super::tests::scene(&ordinary["cases"][0]);
    let id = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].infantry)
        .unwrap()
        .id;
    let unit = m.world.game.units.get_mut(&id).unwrap();
    let before = Pose {
        x: unit.x,
        z: unit.z,
        rot: unit.rot,
    };
    let prior_dust = unit.dust;
    unit.x += 0.25;
    unit.owner = LOCAL_DEFENDER_OWNER;
    let world = format!("{:?}", m.world);
    assert!(
        m.apply_moved_ground_contact(id, before, prior_dust)
            .is_err()
    );
    assert_eq!(format!("{:?}", m.world), world);
}

#[test]
fn mine_alliances_keep_ordinary_commanders_and_world_local_identity_distinct() {
    let ordinary = super::tests::fixture();
    for mine_owner in [0, 1] {
        let (mut m, id, c) = super::tests::scene(&ordinary["cases"][0]);
        m.options.players[1].team = 0;
        m.start.players[1].config.team = 0;
        m.start.economy.players[1].team = 0;
        m.world.game.players[1].team = 0;
        m.world.set_cell(c, b'N').unwrap();
        m.world.mines.insert(c, mine_owner);
        m.world.take_journal();
        m.world.take_new_cells();
        let before = Pose {
            x: 50.0,
            z: 51.0,
            rot: 0.0,
        };
        let wear = m.world.game.wear.as_ref().unwrap()[c];
        m.apply_moved_ground_contact(id, before, None).unwrap();
        assert_eq!(m.world.mines[&c], mine_owner);
        assert_eq!(m.world.game.chars[c], b'N');
        assert_eq!(m.world.game.wear.as_ref().unwrap()[c], wear);
        assert!(m.world.journal().is_empty() && m.world.new_cells().is_empty());
        for invalid_owner in [-1, -2] {
            m.world.mines.insert(c, invalid_owner);
            let prior_dust = m.world.game.units[&id].dust;
            let world = format!("{:?}", m.world);
            assert!(
                m.apply_moved_ground_contact(id, before, prior_dust)
                    .is_err()
            );
            assert_eq!(format!("{:?}", m.world), world);
        }
    }
    let f = fixture();
    let case = &f["cases"][8];
    let (mut m, id, c) = make(&f, case);
    let before = movement_input(&mut m, id, &case["contacts"][0]);
    // Source's absent mine owner also defaults to -1, but only World has that actor.
    m.world.mines.remove(&c);
    m.apply_moved_ground_contact(id, before, None).unwrap();
    assert_eq!(m.world.game.chars[c], b'N');
    assert!(m.world.mines.is_empty());
    assert!(m.world.journal().is_empty() && m.world.new_cells().is_empty());
}
