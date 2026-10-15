//! Pending ordinary-command witnesses for the complete MOV-41/46/47 checks.
//! Canonical constructors and purchases retain every actor and production owner.
use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, MoveOrder, MoveOutcome, Stance},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::{TickPathConfig, TickPhase},
    math,
    orders::OrderKey,
    recipient_snapshot::SnapshotCache,
    terrain,
    weather::WeatherChoice,
};

fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}

struct Scene {
    m: Match,
    sequence: u64,
    inputs: Value,
}
impl Scene {
    fn new(inputs: &Value, spec: &Value, retries: u32) -> Self {
        let input = &spec["map"];
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(input).unwrap())
            .expect("real authored map admission");
        let mut m = Match::new(
            &map,
            MatchOptions {
                seed: inputs["seed"].as_u64().unwrap(),
                mode: Mode::Conquest,
                players: vec![
                    PlayerConfig { name: "A".into(), team: 0, faction: Faction::Usa },
                    PlayerConfig { name: "B".into(), team: 0, faction: Faction::Germany },
                ],
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
        ).expect("canonical ordinary constructor");
        let defaults = TickPathConfig::default();
        assert_eq!(defaults.retry_limit, integer(inputs, "defaultRetries") as u32);
        assert_eq!(defaults.budget, integer(inputs, "pathBudget"));
        let paths = if retries == defaults.retry_limit { defaults }
            else { TickPathConfig { retry_limit: retries, ..defaults } };
        m.initialize_tick_scene(0, paths).unwrap();
        // Declared funding precedes all purchases. HP, poses, RNG and live routes
        // are never imported or reset to reach a witness.
        m.start.economy.players[0].mp = inputs["wallet"].as_f64().unwrap();
        let mut s = Self { m, sequence: 0, inputs: inputs.clone() };
        // Both constructors retain their forces. Use ordinary setup controls
        // for the allied observer too so its idle routes cannot add retry work.
        for owner in 0..2 {
            let ids = s.m.world.game.units.values().filter(|u| u.owner == owner)
                .map(|u| i64::from(u.id)).collect();
            s.issue_for(owner as u32, Command::Stance {
                ids, stance: Stance::HoldPosition, on: true,
            });
        }
        s.step();
        s
    }
    fn issue(&mut self, command: Command) {
        self.issue_for(0, command);
    }
    fn issue_for(&mut self, issuer: u32, command: Command) {
        self.sequence += 1;
        self.m.enqueue_command(OrderKey {
            tick: self.m.world.game.tick as u64 + 1,
            issuer,
            sequence: self.sequence,
        }, command).expect("ordinary recorded input");
    }
    fn step(&mut self) {
        let report = self.m.advance_tick().expect("actual admitted tick");
        assert!(report.commands.iter().all(|c| c.denial.is_none()), "every ordinary input admits");
        assert!(self.m.tick_state.fault().is_none());
        assert!(report.phases.contains(&TickPhase::Separation));
        assert!(report.phases.contains(&TickPhase::Reactions));
    }
    fn rifles(&mut self, count: usize) -> Vec<u32> {
        let mut ids: Vec<_> = self.m.world.game.units.values()
            .filter(|u| u.owner == 0 && self.m.rules.kinds[u.kind].name == "rifle")
            .map(|u| u.id).collect();
        let hq = self.m.world.game.units.values()
            .find(|u| u.owner == 0 && self.m.rules.kinds[u.kind].name == "hq").unwrap().id;
        while ids.len() < count {
            let before = self.m.next_unit_id;
            self.issue(Command::Buy { kind: "rifle".into(), from: Some(hq) });
            self.step();
            let born: Vec<_> = self.m.world.game.units.values()
                .filter(|u| u.id >= before && u.owner == 0 && self.m.rules.kinds[u.kind].name == "rifle")
                .map(|u| u.id).collect();
            assert_eq!(born.len(), 1, "actual HQ purchase, no synthetic birth");
            let id = born[0];
            assert_eq!(self.m.recruit_metadata[&id].building, hq);
            assert!(self.m.combat.get(id).is_some() && self.m.firing.get(id).is_some()
                && self.m.abilities.get(id).is_some() && self.m.recovery.get(id).is_some());
            self.issue(Command::Stance { ids: vec![i64::from(id)], stance: Stance::HoldPosition, on: true });
            self.step();
            ids.push(id);
        }
        ids.truncate(count);
        ids
    }
    fn move_to(&mut self, goals: &[(u32, Point)], attack_move: bool, queue: bool) {
        self.issue(Command::Move {
            orders: goals.iter().map(|(id, at)| MoveOrder { unit: i64::from(*id), at: *at }).collect(),
            control_ids: None, attack_move, queue, together: false, face: None,
        });
    }
    fn stage(&mut self, goals: &[(u32, Point)]) {
        self.move_to(goals, false, false);
        for _ in 0..integer(&self.inputs, "stageTicks") {
            self.step();
            if goals.iter().all(|(id, at)| {
                let u = &self.m.world.game.units[id];
                distance(u.at(), *at) <= self.inputs["stageTolerance"].as_f64().unwrap() && u.path.is_empty() && u.world_goal.is_none()
            }) {
                return;
            }
        }
        panic!("ordinary commands did not reach the authored starting arrangement");
    }
    fn release(&mut self, ids: &[u32]) {
        self.issue(Command::Stance {
            ids: ids.iter().map(|id| i64::from(*id)).collect(),
            stance: Stance::HoldPosition, on: false,
        });
        self.step();
    }
}

fn integer(value: &Value, key: &str) -> usize {
    value[key].as_u64().unwrap() as usize
}
fn point(value: &Value) -> Point {
    Point { x: value[0].as_f64().unwrap(), z: value[1].as_f64().unwrap() }
}
fn spec<'a>(inputs: &'a Value, name: &str) -> &'a Value {
    inputs["scenes"].as_array().unwrap().iter().find(|s| s["name"] == name).unwrap()
}
fn starts(spec: &Value, ids: &[u32]) -> Vec<(u32, Point)> {
    let points = spec["starts"].as_array().unwrap();
    assert_eq!(points.len(), ids.len());
    ids.iter().zip(points).map(|(id, p)| (*id, point(p))).collect()
}
fn companion<'a>(fixture: &'a Value, name: &str) -> &'a Value {
    fixture["companions"].as_array().unwrap().iter().find(|c| c["name"] == name).unwrap()
}
fn fixture() -> Value {
    let inputs: Value = serde_json::from_str(include_str!(
        "../../../../harness/mov-ordinary-inputs.json"
    )).unwrap();
    let file = std::fs::File::open(concat!(env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/mov-ordinary-predicates.json.gz"))
        .expect("new Root-granted Source capture and committed fixture freeze required first");
    let mut text = String::new();
    flate2::read::GzDecoder::new(file).read_to_string(&mut text).unwrap();
    let fixture: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(fixture["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    assert_eq!(fixture["inputs"], inputs, "authored inputs, never recorded expected state");
    let originals = fixture["originals"].as_array().unwrap();
    assert_eq!(originals.len(), 5);
    for (original, spec) in originals.iter().zip(inputs["originalChecks"].as_array().unwrap()) {
        for key in ["file", "name", "sha256"] { assert_eq!(original[key], spec[key]); }
        assert_eq!(original["assertionsCompleted"], true);
        assert_eq!(original["body"], spec["body"], "full pinned original assertion block");
        if spec["file"] == "test.js" {
            assert_eq!(original["helpers"], inputs["originalHelpers"]);
        }
    }
    let companions = fixture["companions"].as_array().unwrap();
    assert_eq!(companions.iter().map(|c| c["name"].as_str().unwrap()).collect::<Vec<_>>(),
        ["retry-3", "retry-20", "spreading", "capture", "river", "cliff", "idle"]);
    for c in companions {
        assert_eq!(c["assertionsCompleted"], true);
        let name = c["name"].as_str().unwrap();
        assert_eq!(c["inputs"], *spec(&inputs, if name.starts_with("retry-") { "retry" } else { name }));
        assert!(!c["commands"].as_array().unwrap().is_empty());
        assert!(!c["events"].as_array().unwrap().is_empty());
    }
    fixture
}

#[test]
fn retry_cap_fresh_command_and_waiting_order_use_real_unreachable() {
    let fixture = fixture();
    let inputs = &fixture["inputs"];
    let spec = spec(inputs, "retry");
    for cap in spec["caps"].as_array().unwrap() {
        let cap = cap.as_u64().unwrap() as u32;
        let source_case = companion(&fixture, &format!("retry-{cap}"));
        let source = &source_case["result"];
        let source_id = &source_case["ids"][0];
        let source_start = source["queuedStart"].as_i64().unwrap();
        assert!(source["feedback"].as_array().unwrap().iter().any(|packet|
            packet["movement"].as_array().unwrap().iter().any(|row|
                row[0] == *source_id &&
                (row[1] == "unreachable" && row[2].as_i64().is_some_and(|t| t > source_start)
                    || row[3] == "unreachable" && row[4].as_i64().is_some_and(|t| t > source_start)))));
        assert_eq!(source["attempts"].as_u64(), Some(u64::from(cap)));
        assert_eq!(source["searches"], 1);
        assert_eq!(source["dropped"], 3);
        assert_eq!(source["freshAttempts"].as_u64(), Some(u64::from(cap)));
        let trace = source["attemptTrace"].as_array().unwrap();
        assert_eq!(trace.len(), cap as usize);
        for (i, row) in trace.iter().enumerate() {
            assert_eq!(row["attempt"].as_u64(), Some(i as u64 + 1));
            assert_eq!(row["dropped"].as_u64(), Some(u64::from(i + 1 == cap as usize)));
        }
        assert_eq!(source["freshFeedback"], true);
        assert_eq!(source["recovered"], true);
        let mut s = Scene::new(inputs, spec, cap);
        let id = s.rifles(1)[0];
        s.stage(&starts(spec, &[id]));
        let target = point(&spec["target"]);
        let at = point(&spec["starts"][0]);
        let dropped = s.m.commands.path_requests().stats().dropped;
        s.move_to(&[(id, target)], true, false);
        let mut attempts = 0;
        for _ in 0..(cap as usize * integer(inputs, "dropTickBudgetPerRetry") + integer(inputs, "dropExtraTicks")) {
            s.step();
            let searched = s.m.commands.path_requests().used();
            assert!(searched <= 1, "one live actor supplies every failed retry");
            attempts += searched;
            let c = &s.m.commands.units[&id];
            let u = &s.m.world.game.units[&id];
            assert!(u.path.is_empty(), "real disconnected searches supply no route");
            assert!(distance(u.at(), at) <= inputs["stageTolerance"].as_f64().unwrap());
            if c.outcome == Some(MoveOutcome::Unreachable) {
                assert_eq!(attempts, cap as usize);
                assert_eq!(s.m.commands.path_requests().stats().dropped, dropped + 1);
                assert!(u.amove.is_none());
                break;
            }
            assert_eq!(s.m.commands.path_requests().stats().dropped, dropped);
        }
        assert_eq!(s.m.commands.units[&id].outcome, Some(MoveOutcome::Unreachable));
        for _ in 0..integer(inputs, "quietTicks") {
            s.step(); assert_eq!(s.m.commands.path_requests().used(), 0);
        }
        s.move_to(&[(id, target)], true, false);
        let mut searches = 0;
        for _ in 0..integer(inputs, "freshTicks") {
            s.step(); searches += s.m.commands.path_requests().used();
            if searches > 0 { break; }
        }
        assert_eq!(searches, 1);
        assert_eq!(s.m.world.game.units[&id].amove, Some(target));
        assert_eq!(s.m.commands.path_requests().stats().dropped, dropped + 1);
        let mut fresh_attempts = searches;
        for _ in 0..(cap as usize * integer(inputs, "dropTickBudgetPerRetry") + integer(inputs, "dropExtraTicks")) {
            if s.m.world.game.units[&id].amove.is_none() { break; }
            s.step();
            let searched = s.m.commands.path_requests().used();
            assert!(searched <= 1);
            fresh_attempts += searched;
            assert_eq!(s.m.commands.path_requests().stats().dropped,
                dropped + if fresh_attempts == cap as usize { 2 } else { 1 });
        }
        assert_eq!(fresh_attempts, cap as usize);
        assert!(s.m.world.game.units[&id].amove.is_none());
        assert_eq!(s.m.commands.path_requests().stats().dropped, dropped + 2);
        let queued_start = s.m.world.game.tick + 1;
        let queued_drops = s.m.commands.path_requests().stats().dropped;
        let queued = point(&spec["queued"]);
        s.move_to(&[(id, target)], false, false);
        s.move_to(&[(id, queued)], false, true);
        let mut unreachable = false;
        let mut recovered = false;
        let queue_ticks = if cap == 3 { integer(inputs, "queueExtraTicks") }
            else { cap as usize * integer(inputs, "dropTickBudgetPerRetry") + integer(inputs, "queueExtraTicks") };
        for _ in 0..queue_ticks / 2 {
            // The original selected recipient loop observes every two ticks.
            s.step(); s.step();
            let packet = SnapshotCache::new(&s.m).unwrap().recipient(0).unwrap();
            unreachable |= packet.fields["movement"].as_array().unwrap().iter().any(|row| {
                row[0].as_u64() == Some(u64::from(id))
                    && (row[1] == "unreachable" && row[2].as_i64().is_some_and(|t| t > queued_start)
                        || row[3] == "unreachable" && row[4].as_i64().is_some_and(|t| t > queued_start))
            });
            let u = &s.m.world.game.units[&id];
            recovered = distance(u.at(), queued) < 0.5 && u.path.is_empty() && u.world_goal.is_none()
                && s.m.commands.units[&id].orders.is_empty();
            if recovered { break; }
        }
        assert_eq!(s.m.commands.path_requests().stats().dropped, queued_drops + 1);
        assert!(unreachable, "new queued journey result must have a later tick");
        assert!(recovered, "waiting order starts and arrives after real unreachable");
    }
}

#[test]
fn spreading_capture_circle_and_both_near_side_fallbacks_use_ordinary_routes() {
    let fixture = fixture();
    let inputs = &fixture["inputs"];
    let catalog = Catalog::compiled().unwrap();
    let cfg = catalog.table("CFG").unwrap();
    let spacing = cfg.get("behavior").unwrap().get("spacing").unwrap().number().unwrap();
    let point_radius = cfg.get("pointRadius").unwrap().number().unwrap();
    for name in ["spreading", "capture", "river", "cliff"] {
        let spec = spec(inputs, name);
        let source = &companion(&fixture, name)["result"];
        assert_eq!(source["completePredicate"], true);
        let source_ends: Vec<_> = source["ends"].as_array().unwrap().iter().map(|p|
            Point { x: p["x"].as_f64().unwrap(), z: p["z"].as_f64().unwrap() }).collect();
        let mut s = Scene::new(inputs, spec, integer(inputs, "defaultRetries") as u32);
        let ids = s.rifles(spec["starts"].as_array().unwrap().len());
        s.stage(&starts(spec, &ids));
        s.release(&ids);
        let click = if name == "capture" { s.m.start.points[0].at } else { point(&spec["click"]) };
        assert_eq!(source["click"]["x"].as_f64().unwrap().to_bits(), click.x.to_bits());
        assert_eq!(source["click"]["z"].as_f64().unwrap().to_bits(), click.z.to_bits());
        s.move_to(&ids.iter().map(|id| (*id, click)).collect::<Vec<_>>(), false, false);
        s.step();
        let ends: Vec<_> = ids.iter().map(|id|
            *s.m.world.game.units[id].path.last().expect("every actual route is nonempty")).collect();
        assert_eq!(ends.len(), source_ends.len());
        // Read every original predicate independently on both produced outputs.
        for produced in [&ends, &source_ends] {
            if name == "spreading" {
                for (i, p) in produced.iter().enumerate() {
                    assert!(distance(*p, click) < 12.0);
                    for q in &produced[i + 1..] { assert!(distance(*p, *q) >= spacing); }
                }
            } else if name == "capture" {
                assert!(produced.iter().all(|p| distance(*p, click) <= point_radius));
            } else {
                assert!(produced.iter().all(|p| p.z < 40.0), "{name}: all nine ends stay near-side");
                let initial = starts(spec, &ids);
                let center = Point { x: initial.iter().map(|(_, p)| p.x).sum::<f64>() / 9.0,
                    z: initial.iter().map(|(_, p)| p.z).sum::<f64>() / 9.0 };
                let dx = (click.x - center.x) / distance(click, center);
                assert!(click.z + dx * 5.0 > 40.0, "uncorrected formation crosses barrier");
            }
        }
    }
}

fn idle_pair(inputs: &Value) -> (Scene, [u32; 2]) {
    let spec = spec(inputs, "idle");
    let mut s = Scene::new(inputs, spec, integer(inputs, "defaultRetries") as u32);
    let ids = s.rifles(2);
    let ids = [ids[0], ids[1]];
    s.stage(&starts(spec, &ids));
    (s, ids)
}

#[test]
fn idle_spacing_four_seconds_requires_the_idle_route_provider() {
    let fixture = fixture();
    let inputs = &fixture["inputs"];
    let source_case = companion(&fixture, "idle");
    let source = &source_case["result"];
    assert_eq!(source["actualBefore"], source["controlBefore"], "sole timer intervention");
    assert!(source["actualBefore"].is_object());
    let source_ids = source_case["ids"].as_array().unwrap();
    assert_eq!(source_ids.len(), 2);
    let idle_start = source["idleStartTick"].as_i64().unwrap();
    let actual_source: Vec<_> = source_case["events"].as_array().unwrap().iter()
        .filter(|e| e["tick"].as_i64().unwrap() >= idle_start).collect();
    let control_source: Vec<_> = source["controlEvents"].as_array().unwrap().iter()
        .filter(|e| e["tick"].as_i64().unwrap() >= idle_start).collect();
    assert_eq!(actual_source.len(), integer(inputs, "idleTicks"));
    assert_eq!(control_source.len(), actual_source.len());
    let unit_at = |event: &Value, id: &Value| {
        let u = event["units"].as_array().unwrap().iter().find(|u| u["id"] == *id).unwrap();
        Point { x: u["x"].as_f64().unwrap(), z: u["z"].as_f64().unwrap() }
    };
    let source_final = actual_source.last().unwrap();
    let control_final = control_source.last().unwrap();
    assert!(distance(unit_at(source_final, &source_ids[0]), unit_at(source_final, &source_ids[1])) >= 2.75);
    assert!(distance(unit_at(control_final, &source_ids[0]), unit_at(control_final, &source_ids[1])) < 2.75);
    let source_start = &source["starts"][1];
    assert!(distance(unit_at(source_final, &source_ids[1]), Point {
        x: source_start["x"].as_f64().unwrap(), z: source_start["z"].as_f64().unwrap(),
    }) <= 4.0);
    assert!(source["coverRanks"][1].as_u64().unwrap() <= source["coverRanks"][0].as_u64().unwrap());
    let mut source_timer = inputs["controlReact"].as_f64().unwrap();
    let mut source_routed = false;
    for (event, control_event) in actual_source.iter().zip(control_source) {
        source_timer -= 0.05;
        let b = event["units"].as_array().unwrap().iter().find(|u| u["id"] == source_ids[1]).unwrap();
        source_routed |= b["drift"] == "space" && !b["path"].as_array().unwrap().is_empty();
        for id in source_ids {
            let c = control_event["units"].as_array().unwrap().iter().find(|u| u["id"] == *id).unwrap();
            assert_eq!(c["react"].as_f64().unwrap().to_bits(), source_timer.to_bits());
            assert!(c["path"].as_array().unwrap().is_empty());
        }
    }
    assert!(source_routed && source_timer > 0.0);
    assert_eq!(source["routed"], true);
    assert_eq!(source["ticks"], inputs["idleTicks"]);
    assert_eq!(source["coverAndRadius"], true);
    assert!(source["separation"].as_f64().unwrap() >= 2.75);
    assert!(source["controlSeparation"].as_f64().unwrap() < 2.75);
    let (mut actual, ids) = idle_pair(inputs);
    let (mut control, control_ids) = idle_pair(inputs);
    assert_eq!(ids, control_ids);
    // Independent constructors consume the same declared native streams. The
    // comparison intervention below changes only these actors' reaction timers.
    assert_eq!(format!("{:?}", actual.m.world.game), format!("{:?}", control.m.world.game));
    assert_eq!(actual.m.commands.units, control.m.commands.units);
    assert_eq!(actual.m.start.random, control.m.start.random);
    assert_eq!(actual.m.combat, control.m.combat);
    assert_eq!(actual.m.firing, control.m.firing);
    assert_eq!(actual.m.abilities, control.m.abilities);
    assert_eq!(actual.m.recovery, control.m.recovery);
    for id in ids { assert_eq!(actual.m.world.game.units[&id].at(), control.m.world.game.units[&id].at()); }
    let starts = ids.map(|id| actual.m.world.game.units[&id].at());
    assert!(distance(starts[0], starts[1]) < 0.75);
    let mut expected_timer = inputs["controlReact"].as_f64().unwrap();
    for id in ids { control.m.commands.units.get_mut(&id).unwrap().react = expected_timer; }
    let mut routed = false;
    for tick in 0..integer(inputs, "idleTicks") {
        if tick == 0 { actual.release(&ids); control.release(&ids); }
        else { actual.step(); control.step(); }
        expected_timer -= 0.05;
        for id in ids {
            assert_eq!(control.m.commands.units[&id].react.to_bits(), expected_timer.to_bits());
            assert!(control.m.world.game.units[&id].path.is_empty());
        }
        let u = &actual.m.world.game.units[&ids[1]];
        routed |= u.drift && !u.path.is_empty();
    }
    let a = &actual.m.world.game.units[&ids[0]];
    let b = &actual.m.world.game.units[&ids[1]];
    let ca = &control.m.world.game.units[&ids[0]];
    let cb = &control.m.world.game.units[&ids[1]];
    assert!(expected_timer > 0.0 && routed);
    assert!(distance(a.at(), b.at()) >= 2.75);
    assert!(distance(ca.at(), cb.at()) < 2.75, "live crowd separation alone stays below idle spacing");
    assert!(distance(b.at(), starts[1]) <= 4.0);
    assert!(terrain::cover_rank(&actual.m.world.game, &actual.m.rules, b.at(), None)
        <= terrain::cover_rank(&actual.m.world.game, &actual.m.rules, starts[1], None));
}
