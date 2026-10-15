//! Six selected Source predicates over the existing ordinary construction caller.
//! The fixture is generated later under Root's slot grant. Missing data is a failure.
use super::*;
use flate2::read::GzDecoder;
use std::{fs::File, io::Read, path::Path};
use ww2_core::{economy_state, math};

const PIN: &str = "e4e579a838aabae3b1d9dadbc17a0a87147bc52d";

fn fixture() -> Value {
    // Runtime loading keeps this source checkpoint compilable before recording.
    // Never skip or synthesize an expected result when the recording is absent.
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/base-construction-selected.json.gz");
    let file =
        File::open(path).expect("Root must grant Source recording before selected base replay");
    let mut text = String::new();
    GzDecoder::new(file).read_to_string(&mut text).unwrap();
    let fixture: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(fixture["reference"], PIN);
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 4);
    let predicates = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|case| case["predicates"].as_array().unwrap().iter())
        .map(|name| name.as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        predicates,
        BTreeSet::from([
            "base26_pillbox_slit_faces_out_from_home",
            "base28_b05_occupied_adjacent_complete_and_escape",
            "base29_conquest_construction_completes_and_buys_from_real_motorpool",
            "base30_conquest_builders_repair_after_real_tank_birth",
            "base48_mortar_needs_company_hq",
            "base48_pillbox_needs_company_hq",
        ])
    );
    for case in fixture["cases"].as_array().unwrap() {
        check_preparation(case);
    }
    fixture
}

fn check_preparation(case: &Value) {
    let proof = &case["preparation"];
    assert_eq!(
        proof["kind"],
        "root-approved-name-and-one-zero-value-neutral-point"
    );
    assert_eq!(proof["nativeAdmissionOfRaw"], false);
    assert_eq!(
        proof["originalValidation"],
        "name must be a string up to 40 chars"
    );
    assert_eq!(proof["nameOnlyValidation"], "needs 1-9 capture points");
    assert!(proof["amendedValidation"].is_null());
    let original = &proof["originalMap"];
    assert!(original.get("name").is_none());
    assert_eq!(original["points"], json!([]));
    let error = AuthoritativeMap::from_json(&serde_json::to_vec(original).unwrap())
        .err()
        .expect("raw component map must not gain native admission");
    assert_eq!(
        error.to_string(),
        "name: expected text up to 40 UTF-16 code units"
    );
    let mut named = original.clone();
    named["name"] = json!("Base selected");
    let error = AuthoritativeMap::from_json(&serde_json::to_vec(&named).unwrap())
        .err()
        .expect("name-only component map still has no capture points");
    assert_eq!(error.to_string(), "points: needs 1-9 capture points");
    let width = original["w"].as_u64().unwrap();
    assert!(width == 80 || width == 64);
    named["points"] = json!([{"x":if width == 80 {40} else {32},"y":4,"vp":0,"mp":0}]);
    assert_eq!(named, case["setup"]["map"]);
    assert_eq!(named, proof["amendedMap"]);
    AuthoritativeMap::from_json(&serde_json::to_vec(&named).unwrap()).unwrap();
    let control = &proof["control"];
    assert_eq!(control["setup"]["map"], *original);
    let mut control_setup = control["setup"].clone();
    control_setup["map"] = named;
    assert_eq!(control_setup, case["setup"]);
    for (key, value) in control.as_object().unwrap() {
        if key != "setup" {
            assert_eq!(
                *value, case[key],
                "complete original/amended recorded result: {key}"
            );
        }
    }
    assert_eq!(proof["comparedFrames"], case["snapshots"]);
    assert_eq!(proof["rawSteps"], proof["amendedSteps"]);
    assert_eq!(proof["rawFramesSha256"], proof["amendedFramesSha256"]);
    assert_eq!(proof["rawFramesSha256"].as_str().unwrap().len(), 64);
    for key in ["rawPointFrames", "amendedPointFrames"] {
        assert_eq!(
            proof[key].as_array().unwrap().len(),
            case["snapshots"].as_u64().unwrap() as usize
        );
    }
    assert!(
        proof["rawPointFrames"]
            .as_array()
            .unwrap()
            .iter()
            .all(|frame| *frame == json!([]))
    );
}

fn population(m: &Match) -> Vec<Value> {
    assert!(m.world.game.strikes.is_empty());
    let queues = m
        .world
        .game
        .units
        .keys()
        .map(|id| {
            m.production
                .building(*id)
                .map(|row| row.queue().collect::<Vec<_>>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    let units = m
        .world
        .game
        .units
        .values()
        .zip(&queues)
        .map(|(unit, queue)| economy_state::Unit {
            id: u64::from(unit.id),
            owner: Some(unit.owner),
            kind: &m.rules.kinds[unit.kind].name,
            hp: unit.hp,
            built: unit.built.unwrap_or(1.0),
            queue,
        })
        .collect::<Vec<_>>();
    let context = economy_state::Context {
        mode: economy_state::ModeContext::default(),
        army: m.options.army,
        units: &units,
        drops: &[],
        points: &[],
        nodes: &[],
        world: None,
    };
    (0..m.start.economy.players.len())
        .map(|slot| bits(m.start.economy.population(&context, slot).unwrap()))
        .collect()
}

fn extra_compare(m: &Match, expected: &Value, points: &Value) {
    assert_eq!(m.start.points.len(), 1);
    assert_eq!(points.as_array().unwrap().len(), 1);
    let actual = &m.start.points[0];
    let point = &points[0];
    assert_eq!(
        actual.owner, None,
        "approved neutral point must never become owned"
    );
    assert_eq!(
        actual.capper, None,
        "approved neutral point must never become captured"
    );
    assert_eq!(actual.progress.to_bits(), 0.0f64.to_bits());
    assert_eq!(actual.mp.to_bits(), 0.0f64.to_bits());
    assert_eq!(actual.vp.to_bits(), 0.0f64.to_bits());
    assert!(!actual.cut);
    assert_eq!(point["owner"], -1);
    assert_eq!(point["capper"], -1);
    assert_eq!(bits(actual.at.x), point["x"]);
    assert_eq!(bits(actual.at.z), point["z"]);
    assert_eq!(bits(actual.progress), point["progress"]);
    assert_eq!(bits(actual.mp), point["mp"]);
    assert_eq!(bits(actual.vp), point["vp"]);
    assert_eq!(json!(actual.cut), point["cut"]);
    assert_eq!(actual.id, point["scenarioId"]);
    assert_eq!(json!(actual.kind), point["kind"]);
    assert_eq!(json!(actual.needs.map_or(-1, |n| n as i64)), point["needs"]);
    assert_eq!(json!(population(m)), expected["population"]);
    for row in expected["units"].as_array().unwrap() {
        let id = row["id"].as_u64().unwrap() as u32;
        assert_eq!(bits(m.firing.get(id).unwrap().cooldown), row["cooldown"]);
    }
}

fn selected_prime(m: &mut Match, aliases: &BTreeMap<String, u32>, action: &Value) {
    // Delegate existing authored HP, pose, wallet and tier inputs to the parent.
    // Only quiet flags and the original B05 still/cooldown inputs are new here.
    let mut parent = action.clone();
    if let Some(rows) = parent["units"].as_object_mut() {
        for (alias, fields) in rows {
            let id = aliases[alias];
            let fields = fields.as_object_mut().unwrap();
            for key in ["holdFire", "holdPos", "auto", "autoRetreat"] {
                if let Some(value) = fields.remove(key) {
                    let value = value.as_bool().unwrap();
                    match key {
                        "holdFire" => m.commands.units.get_mut(&id).unwrap().hold_fire = value,
                        "holdPos" => m.world.game.units.get_mut(&id).unwrap().hold_pos = value,
                        "auto" => m.commands.units.get_mut(&id).unwrap().auto_cast = value,
                        "autoRetreat" => {
                            m.commands.units.get_mut(&id).unwrap().auto_retreat = value
                        }
                        _ => unreachable!(),
                    }
                }
            }
            if let Some(value) = fields.remove("still") {
                m.world.game.units.get_mut(&id).unwrap().still = Some(value.as_f64().unwrap());
            }
            if let Some(value) = fields.remove("cooldown") {
                m.firing.get_mut(id).unwrap().cooldown = value.as_f64().unwrap();
            }
            // The authentic purchase has no rally and no orders. Confirm rather
            // than erasing a route to make B05's original empty setup fit.
            if let Some(value) = fields.remove("path") {
                assert_eq!(value, json!([]));
                assert!(m.world.game.units[&id].path.is_empty());
            }
            if let Some(value) = fields.remove("orders") {
                assert_eq!(value, json!([]));
                assert!(m.commands.units[&id].orders.is_empty());
            }
        }
    }
    prime(m, aliases, &parent);
}

fn witness(m: &Match) -> Value {
    let actors = m
        .world
        .game
        .units
        .values()
        .map(|unit| {
            let production = m.production.building(unit.id);
            let structure = m
                .world
                .structures
                .iter()
                .find(|row| row.entity_id == Some(unit.id));
            json!({"id":unit.id,"type":m.rules.kinds[unit.kind].name,"owner":unit.owner,
                "hp":bits(unit.hp),"x":bits(unit.x),"z":bits(unit.z),"rot":bits(unit.rot),
                "built":unit.built.map(bits),"cells":unit.cells,
                "structureId":structure.map(|row|row.id.as_str()),
                "queue":production.map(|row|row.queue().collect::<Vec<_>>()),
                "producer":m.recruit_metadata.get(&unit.id).map(|row|row.building)})
        })
        .collect::<Vec<_>>();
    json!({"tick":m.world.game.tick,"actors":actors,"nextId":m.next_unit_id,
        "mp":bits(m.start.economy.players[0].mp),"population":population(m),
        "tier":m.production.player(0).unwrap().tech().tier(),
        "chars":String::from_utf8(m.world.game.chars.clone()).unwrap()})
}

struct Replay {
    aliases: BTreeMap<String, u32>,
    frames: BTreeMap<String, Value>,
    results: BTreeMap<String, Value>,
    steps: BTreeMap<String, usize>,
}
impl Replay {
    fn frame(&self, label: &str) -> &Value {
        &self.frames[label]
    }
    fn unit(&self, label: &str, alias: &str) -> &Value {
        let id = self.aliases[alias];
        self.frame(label)["actors"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == id)
            .unwrap()
    }
    fn mp(&self, label: &str) -> f64 {
        number(&self.frame(label)["mp"])
    }
}

fn replay(name: &str, predicate: &str) -> Replay {
    let fixture = fixture();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap();
    assert!(
        case["predicates"]
            .as_array()
            .unwrap()
            .contains(&json!(predicate))
    );
    assert_eq!(case["setup"]["spawn"], json!([]));
    assert_eq!(case["setup"]["setupRandomDraws"], 0);
    assert_eq!(case["setup"]["opts"]["supply"], false);
    assert_eq!(case["setup"]["opts"]["logistics"], false);
    let mut m = make(case);
    let constructor = m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
    let rng = m.start.random.snapshot();
    let mut aliases: BTreeMap<String, u32> =
        serde_json::from_value(case["setup"]["initialAliases"].clone()).unwrap();
    let mut expected = case["initial"].clone();
    let mut cursors = Cursors::default();
    let mut effects = json!([]);
    compare(&m, &expected, &rng, &mut cursors, &effects, name);
    extra_compare(&m, &expected, &case["preparation"]["amendedPointFrames"][0]);
    let mut frames = BTreeMap::from([("initial".into(), witness(&m))]);
    let mut results = BTreeMap::new();
    let mut steps = BTreeMap::new();
    let mut next = 0;
    for record in case["records"].as_array().unwrap() {
        let action = &record["action"];
        let op = action["op"].as_str().unwrap();
        let label = action["label"].as_str().unwrap();
        assert_eq!(record["randomDraws"], 0);
        assert_eq!(json!(m.start.environment_seed), record["seedBefore"]);
        let count = match op {
            "command" => {
                let before = m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
                let cmd = Command::from_json(&action["cmd"]).unwrap();
                let denial = m
                    .execute_command(action["slot"].as_u64().unwrap_or(0) as usize, &cmd, false)
                    .unwrap_or_else(|error| panic!("{name} {label}: {error}"));
                let result = json!(denial.map(|denial| denial.name()));
                assert_eq!(
                    result, record["result"],
                    "{name} {label}: Source command result"
                );
                assert_eq!(
                    result, action["expect"],
                    "{name} {label}: original predicate result"
                );
                if let Some(alias) = action["as"].as_str() {
                    let born = m
                        .world
                        .game
                        .units
                        .keys()
                        .copied()
                        .filter(|id| !before.contains(id))
                        .collect::<Vec<_>>();
                    assert_eq!(born.len(), 1);
                    let id = born[0];
                    aliases.insert(alias.into(), id);
                    if action["cmd"]["t"] == "buy" {
                        let producer = action["cmd"]["from"].as_u64().unwrap() as u32;
                        assert_eq!(m.recruit_metadata[&id].building, producer);
                        let maker = &m.world.game.units[&producer];
                        assert_eq!(maker.owner, 0);
                        assert_eq!(maker.built, Some(1.0));
                        assert!(
                            maker.hp > 0.0
                                && maker.cells.as_ref().is_some_and(|cells| !cells.is_empty())
                        );
                        assert!(
                            m.world
                                .structures
                                .iter()
                                .any(|row| row.entity_id == Some(producer))
                        );
                    } else {
                        assert_eq!(action["cmd"]["t"], "build");
                        let site = &m.world.game.units[&id];
                        let definition = Catalog::compiled()
                            .unwrap()
                            .unit(&m.rules.kinds[site.kind].name)
                            .unwrap();
                        let size = definition.get("size").unwrap().number().unwrap() as usize;
                        assert_eq!(site.built, Some(0.0));
                        assert_eq!(
                            site.hp,
                            definition.get("hpPer").unwrap().number().unwrap() * 0.25
                        );
                        let cells = site.cells.as_ref().unwrap();
                        assert_eq!(cells.len(), size * size);
                        let indexed_cells = cells
                            .iter()
                            .map(|cell| usize::try_from(*cell).expect("actor cell fits usize"))
                            .collect::<Vec<_>>();
                        assert!(indexed_cells.iter().all(|cell| m.building_cells[cell].unit == id));
                        assert!(
                            m.world
                                .structures
                                .iter()
                                .any(|row| row.entity_id == Some(id) && row.cells == indexed_cells)
                        );
                    }
                }
                assert!(results.insert(label.into(), result).is_none());
                1
            }
            "prime" => {
                selected_prime(&mut m, &aliases, action);
                1
            }
            "steps" | "until" => {
                let count = record["steps"].as_u64().unwrap() as usize;
                if op == "steps" {
                    assert_eq!(json!(count), action["count"]);
                } else {
                    assert_eq!(action["condition"], "finished");
                    assert!(count > 0 && count <= action["max"].as_u64().unwrap() as usize);
                }
                assert!(steps.insert(label.into(), count).is_none());
                count
            }
            other => panic!("unrepresented selected base action {other}"),
        };
        for index in 0..count {
            if op == "steps" || op == "until" {
                if op == "until" {
                    let id = aliases[action["target"].as_str().unwrap()];
                    assert!(m.world.game.units[&id].built.unwrap() < 1.0);
                }
                let report = m.advance_tick().unwrap_or_else(|error| {
                    panic!(
                        "{name} {label} step{index} tick{}: {error}",
                        m.world.game.tick
                    )
                });
                assert!(report.commands.is_empty());
                assert_eq!(
                    report
                        .phases
                        .iter()
                        .filter(|phase| **phase == TickPhase::Construction)
                        .count(),
                    1
                );
                assert!(!report.unavailable.contains(&TickPhase::Construction));
                let position = |phase| {
                    report
                        .phases
                        .iter()
                        .position(|value| *value == phase)
                        .unwrap()
                };
                assert!(position(TickPhase::Firing) < position(TickPhase::Construction));
                assert!(position(TickPhase::Construction) < position(TickPhase::Separation));
                effects
                    .as_array_mut()
                    .unwrap()
                    .extend(report.events.iter().map(|event| match event {
                        TickEvent::Environment(effect) => effect.to_json(),
                        other => panic!("{name}: unexpected event in quiet scene {other:?}"),
                    }));
            }
            patch(&mut expected, &case["patches"][next]);
            next += 1;
            assert_eq!(json!(aliases), expected["aliases"]);
            compare(
                &m,
                &expected,
                &rng,
                &mut cursors,
                &effects,
                &format!("{name} {label} snapshot{next}"),
            );
            extra_compare(
                &m,
                &expected,
                &case["preparation"]["amendedPointFrames"][next],
            );
            assert!(
                constructor
                    .iter()
                    .all(|id| m.world.game.units.contains_key(id))
            );
        }
        if op == "until" {
            let id = aliases[action["target"].as_str().unwrap()];
            assert_eq!(m.world.game.units[&id].built, Some(1.0));
        }
        assert_eq!(json!(m.start.environment_seed), record["seedAfter"]);
        assert_eq!(json!(next), record["lastSnapshot"]);
        assert_eq!(json!(m.world.game.tick), case["checkpoints"][label]);
        assert!(frames.insert(label.into(), witness(&m)).is_none());
    }
    assert_eq!(next, case["patches"].as_array().unwrap().len());
    assert_eq!(json!(next + 1), case["snapshots"]);
    assert_eq!(json!(aliases), case["finalAliases"]);
    Replay {
        aliases,
        frames,
        results,
        steps,
    }
}

#[test]
fn base26_pillbox_slit_faces_out_from_home() {
    let trace = replay(
        "pillbox-facing-and-tier",
        "base26_pillbox_slit_faces_out_from_home",
    );
    assert!(trace.results["placed"].is_null());
    let site = trace.unit("placed", "site");
    assert_eq!(site["type"], "pillbox");
    assert!(number(&site["built"]) < 1.0);
    assert!(
        number(&site["rot"]).abs() < 0.2,
        "the slit faces out from home, toward the enemy side"
    );
    assert_eq!(site["cells"].as_array().unwrap().len(), 4);
    assert!(site["structureId"].is_string());
    assert_eq!(trace.mp("placed"), trace.mp("tier-two") - 200.0);
}

#[test]
fn base28_b05_occupied_adjacent_complete_and_escape() {
    let trace = replay(
        "b05-real-producer-completion-and-escape",
        "base28_b05_occupied_adjacent_complete_and_escape",
    );
    let born = trace.unit("purchased", "worker");
    assert_eq!(born["type"], "rifle");
    assert_eq!(born["producer"], json!(trace.aliases["hq"]));
    let price = Catalog::compiled()
        .unwrap()
        .unit("rifle")
        .unwrap()
        .get("cost")
        .unwrap()
        .number()
        .unwrap();
    assert_eq!(trace.mp("purchased"), trace.mp("funded") - price);
    assert_eq!(trace.results["occupied"], "blocked");
    assert_eq!(
        trace.mp("occupied"),
        trace.mp("ready"),
        "refusal spends no construction MP"
    );
    let builder = trace.unit("occupied", "worker");
    let cell = (number(&builder["z"]) / 2.0).floor() as usize * 64
        + (number(&builder["x"]) / 2.0).floor() as usize;
    assert_eq!(
        trace.frame("occupied")["chars"]
            .as_str()
            .unwrap()
            .as_bytes()[cell],
        b'.'
    );
    assert!(trace.mp("occupied") >= trace.mp("posed"));
    assert!(trace.results["adjacent"].is_null());
    assert_eq!(trace.unit("complete", "site")["built"], bits(1.0));
    assert!(trace.results["escape-order"].is_null());
    let escaped = trace.unit("escaped", "worker");
    assert!(math::hypot(number(&escaped["x"]) - 81.0, number(&escaped["z"]) - 31.0) < 0.5);
    assert_eq!(
        trace.steps,
        BTreeMap::from([
            ("ready".into(), 1),
            ("complete".into(), 700),
            ("escaped".into(), 600)
        ])
    );
}

#[test]
fn base29_conquest_construction_completes_and_buys_from_real_motorpool() {
    let trace = replay(
        "conquest-motorpool-completion-and-repair",
        "base29_conquest_construction_completes_and_buys_from_real_motorpool",
    );
    assert!(
        trace.results["placed"].is_null(),
        "conquest rifle builds Motor Pool"
    );
    assert_eq!(trace.results["unfinished-buy"], "needs");
    assert_eq!(
        trace.unit("complete", "site")["built"],
        bits(1.0),
        "conquest construction completes"
    );
    assert!(trace.steps["complete"] <= 1600);
    assert!(trace.results["tank-born"].is_null());
    let tank = trace.unit("tank-born", "tank");
    assert_eq!(tank["type"], "tank");
    assert_eq!(tank["owner"], 0);
    assert_eq!(tank["producer"], json!(trace.aliases["site"]));
    assert_eq!(trace.mp("tank-born"), trace.mp("complete") - 300.0);
}

#[test]
fn base30_conquest_builders_repair_after_real_tank_birth() {
    let trace = replay(
        "conquest-motorpool-completion-and-repair",
        "base30_conquest_builders_repair_after_real_tank_birth",
    );
    assert_eq!(trace.unit("complete", "site")["built"], bits(1.0));
    assert_eq!(trace.unit("damaged", "site")["hp"], bits(500.0));
    assert!(trace.results["assist"].is_null());
    assert_eq!(trace.steps["repaired"], 100);
    assert!(
        number(&trace.unit("repaired", "site")["hp"]) > 500.0,
        "builders repair"
    );
    assert_eq!(
        trace.unit("repaired", "tank")["id"],
        trace.unit("tank-born", "tank")["id"]
    );
    assert_eq!(
        trace.unit("repaired", "tank")["producer"],
        json!(trace.aliases["site"])
    );
}

#[test]
fn base48_mortar_needs_company_hq() {
    let trace = replay(
        "mortar-and-motorpool-tier",
        "base48_mortar_needs_company_hq",
    );
    assert_eq!(trace.frame("funded")["tier"], 1);
    assert_eq!(
        trace.results["mortar-refusal"], "tier",
        "mortar needs Company HQ"
    );
    assert_eq!(trace.results["motorpool-refusal"], "tier");
    for label in ["mortar-refusal", "motorpool-refusal"] {
        assert_eq!(trace.mp(label), trace.mp("funded"));
        assert_eq!(
            trace.frame(label)["nextId"],
            trace.frame("funded")["nextId"]
        );
        assert_eq!(
            trace.frame(label)["population"],
            trace.frame("funded")["population"]
        );
        assert_eq!(
            trace.unit(label, "barracks")["queue"],
            trace.unit("funded", "barracks")["queue"]
        );
        let ids = |frame: &Value| {
            frame["actors"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["id"].clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(trace.frame(label)), ids(trace.frame("funded")));
    }
}

#[test]
fn base48_pillbox_needs_company_hq() {
    let trace = replay("pillbox-facing-and-tier", "base48_pillbox_needs_company_hq");
    assert_eq!(trace.frame("funded")["tier"], 1);
    assert_eq!(
        trace.results["tier-refusal"], "tier",
        "a pillbox needs Company HQ"
    );
    assert_eq!(trace.mp("tier-refusal"), trace.mp("funded"));
    assert_eq!(trace.frame("tier-two")["tier"], 2);
    assert!(trace.results["placed"].is_null());
    assert!(number(&trace.unit("placed", "site")["built"]) < 1.0);
}
