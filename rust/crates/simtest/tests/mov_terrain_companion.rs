//! Complete selected distance predicates through canonical constructors and commands.
//! This disclosed companion retains all constructor actors and footprints. The
//! cleared original Source callbacks remain a separate recorder, never native setup.
use std::collections::BTreeMap;

use serde_json::{Value, json};
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{Point, Smoke},
    map::AuthoritativeMap,
    match_commands::{Command, MoveOrder},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::SnapshotCache,
    weather::{WeatherChoice, WeatherKind},
};

const MANIFEST: &str = include_str!("../../../../harness/mov-terrain-scenes.json");
const PIN: &str = "e4e579a838aabae3b1d9dadbc17a0a87147bc52d";

fn fixture() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mov-terrain-companion.json.gz");
    let bytes = std::fs::read(path)
        .expect("fixture absent: Root must grant recording before native execution");
    let mut reader = flate2::read::GzDecoder::new(bytes.as_slice());
    let mut text = String::new();
    std::io::Read::read_to_string(&mut reader, &mut text).unwrap();
    let f: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(f["reference"], PIN);
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    assert_eq!(
        f["completed"], true,
        "first Source failure remains fatal: {}",
        f["failure"]
    );
    assert!(f["failure"].is_null());
    assert!(f["readbackFailure"].is_null());
    assert_eq!(f["originalCapture"]["sha256"].as_str().unwrap().len(), 64);
    let original_checks = f["originalCapture"]["checks"].as_array().unwrap();
    assert_eq!(original_checks.len(), 4);
    for (check, name) in original_checks.iter().zip([
        "Roads and mud",
        "Ground that wears",
        "Slope",
        "Wind, weather, dust and fire",
    ]) {
        assert_eq!(check["name"], name);
        assert_eq!(check["completed"], true);
        assert_eq!(check["sha256"].as_str().unwrap().len(), 64);
    }
    let manifest: Value = serde_json::from_str(MANIFEST).unwrap();
    assert_eq!(
        f["manifest"], manifest,
        "authored inputs, never expected native state"
    );
    let expected: Vec<_> = manifest["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| &s["name"])
        .collect();
    let actual: Vec<_> = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| &s["name"])
        .collect();
    assert_eq!(actual, expected, "all thirteen scenes in authored order");
    assert_eq!(actual.len(), 13);
    f
}
fn n(v: &Value) -> f64 {
    v.as_f64().expect("authored finite number")
}
fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}
fn at(v: &Value) -> Point {
    Point {
        x: n(&v[0]),
        z: n(&v[1]),
    }
}
fn scene_map(manifest: &Value, spec: &Value) -> Value {
    let w = manifest["map"]["w"].as_u64().unwrap() as usize;
    let h = manifest["map"]["h"].as_u64().unwrap() as usize;
    let surface = spec["surface"].as_str().unwrap();
    let rows: Vec<_> = (0..h)
        .map(|y| {
            if y < manifest["map"]["mudRows"].as_u64().unwrap() as usize && surface == "mud" {
                "M".repeat(w)
            } else if (1..=3).contains(&y) && surface == "road" {
                "D".repeat(w)
            } else {
                ".".repeat(w)
            }
        })
        .collect();
    let heights: Vec<_> = (0..h)
        .map(|y| {
            if y < 20 && surface == "hill" {
                "0".repeat(10) + &"1".repeat(10) + &"0".repeat(w - 20)
            } else {
                "0".repeat(w)
            }
        })
        .collect();
    json!({"name":format!("Canonical MOV companion {}",spec["name"].as_str().unwrap()),
        "w":w,"h":h,"rows":rows,"heights":heights,"spawns":manifest["map"]["spawns"],
        "points":manifest["map"]["points"],"weather":"clear"})
}
fn roster(m: &Match) -> Value {
    json!(
        m.world
            .game
            .units
            .values()
            .map(|u| json!({"id":u.id,
                "kind":m.rules.kinds[u.kind].name,"owner":u.owner,"cells":u.cells,
                "at":m.rules.kinds[u.kind].structure.then(||vec![bits(u.x),bits(u.z)])
            }))
            .collect::<Vec<_>>()
    )
}
fn structures(m: &Match) -> Value {
    json!(
        m.world
            .structures
            .iter()
            .map(|s| json!([s.id, s.entity_id, s.cells, s.section_ids]))
            .collect::<Vec<_>>()
    )
}
fn refresh(m: &mut Match) {
    m.grid.rebuild(&m.world.game.units);
    ww2_core::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    for owner in 0..m.world.game.players.len() {
        m.refresh_recipient_routing(owner)
            .expect("real post-placement recipient observation");
    }
}
fn frame(m: &Match, aliases: &BTreeMap<String, u32>) -> Value {
    let units: BTreeMap<_, _> = aliases.iter().map(|(name, id)| {
        let u = &m.world.game.units[id];
        (name.clone(), json!({"id":id,"x":bits(u.x),"z":bits(u.z),"rot":bits(u.rot),
            "speed":bits(u.move_speed),"pace":bits(u.pace),"path":u.path.iter().map(|p|vec![bits(p.x),bits(p.z)]).collect::<Vec<_>>(),
            "vx":bits(u.vx.unwrap_or(f64::NAN)),"vz":bits(u.vz.unwrap_or(f64::NAN))}))
    }).collect();
    json!({"tick":m.world.game.tick,"units":units})
}

struct Scene<'a> {
    m: Match,
    oracle: &'a Value,
    aliases: BTreeMap<String, u32>,
    retained: Value,
    retained_structures: Value,
    step: usize,
    command: usize,
}
impl<'a> Scene<'a> {
    fn new(manifest: &Value, spec: &Value, oracle: &'a Value) -> Self {
        let map = scene_map(manifest, spec);
        assert_eq!(
            map, oracle["map"],
            "map is computed independently from authored matrix"
        );
        let authoritative = AuthoritativeMap::from_json(&serde_json::to_vec(&map).unwrap())
            .expect("authentic companion map admission; original small cleared map is not claimed");
        let mut m = Match::new(
            &authoritative,
            MatchOptions {
                seed: 7301,
                mode: Mode::Conquest,
                players: vec![
                    PlayerConfig {
                        name: "A".into(),
                        team: 0,
                        faction: Faction::Usa,
                    },
                    PlayerConfig {
                        name: "B".into(),
                        team: 1,
                        faction: Faction::Germany,
                    },
                ],
                shuffle: false,
                army: ArmySize::Standard,
                weather: WeatherChoice::Fixed(WeatherKind::Clear),
                weather_seed: Some(1),
                living_weather: true,
                map_key: None,
                tech: false,
                supply: false,
                logistics: false,
                sandbox: false,
            },
        )
        .expect("real canonical Match constructor");
        assert_eq!(
            roster(&m),
            oracle["initial"],
            "constructor identities and footprint poses"
        );
        m.start.environment_seed = manifest["startup"]["environmentSeed"].as_u64().unwrap() as u32;
        m.start.wind = (
            n(&manifest["startup"]["wind"][0]),
            n(&manifest["startup"]["wind"][1]),
        );
        let retained = roster(&m);
        let retained_structures = structures(&m);
        let mut scene = Self {
            m,
            oracle,
            aliases: BTreeMap::new(),
            retained,
            retained_structures,
            step: 0,
            command: 0,
        };
        scene.dry();
        scene
            .m
            .validate_tick_state()
            .expect("first real tick admission, no bypass");
        let builder_kind = manifest["producer"]["builderKind"].as_str().unwrap();
        assert!(
            ww2_core::economy::builder_types(Mode::Conquest)
                .unwrap()
                .contains(&builder_kind)
        );
        let builder = scene
            .m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0 && scene.m.rules.kinds[u.kind].name == builder_kind)
            .expect("actual constructor actor admitted by native builder policy")
            .id;
        assert_eq!(json!(builder), oracle["builder"]);
        scene.m.start.economy.players[0].mp = n(&manifest["producer"]["wallet"]);
        let births = scene.issue(Command::Build {
            ids: vec![i64::from(builder)],
            kind: "motorpool".into(),
            at: Some(at(&manifest["producer"]["at"])),
            direction: None,
            queue: false,
        });
        assert_eq!(births.len(), 1, "actual construction site publication");
        let maker = births[0];
        assert_eq!(
            scene.m.rules.kinds[scene.m.world.game.units[&maker].kind].name,
            "motorpool"
        );
        let mut count = 0;
        let cap = manifest["producer"]["limitTicks"].as_u64().unwrap();
        while !scene.m.world.game.units[&maker]
            .built
            .is_some_and(|v| v >= 1.0)
            && count < cap
        {
            scene.ticks("construction", 1);
            count += 1;
        }
        assert!(
            scene.m.world.game.units[&maker]
                .built
                .is_some_and(|v| v >= 1.0),
            "actual construction completion before cap"
        );
        assert_eq!(json!(count), oracle["constructionTicks"]);
        scene.issue(Command::Stop {
            ids: vec![i64::from(builder)],
        });
        let actual = roster(&scene.m);
        assert_eq!(
            actual
                .as_array()
                .unwrap()
                .iter()
                .find(|u| u["id"] == maker)
                .unwrap(),
            &oracle["producer"]
        );
        assert!(
            scene.m.production.building(maker).is_some(),
            "real completed production binding"
        );
        assert!(
            scene.m.building_rallies.contains_key(&maker),
            "canonical producer rally binding"
        );
        scene.retained_structures = structures(&scene.m);
        if let Some(wear) = spec["wear"].as_f64() {
            // Authored measurement-band wear, matching Source. Never write a footprint cell.
            let rows = if spec["surface"] == "mud" {
                manifest["map"]["mudRows"].as_u64().unwrap() as usize
            } else {
                20
            };
            let band = manifest["map"]["w"].as_u64().unwrap() as usize * rows;
            assert!(scene.m.building_cells.keys().all(|c| *c >= band));
            scene.m.world.game.wear.as_mut().unwrap()[..band].fill(wear as f32);
        }
        scene.dry();
        if spec["name"] == "wet-ground" {
            scene.m.start.wind = (0.0, 1.0);
            scene.m.world.game.smokes.push(Smoke {
                id: 1,
                remaining: Some(60.0),
                x: 10.0,
                z: 10.0,
                r: 5.0,
            });
            scene.ticks("smoke", 100);
            let smoke = &scene.m.world.game.smokes[0];
            assert!(
                smoke.x > 14.0 && (smoke.z - 10.0).abs() < 2.0,
                "smoke drifts downwind"
            );
        }
        for actor in spec["actors"].as_array().unwrap() {
            let kind = actor["kind"].as_str().unwrap();
            let producer = if matches!(kind, "tank" | "at") {
                maker
            } else {
                scene
                    .m
                    .world
                    .game
                    .units
                    .values()
                    .find(|u| {
                        u.owner == 0
                            && scene.m.rules.kinds[u.kind].name
                                == manifest["producer"]["rifleMaker"].as_str().unwrap()
                    })
                    .expect("retained constructor HQ")
                    .id
            };
            scene.m.start.economy.players[0].mp = n(&manifest["producer"]["wallet"]);
            let births = scene.issue(Command::Buy {
                kind: kind.into(),
                from: Some(producer),
            });
            assert_eq!(births.len(), 1, "public Buy publishes exactly one actor");
            let id = births[0];
            assert_eq!(scene.m.recruit_metadata[&id].building, producer);
            assert!(
                scene.m.combat.get(id).is_some()
                    && scene.m.firing.get(id).is_some()
                    && scene.m.abilities.get(id).is_some()
                    && scene.m.recovery.get(id).is_some(),
                "canonical birth sidecars"
            );
            let u = scene.m.world.game.units.get_mut(&id).unwrap();
            assert_eq!(scene.m.rules.kinds[u.kind].name, kind);
            assert_eq!(u.owner, 0);
            let p = at(&actor["at"]);
            u.x = p.x;
            u.z = p.z;
            u.rot = n(&actor["rot"]);
            let control = scene.m.commands.units.get_mut(&id).unwrap();
            control.aim = 0.0;
            control.auto_retreat = false;
            scene
                .aliases
                .insert(actor["alias"].as_str().unwrap().into(), id);
        }
        refresh(&mut scene.m);
        scene.move_batch(spec);
        for actor in spec["actors"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["straight"] == true)
        {
            let id = scene.aliases[actor["alias"].as_str().unwrap()];
            // The original half-road check explicitly replaces this route array.
            let u = scene.m.world.game.units.get_mut(&id).unwrap();
            u.path = vec![at(&actor["goal"])];
            u.path_gen = u
                .path_gen
                .checked_add(1)
                .expect("authored new route identity");
        }
        scene
    }
    fn dry(&mut self) {
        let live = self
            .m
            .start
            .living
            .as_mut()
            .expect("real living-weather constructor state");
        live.raining = false;
        live.next = 1e9;
        self.m.world.game.rain = 0.0;
        self.m.world.game.wet = Some(0.0);
    }
    fn preserve(&self) {
        let current = roster(&self.m);
        for before in self.retained.as_array().unwrap() {
            assert_eq!(
                current
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|u| u["id"] == before["id"]),
                Some(before),
                "retained constructor identity/footprint"
            );
        }
        let current = structures(&self.m);
        for before in self.retained_structures.as_array().unwrap() {
            assert!(
                current.as_array().unwrap().contains(before),
                "retained real structure and section identities"
            );
        }
    }
    fn issue(&mut self, command: Command) -> Vec<u32> {
        let before: Vec<_> = self.m.world.game.units.keys().copied().collect();
        let mp = self.m.start.economy.players[0].mp;
        let oracle = &self.oracle["commands"][self.command];
        let parsed = Command::from_json(&oracle["command"]).expect("actual Source command parses");
        assert_eq!(
            parsed, command,
            "actual command input, never an oracle callback"
        );
        let result = self.m.execute_command(0, &command, false);
        assert_eq!(
            result,
            Ok(None),
            "{} public command {command:?}: {result:?}",
            self.oracle["name"]
        );
        assert!(oracle["result"].is_null());
        let births: Vec<_> = self
            .m
            .world
            .game
            .units
            .keys()
            .copied()
            .filter(|id| !before.contains(id))
            .collect();
        assert_eq!(json!(births), oracle["births"]);
        assert_eq!(
            bits(mp - self.m.start.economy.players[0].mp),
            oracle["debit"],
            "real wallet debit"
        );
        assert_eq!(
            bits(self.m.story[0].mp_spent),
            oracle["spent"],
            "real spending attribution"
        );
        self.command += 1;
        self.preserve();
        births
    }
    fn move_batch(&mut self, spec: &Value) {
        let orders = spec["actors"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| !a["goal"].is_null())
            .map(|a| MoveOrder {
                unit: i64::from(self.aliases[a["alias"].as_str().unwrap()]),
                at: at(&a["goal"]),
            })
            .collect();
        self.issue(Command::Move {
            orders,
            control_ids: None,
            attack_move: false,
            queue: false,
            together: spec["together"] == true,
            face: None,
        });
    }
    fn ticks(&mut self, phase: &str, count: u64) {
        for _ in 0..count {
            let report = self.m.advance_tick().unwrap_or_else(|failure| {
                panic!(
                    "{}:{phase}:{} first real runner failure: {failure:?}",
                    self.oracle["name"], self.m.world.game.tick
                )
            });
            assert_eq!(report.tick, self.m.world.game.tick);
            self.preserve();
            let expected = &self.oracle["steps"][self.step];
            assert_eq!(expected["phase"], phase);
            let mut actual = frame(&self.m, &self.aliases);
            actual["phase"] = json!(phase);
            assert_eq!(
                actual, *expected,
                "{}:{phase}:{} complete selected motion fields",
                self.oracle["name"], self.m.world.game.tick
            );
            self.step += 1;
        }
    }
    fn flags(&self) -> u64 {
        let row = SnapshotCache::new(&self.m)
            .expect("real snapshot admission")
            .recipient(0)
            .expect("actual owner delivery");
        let id = self.aliases["subject"];
        row.fields["units"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r[0] == id)
            .expect("real purchased actor delivery")[12]
            .as_u64()
            .unwrap()
    }
    fn finish(self) {
        assert_eq!(self.step, self.oracle["steps"].as_array().unwrap().len());
        assert_eq!(
            self.command,
            self.oracle["commands"].as_array().unwrap().len()
        );
        assert_eq!(roster(&self.m), self.oracle["finalRoster"]);
    }
}

fn run(f: &Value, name: &str) -> BTreeMap<String, (f64, f64)> {
    let manifest = &f["manifest"];
    let spec = manifest["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == name)
        .unwrap();
    let oracle = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == name)
        .unwrap();
    let mut scene = Scene::new(manifest, spec, oracle);
    scene.ticks("warmup", spec["warmup"].as_u64().unwrap());
    let starts: BTreeMap<_, _> = scene
        .aliases
        .iter()
        .map(|(name, id)| (name.clone(), scene.m.world.game.units[id].x))
        .collect();
    scene.ticks("measure", spec["measure"].as_u64().unwrap());
    let mut measured: BTreeMap<_, _> = scene
        .aliases
        .iter()
        .map(|(name, id)| {
            let u = &scene.m.world.game.units[id];
            assert_eq!(
                bits(u.x - starts[name]),
                bits(n(&oracle["measurements"][name]["distance"]))
            );
            assert_eq!(bits(u.x), bits(n(&oracle["measurements"][name]["x"])));
            (name.clone(), (u.x - starts[name], u.x))
        })
        .collect();
    if let Some(count) = spec["rainTicks"].as_u64() {
        assert_ne!(
            scene.flags() & 32768,
            0,
            "a tank moving on dry ground trails dust"
        );
        scene.m.start.living.as_mut().unwrap().next = 0.0;
        scene.ticks("rain", count);
        assert_eq!(scene.m.world.game.rain, 1.0, "the rain sets in");
        let wet = scene.m.world.game.wet.unwrap();
        assert!(wet > 0.15 && wet < 0.5, "and the ground soaks slowly");
        let delivered = SnapshotCache::new(&scene.m)
            .expect("real weather snapshot")
            .recipient(0)
            .unwrap();
        let delivered_rain = delivered.fields["wx"][0]
            .as_f64()
            .filter(|rain| rain.is_finite())
            .expect("delivered wx[0] must be a present finite Number");
        assert_eq!(
            delivered_rain.to_bits(),
            1.0_f64.to_bits(),
            "clients are told the weather"
        );
        scene.m.world.game.wet = Some(1.0);
        scene.m.start.living.as_mut().unwrap().next = 1e9;
        let id = scene.aliases["subject"];
        let u = scene.m.world.game.units.get_mut(&id).unwrap();
        u.x = 3.0;
        u.z = 25.0;
        u.rot = 0.0;
        u.move_speed = 0.0;
        u.path = vec![];
        u.path_gen = u
            .path_gen
            .checked_add(1)
            .expect("authored reset route identity");
        refresh(&mut scene.m);
        scene.issue(Command::Move {
            orders: vec![MoveOrder {
                unit: i64::from(id),
                at: Point { x: 37.0, z: 25.0 },
            }],
            control_ids: None,
            attack_move: false,
            queue: false,
            together: false,
            face: None,
        });
        scene.ticks("wet-warmup", 40);
        let start = scene.m.world.game.units[&id].x;
        scene.ticks("wet-measure", 20);
        let u = &scene.m.world.game.units[&id];
        measured.insert("wet".into(), (u.x - start, u.x));
        assert_eq!(
            bits(u.x - start),
            bits(n(&oracle["measurements"]["wet"]["distance"]))
        );
        assert_eq!(scene.flags() & 32768, 0, "no dust in the wet");
    }
    scene.finish();
    measured
}
fn rule(path: &[&str]) -> f64 {
    let catalog = Catalog::compiled().unwrap();
    let mut value = catalog.table("CFG").unwrap();
    for key in path {
        value = value.get(key).unwrap();
    }
    value.number().unwrap()
}

#[test]
fn original_roads_and_mud_distances_include_partial_road() {
    let f = fixture();
    let travel = |name| run(&f, name)["subject"].0;
    let grass = travel("grass");
    let road = travel("road");
    let edge = travel("road-edge");
    assert!(
        (road / grass - rule(&["roadSpeed"])).abs() < 0.1,
        "a tank on a road drives roadSpeed times faster"
    );
    assert!(
        edge / grass > 1.03 && edge / grass < rule(&["roadSpeed"]) - 0.1,
        "a tank half on the road gets part of the bonus"
    );
    let mud = Catalog::compiled()
        .unwrap()
        .table("CFG")
        .unwrap()
        .get("mudSpeed")
        .unwrap()
        .array()
        .unwrap();
    assert!(
        (travel("shallow-mud") / grass - mud[0].number().unwrap()).abs() < 0.1,
        "a tank in shallow mud"
    );
    assert!(
        (travel("deep-mud") / grass - mud[1].number().unwrap()).abs() < 0.1,
        "a tank in deep mud"
    );
    assert!(
        (travel("rifle-mud") - travel("rifle-grass")).abs() < 0.2,
        "infantry walk through mud at full speed"
    );
}
#[test]
fn original_half_churn_and_wet_ground_distances() {
    let f = fixture();
    let reference = run(&f, "churn-reference")["subject"].0;
    let worn = run(&f, "half-churn")["subject"].0;
    assert!(
        (worn / reference - (1.0 - rule(&["churn"]) * 0.5)).abs() < 0.06,
        "half-churned ground costs half the churn at steady speed"
    );
    let wet = run(&f, "wet-ground");
    assert!(
        (wet["wet"].0 / wet["subject"].0 - (1.0 - rule(&["weather", "wetGround"]))).abs() < 0.06,
        "wet ground slows a tank at steady speed"
    );
}
#[test]
fn original_uphill_and_downhill_distances() {
    let f = fixture();
    let hill = run(&f, "slope-hill");
    let flat = run(&f, "slope-flat");
    assert!(
        hill["up"].1 < flat["up"].1 - 0.5,
        "a tank climbs slower than it drives on the flat"
    );
    assert!(
        (hill["down"].1 - flat["down"].1).abs() < 0.3,
        "and loses nothing going down"
    );
}

#[test]
fn original_together_twenty_ticks_and_plain_order_reset() {
    let f = fixture();
    let spec = f["manifest"]["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "together")
        .unwrap();
    let oracle = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "together")
        .unwrap();
    let mut scene = Scene::new(&f["manifest"], spec, oracle);
    let tank = scene.aliases["tank"];
    let gun = scene.aliases["gun"];
    let slowest = scene.m.rules.kinds[scene.m.world.game.units[&gun].kind].speed;
    assert_eq!(
        scene.m.world.game.units[&tank].pace, slowest,
        "the group marches at its slowest unit"
    );
    assert_eq!(scene.m.world.game.units[&gun].pace, slowest);
    assert_eq!(bits(slowest), oracle["march"]["assigned"]);
    assert_eq!(bits(slowest), oracle["march"]["gun"]);
    scene.ticks("measure", 20);
    let tank_x = scene.m.world.game.units[&tank].x;
    let gun_x = scene.m.world.game.units[&gun].x;
    assert!(
        ((tank_x - 10.0) - (gun_x - 10.0)).abs() < 2.0,
        "tank and gun stay level on the march"
    );
    for (name, x) in [("tank", tank_x), ("gun", gun_x)] {
        assert_eq!(bits(x), bits(n(&oracle["measurements"][name]["x"])));
        assert_eq!(
            bits(x - 10.0),
            bits(n(&oracle["measurements"][name]["distance"]))
        );
    }
    scene.issue(Command::Move {
        orders: vec![MoveOrder {
            unit: i64::from(tank),
            at: Point { x: 90.0, z: 20.0 },
        }],
        control_ids: None,
        attack_move: false,
        queue: false,
        together: false,
        face: None,
    });
    assert_eq!(
        scene.m.world.game.units[&tank].pace, 0.0,
        "a plain order drops the march pace"
    );
    assert_eq!(
        bits(scene.m.world.game.units[&tank].pace),
        oracle["march"]["plain"]
    );
    scene.finish();
}
