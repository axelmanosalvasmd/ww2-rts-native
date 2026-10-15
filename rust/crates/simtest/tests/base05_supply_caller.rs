//! Genuine capture and mutable crossing histories through the ordinary runner.
use flate2::read::GzDecoder;
use serde_json::{Value, json};
use std::{collections::BTreeSet, io::Read};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    environment::{self, Blast, DamageContext},
    game::{Point, VBLOCK},
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::TickPhase,
    recipient_snapshot::SnapshotCache,
    weather::WeatherChoice,
    world::CellChange,
};

const CROSSING: [usize; 3] = [3239, 3240, 3241];

fn bits(value: f64) -> Value {
    json!(format!("{:016x}", value.to_bits()))
}

fn number_bits(value: Value) -> Value {
    match value {
        Value::Number(number) => bits(number.as_f64().unwrap()),
        Value::Array(rows) => Value::Array(rows.into_iter().map(number_bits).collect()),
        other => other,
    }
}

fn actors(m: &Match) -> Value {
    json!(m.world.game.units.values().map(|u| json!([
        u.id, u.owner, m.rules.kinds[u.kind].name,
        bits(u.hp), bits(u.x), bits(u.z)
    ])).collect::<Vec<_>>())
}

fn retained(m: &Match, initial: &Value) {
    let rows = initial.as_array().unwrap();
    assert_eq!(m.world.game.units.len(), rows.len());
    for row in rows {
        let id = row[0].as_u64().unwrap() as u32;
        let u = &m.world.game.units[&id];
        assert_eq!(json!([u.id, u.owner, m.rules.kinds[u.kind].name, bits(u.hp)]),
            json!(row.as_array().unwrap()[..4]));
        if m.rules.kinds[u.kind].building {
            let structure = m.world.structures.iter().find(|s| s.entity_id == Some(id)).unwrap();
            assert!(!structure.cells.is_empty());
            for &cell in &structure.cells {
                assert_eq!(m.building_cells[&cell].unit, id);
            }
        }
    }
    assert!(m.finish.is_none());
}

fn projection(m: &Match, initial: &Value) -> Value {
    retained(m, initial);
    let g = &m.world.game;
    let p = &m.start.points[0];
    let recipient = SnapshotCache::new(m).unwrap().recipient(0).unwrap();
    let local = (36..43).flat_map(|y| (36..45).map(move |x| y * 80 + x));
    json!({
        "tick": g.tick, "seed": m.start.environment_seed,
        "actors": actors(m),
        "wallets": m.start.economy.players.iter().map(|p| json!([bits(p.mp), bits(p.inc), bits(p.vp)])).collect::<Vec<_>>(),
        "point": [p.owner.map_or(-1, |owner| owner as i64), p.capper.map_or(-1, |owner| owner as i64), bits(p.progress), p.cut, p.contested, p.on_point],
        "recipient": number_bits(recipient.fields["points"].clone()),
        "terrain": local.map(|c| json!([
            c, char::from(g.chars[c]).to_string(),
            char::from(g.ground.as_ref().unwrap()[c]).to_string(),
            char::from(g.objects.as_ref().unwrap()[c]).to_string(),
            g.flags[c], g.cell_hp[c].to_bits(), m.world.cell_state[c]
        ])).collect::<Vec<_>>()
    })
}

fn make(case: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap()).unwrap();
    Match::new(&map, MatchOptions {
        seed: 701, mode: Mode::Conquest,
        players: vec![
            PlayerConfig { name: "a".into(), team: 0, faction: Faction::Usa },
            PlayerConfig { name: "b".into(), team: 1, faction: Faction::Germany },
        ],
        shuffle: false, army: ArmySize::Standard, weather: WeatherChoice::Map,
        weather_seed: Some(1), living_weather: false, map_key: None,
        tech: false, supply: case["supply"].as_bool().unwrap(), logistics: false, sandbox: false,
    }).unwrap()
}

fn blast(m: &mut Match, input: &Value) {
    let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let skip = BTreeSet::new();
    let rain = m.world.game.rain;
    let wet = m.world.game.wet.unwrap_or(0.0);
    let mut context = environment::Context {
        tick: m.world.game.tick as u64, seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id, unit_order: &ids, grid: &m.grid,
        rain, wet, wind_angle: m.start.wind.0, wind_strength: m.start.wind.1,
    };
    let mut damage = DamageContext::plain(&skip);
    damage.actor_order = Some(&ids);
    let effects = m.environment.damage_cells(&mut m.world, Blast {
        at: Point { x: input["x"].as_f64().unwrap(), z: input["z"].as_f64().unwrap() },
        radius: input["radius"].as_f64().unwrap(), damage: input["damage"].as_f64().unwrap(),
        scar: input["scar"].as_f64().unwrap(),
    }, &mut context, &mut damage).unwrap();
    assert!(effects.iter().all(|effect| !matches!(effect, environment::Effect::LastHit { .. })),
        "declared terrain blast must not damage a retained actor");
}

#[test]
fn genuine_capture_enemy_bridge_and_paid_rubble_fill_drive_point_supply() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/base05-supply-caller.json.gz");
    let bytes = std::fs::read(path).expect("Root must grant authentic BASE05 Source recording first");
    let mut text = String::new();
    GzDecoder::new(bytes.as_slice()).read_to_string(&mut text).unwrap();
    let f: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.iter().map(|c| c["name"].as_str().unwrap()).collect::<Vec<_>>(),
        ["bridge-on", "bridge-off", "rubble-on"]);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        assert_eq!(case["supply"].as_bool().unwrap(), name != "bridge-off");
        let mut m = make(case);
        let random = m.start.random.snapshot();
        let initial = &case["initialActors"];
        assert_eq!(initial.as_array().unwrap().len(), 10);
        assert_eq!(actors(&m), *initial);
        assert_eq!(projection(&m, initial), case["initial"]);
        let mut commands = 0;
        let mut observations = BTreeSet::new();
        for action in case["records"].as_array().unwrap() {
            let label = action["label"].as_str().unwrap_or("terrain-input");
            match action["op"].as_str().unwrap() {
                "command" => {
                    let owner = action["owner"].as_u64().unwrap() as usize;
                    let command = Command::from_json(&action["raw"]).unwrap();
                    let mp = m.start.economy.players[owner].mp;
                    assert_eq!(m.execute_command(owner, &command, false).unwrap(), None, "{name}:{label}");
                    assert_eq!(bits(mp - m.start.economy.players[owner].mp), action["debit"]);
                    if label == "paid-real-fill" { assert!(mp > m.start.economy.players[owner].mp); }
                    commands += 1;
                }
                "ground" => {
                    assert_eq!(action["cells"], json!([3159, 3160, 3161]));
                    for &c in &[3159, 3160, 3161] {
                        m.world.mutate_cell(c, CellChange { ground: Some(b'W'), ..CellChange::default() }).unwrap();
                    }
                }
                "blast" => blast(&mut m, &action["blast"]),
                "steps" => {
                    let frames = action["frames"].as_array().unwrap();
                    let limit = action["limit"].as_u64().unwrap() as usize;
                    assert!(frames.len() <= limit);
                    if action["condition"].is_null() { assert_eq!(frames.len(), limit); }
                    for expected in frames {
                        let old_tick = m.world.game.tick;
                        let report = m.advance_tick().unwrap_or_else(|e| panic!("{name}:{label}: {e}"));
                        assert_eq!(report.tick, old_tick + 1);
                        assert_eq!(m.start.random.snapshot(), random, "peaceful history must consume no purpose-stream draw");
                        assert!(report.retired.is_empty());
                        for phase in [TickPhase::Movement, TickPhase::Debris, TickPhase::FieldWork,
                            TickPhase::PointSupply, TickPhase::PointCaptureIncome, TickPhase::Victory] {
                            assert!(report.phases.contains(&phase), "{name}:{label}: {phase:?}");
                            assert!(!report.unavailable.contains(&phase));
                        }
                        assert_eq!(projection(&m, initial), *expected, "{name}:{label}: tick{}", report.tick);
                    }
                    match action["condition"].as_str() {
                        Some("captured") => {
                            assert_eq!(m.start.points[0].owner, Some(0));
                            assert_eq!(m.start.points[0].progress, 1.0);
                        }
                        Some("arrived") => {
                            let id = action["unit"].as_u64().unwrap() as u32;
                            assert!(m.world.game.units[&id].path.is_empty());
                            assert!(m.world.game.units[&id].world_goal.is_none());
                        }
                        Some("filled") => assert!(CROSSING.iter().all(|&c| m.world.game.chars[c] == b'D')),
                        None => {}
                        other => panic!("unrepresented BASE05 condition {other:?}"),
                    }
                    if label == "normal-debris-settlement" {
                        if name == "rubble-on" {
                            assert_eq!(m.world.game.chars[CROSSING[1]], b'R');
                            assert_ne!(m.world.game.flags[CROSSING[1]] & VBLOCK, 0);
                        } else {
                            assert!(CROSSING.iter().all(|&c| m.world.game.chars[c] == b'W'));
                        }
                    }
                    continue;
                }
                "observe" => {
                    observations.insert(label.to_owned());
                    let cut = match label {
                        "enemy-on-bridge" | "blown-bridge-cuts-point" => m.options.supply,
                        "settled-rubble-cuts-point" => true,
                        "bridge-or-road-supplies-owned-point" | "held-point-exempts-near-enemy"
                            | "cleared-road-restores-point" => false,
                        other => panic!("unrepresented BASE05 observation {other}"),
                    };
                    assert_eq!(action["cut"].as_bool().unwrap(), cut);
                    assert_eq!(m.start.points[0].owner, Some(0));
                    assert_eq!(m.start.points[0].cut, cut);
                    assert_eq!(m.start.economy.players[0].inc, if cut { 4.0 } else { 5.0 });
                }
                other => panic!("unrepresented BASE05 input {other}"),
            }
            assert_eq!(projection(&m, initial), action["expected"], "{name}:{label}");
        }
        assert_eq!(commands, case["commands"].as_u64().unwrap());
        assert_eq!(json!(m.world.game.tick), case["ticks"]);
        assert!(observations.contains("bridge-or-road-supplies-owned-point"));
        if name == "rubble-on" {
            assert!(observations.contains("settled-rubble-cuts-point"));
            assert!(observations.contains("cleared-road-restores-point"));
        } else {
            assert!(observations.contains("enemy-on-bridge"));
            assert!(observations.contains("held-point-exempts-near-enemy"));
            assert!(observations.contains("blown-bridge-cuts-point"));
        }
    }
}
