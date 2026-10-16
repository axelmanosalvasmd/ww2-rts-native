//! Genuine ground command witnesses. Registered only by the cfg(test) replay parent.
use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    match_commands::{Command, Denial},
    match_runtime::{MatchOptions, PlayerConfig},
    match_tick::{TickEvent, TickReport},
    weather::WeatherChoice,
};
use std::io::BufRead;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
};

#[path = "aligned_ground_positive.rs"]
mod positive_observers;

#[path = "aligned_retreat_predicate.rs"]
mod retreat_predicate;

#[path = "aligned_retreat_work.rs"]
mod work_cancellation;

#[path = "aligned_destroy_work.rs"]
mod destroy_work;

#[path = "aligned_ground_receipts.rs"]
mod receipt_binding;

fn number(value: f64) -> Value {
    json!({"bits":format!("{:016x}",value.to_bits()),"value":if value.is_finite(){json!(value)}else{json!(if value.is_nan(){"NaN"}else if value.is_sign_positive(){"Infinity"}else{"-Infinity"})}})
}
fn point(value: Point) -> Value {
    json!({"x":number(value.x),"z":number(value.z)})
}
fn goal(value: Option<Point>) -> Value {
    value.map_or(Value::Null, point)
}

fn exact_command_numbers(value: &Value) -> Value {
    match value {
        Value::Number(n) => number(n.as_f64().expect("finite authored command number")),
        Value::Array(rows) => json!(rows.iter().map(exact_command_numbers).collect::<Vec<_>>()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), exact_command_numbers(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

struct Witness {
    state: Match,
    replay: Replay,
    log: std::fs::File,
    source_complete: Value,
    source_input: Value,
    source_commands: Vec<Value>,
    commands: Vec<Value>,
    source_prefixes: Vec<Value>,
    prefixes: Vec<Value>,
    production_witness: Option<Value>,
}
impl Witness {
    fn new(name: &str) -> Self {
        let input: Value = serde_json::from_slice(
            &std::fs::read(
                std::env::var_os("WW2_GROUND_INPUT").expect("frozen shared input required"),
            )
            .unwrap(),
        )
        .unwrap();
        let scene: Value = serde_json::from_str(include_str!(
            "../../../../harness/scenarios/aligned-stop.json"
        ))
        .unwrap();
        let map =
            crate::map::AuthoritativeMap::from_json(&serde_json::to_vec(&scene["map"]).unwrap())
                .unwrap();
        // Source results are read only as evidence. No Source state enters Match.
        let source_directory = std::path::PathBuf::from(
            std::env::var_os("WW2_GROUND_SOURCE_DIR")
                .expect("successful authentic Source case directory"),
        );
        let source_path = source_directory.join(format!("{name}.jsonl"));
        let source_bytes = std::fs::read(&source_path).expect("authentic Source capture required");
        let process_binding = receipt_binding::checked_capture(&source_path, name, &source_bytes);
        let source_file = std::io::Cursor::new(source_bytes);
        let mut source_complete = None;
        let mut source_terminal = None;
        let mut source_input = None;
        let mut source_commands = Vec::new();
        let mut raw_command_controls = 0;
        let mut source_startup = None;
        let mut source_prefixes = Vec::new();
        for line in std::io::BufReader::new(source_file).lines() {
            let row: Value = serde_json::from_str(&line.unwrap()).unwrap();
            let evidence = &row["evidence"];
            let stage = evidence["stage"].as_str().unwrap();
            if [
                "failure",
                "predicateFailure",
                "missingMilestone",
                "commandException",
                "tickException",
            ]
            .contains(&stage)
            {
                panic!("Source failed; no native constructor or tick permitted: {evidence}");
            }
            assert!(
                row.get("stateProviderError").is_none(),
                "Source state provider failed: {row}"
            );
            let row_tick = row["state"]["tick"]
                .as_i64()
                .filter(|tick| *tick >= 0)
                .expect("authentic Source record must retain its actual clock");
            let row_draws = row["state"]["drawCount"]
                .as_u64()
                .expect("authentic Source record must retain its actual draw count");
            if ["startup", "command", "tick"].contains(&stage) {
                source_prefixes.push(json!({"stage":stage,"tick":row["state"]["tick"],"drawCount":row["state"]["drawCount"]}));
            }
            assert!(
                source_input.is_none(),
                "Source input receipt must be the last record"
            );
            assert!(
                stage == "startup" || source_startup.is_some(),
                "Source startup must be first"
            );
            assert!(
                source_complete.is_none() || stage == "inputReceipt",
                "no actions may follow Source completion"
            );
            match stage {
                "startup" => {
                    assert!(source_startup.replace(evidence.clone()).is_none());
                }
                "complete" => {
                    assert!(
                        evidence["checks"].is_object(),
                        "Source complete checks cannot be absent/null"
                    );
                    assert!(
                        source_complete
                            .replace(evidence["checks"].clone())
                            .is_none()
                    );
                    source_terminal = Some((row_tick, row_draws));
                }
                "inputReceipt" => {
                    assert_eq!(
                        source_terminal,
                        Some((row_tick, row_draws)),
                        "Source input receipt must share the actual completion boundary"
                    );
                    assert!(source_input.replace(evidence.clone()).is_none());
                }
                "command" => {
                    assert!(evidence["owner"].as_u64().is_some_and(|owner| owner < 2));
                    assert!(evidence["command"].is_object());
                    assert!(
                        evidence.as_object().unwrap().contains_key("result"),
                        "missing Source result cannot become an accepted null"
                    );
                    assert!(
                        evidence["result"].is_null()
                            || evidence["result"]
                                .as_str()
                                .is_some_and(|reason| !reason.is_empty()),
                        "Source disposition must be explicit null or a denial string"
                    );
                    raw_command_controls += receipt_binding::command_counterfactuals(
                        &evidence["command"],
                        &evidence["commandBits"],
                    );
                    source_commands.push(json!({"tick":row["state"]["tick"],"owner":evidence["owner"],"command":evidence["commandBits"],"denial":evidence["result"]}));
                }
                "failure" | "predicateFailure" | "missingMilestone" => {
                    panic!("Source failed; no native constructor or tick permitted: {evidence}")
                }
                "tick" | "milestone" | "productionWitness" | "retreatPredicate" => {}
                unknown => panic!("unknown Source evidence stage: {unknown}"),
            }
        }
        let source_startup = source_startup.expect("Source startup binding");
        assert_eq!(
            source_startup["reference"],
            "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
        );
        assert_eq!(source_startup["case"], name);
        assert_eq!(source_startup["sharedInput"], input);
        assert_eq!(source_startup["map"], scene["map"]);
        assert_eq!(
            source_startup["sourceHead"],
            std::env::var("WW2_GROUND_SOURCE_HEAD").expect("exact frozen Source-producing head")
        );
        assert_eq!(
            source_startup["profile"],
            json!({"mode":"conquest","army":"endless","tech":false,"supply":true,"logistics":false})
        );
        assert_eq!(source_startup["instrumentation"]["erasesExactly"], true);
        let source_complete =
            source_complete.expect("Source must satisfy the complete case before native starts");
        let source_input =
            source_input.expect("Source must retain the complete actual input receipt");
        assert_eq!(source_input["capacity"], 16384);
        assert_eq!(source_input["exhausted"], false);
        let draws = source_input["draws"]
            .as_array()
            .expect("complete Source draw array");
        assert!(draws.len() < 16384, "Source finite input tape exhausted");
        assert_eq!(source_input["consumed"], draws.len());
        assert_eq!(source_input["unconsumed"], 16384 - draws.len());
        for (index, row) in draws.iter().enumerate() {
            assert_eq!(row["index"], index);
            assert_eq!(row["bits"], input["tape"][index]);
            let consumer = row["consumer"]
                .as_array()
                .expect("actual structured draw consumer");
            assert!(
                !consumer.is_empty()
                    && consumer[0]
                        .as_str()
                        .is_some_and(|s| !s.starts_with("UNMAPPED"))
            );
        }
        let (terminal_tick, terminal_draws) =
            source_terminal.expect("actual Source completion boundary");
        assert_eq!(terminal_draws, draws.len() as u64);
        let input_controls =
            receipt_binding::input_counterfactuals(&source_input, &input, terminal_draws);
        let prefix_controls = receipt_binding::counterfactuals(
            &source_prefixes,
            &source_commands,
            draws.len(),
            terminal_tick,
        );
        let replay = Replay::new(&input).unwrap();
        let state = Match::new_aligned(
            &map,
            MatchOptions {
                seed: input["seed"].as_str().unwrap().parse().unwrap(),
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
                shuffle: true,
                army: ArmySize::Endless,
                weather: WeatherChoice::from_reference(Some("clear")),
                weather_seed: None,
                living_weather: true,
                map_key: Some("aligned-ground-commands".into()),
                tech: false,
                supply: true,
                logistics: false,
                sandbox: false,
            },
            replay.clone(),
        )
        .unwrap();
        let directory = std::path::PathBuf::from(
            std::env::var_os("WW2_GROUND_REPORT_DIR").expect("new private report directory"),
        );
        let log = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(format!("{name}.jsonl")))
            .unwrap();
        let mut w = Self {
            state,
            replay,
            log,
            source_complete,
            source_input,
            source_commands,
            commands: Vec::new(),
            source_prefixes,
            prefixes: Vec::new(),
            production_witness: None,
        };
        w.record(json!({"stage":"startup","head":std::env::var("WW2_GROUND_HEAD").unwrap(),"profile":{"mode":"conquest","army":"endless","tech":false,"supply":true,"logistics":false},"inputReceipt":w.replay.receipt(),"sourceBinding":source_startup}));
        w.record(json!({"stage":"sourceReceiptControls","checked":prefix_controls,"rawCommandControls":raw_command_controls,"inputReceiptControls":input_controls,"sourceProcessBinding":process_binding,"nativeConstructedOnlyAfterSourceValidation":true}));
        for owner in 0..2 {
            let ids = w
                .state
                .world
                .game
                .units
                .values()
                .filter(|u| u.owner == owner && !w.state.rules.kinds[u.kind].structure)
                .map(|u| u.id)
                .collect::<Vec<_>>();
            for (key, on) in [
                ("holdFire", true),
                ("holdPos", true),
                ("autoRetreat", false),
            ] {
                w.command(
                    owner,
                    json!({"t":"stance","ids":ids,"key":key,"on":on}),
                    None,
                );
            }
            w.command(owner, json!({"t":"autocast","ids":ids,"on":false}), None);
        }
        w
    }
    fn checked_snapshot(&self) -> Result<Value, String> {
        let m = &self.state;
        let units = m.world.game.units.values().map(|u| {
                let kind=m.rules.kinds.get(u.kind).ok_or_else(||format!("unknown kind for {}",u.id))?;
                let c = m.commands.units.get(&u.id).ok_or_else(||format!("missing command provider for {}",u.id))?;
                let f = m.firing.get(u.id).ok_or_else(||format!("missing firing provider for {}",u.id))?;
                let combat = m.combat.get(u.id).ok_or_else(||format!("missing combat provider for {}",u.id))?;
                Ok(json!({"id":u.id,"owner":u.owner,"kind":kind.name,
                    "x":number(u.x),"z":number(u.z),"hp":number(u.hp),"supp":number(u.supp),"speed":number(u.move_speed),
                    "path":u.path.iter().copied().map(point).collect::<Vec<_>>(),"goal":goal(u.world_goal),"amove":goal(u.amove),
                    "retreating":u.retreating,"riding":u.riding,"carrier":combat.carrier,"cargo":combat.cargo,
                    "attackId":c.attack_id,"targetId":f.target_id,"orders":format!("{:?}",c.orders),
                    "build":c.build_target,"dig":u.dig,"entrench":u.entrench,"garrison":u.garrison,
                    "lastHit":combat.last_hit,"built":u.built,
                    "rawControl":format!("{c:?}"),"rawFiring":format!("{f:?}"),"rawCombat":format!("{combat:?}") }))
            }).collect::<Result<Vec<_>,String>>()?;
        Ok(
            json!({"tick":m.world.game.tick,"nextId":m.next_unit_id,"units":units,
            "wallets":m.start.economy.players.iter().map(|p|number(p.mp)).collect::<Vec<_>>(),
            "wrecks":m.world.game.wrecks.iter().map(|w|json!({"id":w.id,"x":number(w.x),"z":number(w.z),"cell":w.cell,"moving":w.moving})).collect::<Vec<_>>(),
            "drawCount":self.replay.0.lock().unwrap().used.len()}),
        )
    }
    fn snapshot(&self) -> Value {
        self.checked_snapshot()
            .expect("complete actual snapshot providers")
    }
    fn record(&mut self, evidence: Value) {
        if ["startup", "command", "tick"]
            .iter()
            .any(|stage| evidence["stage"] == *stage)
        {
            self.prefixes.push(json!({"stage":evidence["stage"],"tick":self.state.world.game.tick,"drawCount":self.replay.0.lock().unwrap().used.len()}));
        }
        // Preserve an ordinary command/tick error even when a missing sidecar prevents projection.
        let row = match self.checked_snapshot() {
            Ok(state) => json!({"evidence":evidence,"state":state}),
            Err(reason) => json!({"evidence":evidence,"stateProviderError":reason}),
        };
        serde_json::to_writer(&mut self.log, &row).unwrap();
        self.log.write_all(b"\n").unwrap();
        self.log.flush().unwrap();
    }
    fn command(&mut self, owner: usize, raw: Value, expected: Option<Denial>) {
        let before = self.snapshot();
        let command = match Command::from_json(&raw) {
            Ok(command) => command,
            Err(denial) => {
                self.record(json!({"stage":"commandDecodeFailure","owner":owner,"command":raw,"before":before,"denial":denial.name()}));
                panic!("ordinary authored command decode failed: {denial:?}");
            }
        };
        let result = self
            .state
            .execute_command(owner, &command, self.state.finish.is_some());
        self.record(json!({"stage":"command","owner":owner,"command":raw,"before":before,"result":format!("{result:?}")}));
        let denial = match result {
            Ok(denial) => denial,
            Err(reason) => panic!("ordinary command failed; raw command/error retained: {reason}"),
        };
        self.commands.push(json!({"tick":self.state.world.game.tick,"owner":owner,"command":exact_command_numbers(&raw),"denial":denial.map(|d|d.name())}));
        assert_eq!(denial, expected);
    }
    fn unchanged_command(&mut self, owner: usize, raw: Value, denial: Denial) {
        let before = self.snapshot();
        self.command(owner, raw, Some(denial));
        assert_eq!(
            before,
            self.snapshot(),
            "denied command changed observed state"
        );
    }
    fn step(&mut self) -> TickReport {
        let report = self.state.advance_tick();
        match report {
            Ok(report) => {
                self.record(json!({"stage":"tick","events":format!("{:?}",report.events),"retired":report.retired.iter().map(|r|json!({"id":r.unit.id,"owner":r.unit.owner,"hp":number(r.unit.hp),"lastHit":r.last_hit,"enemyKiller":r.enemy_killer})).collect::<Vec<_>>() }));
                report
            }
            Err(error) => {
                self.record(json!({"stage":"tickFailure","reason":error.reason,"events":format!("{:?}",error.events)}));
                panic!("ordinary runner failed: {error}");
            }
        }
    }
    fn until(&mut self, limit: usize, label: &str, reached: impl Fn(&Match) -> bool) {
        for _ in 0..limit {
            if reached(&self.state) {
                self.record(json!({"stage":"milestone","label":label}));
                return;
            }
            self.step();
        }
        self.record(json!({"stage":"missingMilestone","label":label}));
        panic!("genuine scene did not reach {label} within {limit} ticks");
    }
    fn move_to(&mut self, owner: usize, id: u32, at: Point) {
        self.command(owner, json!({"t":"move","orders":[[id,at.x,at.z]]}), None);
        self.until(1000, "ordinary-move-arrival", |m| {
            let u = &m.world.game.units[&id];
            u.path.is_empty() && crate::math::hypot(u.x - at.x, u.z - at.z) < 1.0
        });
    }
    fn tank(&mut self, owner: usize) -> (u32, u32) {
        let (builders, x, z) = if owner == 0 {
            (vec![1, 2], 29., 9.)
        } else {
            (vec![4, 5], 93., 53.)
        };
        let catalog = crate::catalog::Catalog::compiled().unwrap();
        let build_cost = catalog
            .unit("motorpool")
            .unwrap()
            .get("cost")
            .unwrap()
            .number()
            .unwrap();
        let tank_cost = catalog
            .unit("tank")
            .unwrap()
            .get("cost")
            .unwrap()
            .number()
            .unwrap();
        let full = catalog
            .unit("motorpool")
            .unwrap()
            .get("hpPer")
            .unwrap()
            .number()
            .unwrap();
        let build_tick = self.state.world.game.tick;
        let build_mp = self.state.start.economy.players[owner].mp;
        let before_ids = self
            .state
            .world
            .game
            .units
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        let site = self.state.next_unit_id;
        self.command(
            owner,
            json!({"t":"build","ids":builders,"kind":"motorpool","x":x,"z":z}),
            None,
        );
        assert_eq!(self.state.world.game.tick, build_tick);
        assert_eq!(
            self.state.start.economy.players[owner].mp.to_bits(),
            (build_mp - build_cost).to_bits()
        );
        let after_ids = self
            .state
            .world
            .game
            .units
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            after_ids
                .difference(&before_ids)
                .copied()
                .collect::<Vec<_>>(),
            vec![site]
        );
        assert_eq!(self.state.next_unit_id, site + 1);
        let site_row = &self.state.world.game.units[&site];
        assert_eq!(self.state.rules.kinds[site_row.kind].name, "motorpool");
        assert_eq!(site_row.built, Some(0.0));
        assert_eq!(site_row.hp.to_bits(), (full * 0.25).to_bits());
        for builder in &builders {
            assert!(self.state.world.game.units[builder].build);
            assert_eq!(self.state.commands.units[builder].build_target, Some(site));
        }
        self.until(2400, "paid-motorpool-completed", |m| {
            m.world.game.units[&site].built == Some(1.0)
        });
        let completed_tick = self.state.world.game.tick;
        let tank = self.state.next_unit_id;
        let buy_mp = self.state.start.economy.players[owner].mp;
        let production = self.state.production.clone();
        let before_ids = self
            .state
            .world
            .game
            .units
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        self.command(owner, json!({"t":"buy","unit":"tank","from":site}), None);
        assert_eq!(self.state.world.game.tick, completed_tick);
        assert_eq!(
            self.state.start.economy.players[owner].mp.to_bits(),
            (buy_mp - tank_cost).to_bits()
        );
        let after_ids = self
            .state
            .world
            .game
            .units
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            after_ids
                .difference(&before_ids)
                .copied()
                .collect::<Vec<_>>(),
            vec![tank]
        );
        assert_eq!(self.state.next_unit_id, tank + 1);
        assert_eq!(
            self.state.rules.kinds[self.state.world.game.units[&tank].kind].name,
            "tank"
        );
        assert_eq!(
            self.state.production, production,
            "Conquest purchase must publish immediately without a queued production job"
        );
        self.production_witness = Some(
            json!({"owner":owner,"site":site,"tank":tank,"buildTick":build_tick,"completedTick":completed_tick,"purchaseTick":self.state.world.game.tick,
            "buildCharge":number(build_cost),"tankCharge":number(tank_cost),"initialSiteHP":number(full*0.25),"immediatePurchase":true}),
        );
        self.record(json!({"stage":"productionWitness","facts":self.production_witness}));
        self.command(
            owner,
            json!({"t":"stance","ids":[tank],"key":"holdFire","on":true}),
            None,
        );
        self.command(owner, json!({"t":"autocast","ids":[tank],"on":false}), None);
        self.command(owner, json!({"t":"stop","ids":[tank]}), None);
        (tank, site)
    }
    fn complete(&mut self, mut checks: Value) {
        checks["production"] = self
            .production_witness
            .clone()
            .expect("paid production witness");
        self.complete_checks(checks);
    }
    fn complete_checks(&mut self, checks: Value) {
        let receipt = self.replay.receipt();
        let mut input_equal = true;
        for key in [
            "capacity",
            "consumed",
            "unconsumed",
            "exhausted",
            "draws",
            "startup",
        ] {
            input_equal &= receipt[key] == self.source_input[key];
        }
        input_equal &= receipt["layout"] == self.source_input["layout"];
        let command_difference = original::compare_projection_for_test(
            &json!(self.source_commands),
            &json!(self.commands),
        );
        let predicate_difference =
            original::compare_projection_for_test(&self.source_complete, &checks);
        let prefix_equal = self.prefixes == self.source_prefixes;
        let complete = predicate_difference.is_none()
            && input_equal
            && prefix_equal
            && command_difference.is_none();
        self.record(json!({"stage":"comparison","checks":checks,"sourceChecks":self.source_complete,"inputReceipt":receipt,"sourceInputReceipt":self.source_input,
            "inputEqual":input_equal,"prefixesEqual":prefix_equal,"sourcePrefixes":self.source_prefixes,"nativePrefixes":self.prefixes,"predicateDifference":predicate_difference,"sourceCommands":self.source_commands,"nativeCommands":self.commands,"commandDifference":command_difference,"componentComplete":complete,"rowStatusChanged":false}));
        assert!(
            complete,
            "genuine component predicates or complete input/command receipts differ; raw evidence retained"
        );
    }
}

#[test]
#[ignore = "requires Root exact slot grant and new private ground input/report paths"]
fn genuine_group_retreat_takes_quarter_tank_damage_and_finishes_home() {
    let mut w = Witness::new("retreat");
    let initial_roster = retreat_predicate::roster(&w.state);
    assert_eq!(
        initial_roster.as_array().unwrap().len(),
        10,
        "this fixed constructor has two rifles, one MG, HQ and Barracks per side"
    );
    for owner in [0, 1] {
        for (kind, count) in [("rifle", 2), ("mg", 1), ("hq", 1), ("barracks", 1)] {
            assert_eq!(
                initial_roster
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|u| u["owner"] == owner && u["kind"] == kind)
                    .count(),
                count
            );
        }
    }
    w.record(json!({"stage":"retreatPredicate","initialRoster":initial_roster}));
    let (tank, _) = w.tank(1);
    w.move_to(1, tank, Point { x: 57., z: 29. });
    w.move_to(0, 1, Point { x: 45., z: 29. });
    w.move_to(0, 2, Point { x: 43., z: 33. });
    // Enable outgoing fire so the retreat gate itself must prevent firing.
    w.command(
        0,
        json!({"t":"stance","ids":[1,2],"key":"holdFire","on":false}),
        None,
    );
    w.command(0, json!({"t":"amove","orders":[[1,53.,29.]]}), None);
    w.command(
        0,
        json!({"t":"move","orders":[[1,49.,33.]],"queue":true}),
        None,
    );
    w.command(0, json!({"t":"attack","ids":[2],"target":tank}), None);
    assert!(w.state.world.game.units[&1].amove.is_some());
    assert!(!w.state.commands.units[&1].orders.is_empty());
    assert_eq!(w.state.commands.units[&2].attack_id, tank);
    let live_roster = w.state.world.game.units.keys().copied().collect::<Vec<_>>();
    assert_eq!(live_roster.len(), 12);
    let selection = retreat_predicate::selection(&w.state);
    let before_batch = w.snapshot();
    let before_foreign = format!(
        "{:?}",
        (
            &w.state.world.game.units[&tank],
            &w.state.commands.units[&tank],
            w.state.firing.get(tank),
            w.state.combat.get(tank)
        )
    );
    w.unchanged_command(0, json!({"t":"retreat","ids":[tank]}), Denial::Needs);
    w.unchanged_command(0, json!({"t":"retreat","ids":[]}), Denial::Needs);
    let hp = w.state.world.game.units[&1].hp;
    assert_eq!(hp, 100.0);
    w.command(
        0,
        json!({"t":"retreat","ids":[1,2,tank],"queue":true}),
        None,
    );
    assert_eq!(
        format!(
            "{:?}",
            (
                &w.state.world.game.units[&tank],
                &w.state.commands.units[&tank],
                w.state.firing.get(tank),
                w.state.combat.get(tank)
            )
        ),
        before_foreign
    );
    let after_batch = w.snapshot();
    for before in before_batch["units"].as_array().unwrap() {
        if before["id"] != 1 && before["id"] != 2 {
            let after = after_batch["units"]
                .as_array()
                .unwrap()
                .iter()
                .find(|u| u["id"] == before["id"])
                .unwrap();
            assert_eq!(
                before, after,
                "every actual unselected actor must remain unchanged"
            );
        }
    }
    w.record(json!({"stage":"retreatPredicate","selection":selection,"rosterBefore":before_batch,"rosterAfter":after_batch}));
    let homes = [1, 2]
        .into_iter()
        .map(|id| (id, w.state.reaction_home(id).unwrap()))
        .collect::<BTreeMap<_, _>>();
    let start_distance = [1, 2]
        .into_iter()
        .map(|id| {
            let u = &w.state.world.game.units[&id];
            let home = homes[&id];
            (id, crate::math::hypot(u.x - home.x, u.z - home.z))
        })
        .collect::<BTreeMap<_, _>>();
    let mut previous_distance = start_distance.clone();
    let mut progress = BTreeMap::from([(1_u32, 0_usize), (2_u32, 0_usize)]);
    for id in [1, 2] {
        let u = &w.state.world.game.units[&id];
        let c = &w.state.commands.units[&id];
        assert!(u.retreating && !u.path.is_empty());
        assert!(u.amove.is_none() && c.orders.is_empty() && c.attack_id == 0);
        assert_eq!(w.state.firing.get(id).unwrap().target_id, 0);
        assert!(!u.build && !u.dig && !u.entrench && u.garrison.is_none());
        let home = w
            .state
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0 && w.state.rules.kinds[u.kind].name == "hq")
            .unwrap();
        assert_eq!(u.world_goal, Some(home.at()));
    }
    w.command(1, json!({"t":"attack","ids":[tank],"target":1}), None);
    let mut hit = false;
    let mut fast = BTreeSet::new();
    let mut retreat_ticks = 0;
    let mut original_six_ticks = None;
    for _ in 0..600 {
        let retreating = [1, 2]
            .into_iter()
            .filter(|id| w.state.world.game.units[id].retreating)
            .collect::<BTreeSet<_>>();
        if retreating.is_empty() {
            break;
        }
        let positions = retreating
            .iter()
            .map(|id| (*id, w.state.world.game.units[id].at()))
            .collect::<BTreeMap<_, _>>();
        let prior_story = retreat_predicate::story(&w.state);
        let prior_mp = w
            .state
            .start
            .economy
            .players
            .iter()
            .map(|p| p.mp)
            .collect::<Vec<_>>();
        let report = w.step();
        retreat_ticks += 1;
        let tail =
            retreat_predicate::tail(&w.state, &report, &live_roster, &prior_story, &prior_mp);
        if retreat_ticks == 6 {
            original_six_ticks = Some(retreat_predicate::six_ticks(&w.state));
        }
        for id in [1, 2] {
            let u = &w.state.world.game.units[&id];
            let home = homes[&id];
            let distance = crate::math::hypot(u.x - home.x, u.z - home.z);
            if distance < previous_distance[&id] {
                *progress.get_mut(&id).unwrap() += 1;
            }
            previous_distance.insert(id, distance);
        }
        w.record(json!({"stage":"retreatPredicate","tail":tail,"relativeTick":retreat_ticks,"sixTickCheckpoint":original_six_ticks}));
        for id in &retreating {
            let u = &w.state.world.game.units[id];
            if u.retreating {
                assert_eq!(w.state.firing.get(*id).unwrap().target_id, 0);
                assert_eq!(u.supp, 0.0);
                let prior = positions[id];
                let speed =
                    crate::math::hypot(u.x - prior.x, u.z - prior.z) / w.state.rules.movement.tick;
                if speed >= w.state.rules.kinds[u.kind].speed * 1.49 {
                    fast.insert(*id);
                }
            }
        }
        assert!(!report.events.iter().any(|event| {
            match event {
                TickEvent::Firing(crate::match_firing::LaunchEvent::Flight { shooter, .. }) => w
                    .state
                    .world
                    .game
                    .units
                    .get(shooter)
                    .is_some_and(|u| u.retreating),
                TickEvent::Firing(crate::match_firing::LaunchEvent::SmallArms(e)) => w
                    .state
                    .world
                    .game
                    .units
                    .get(&e.shooter)
                    .is_some_and(|u| u.retreating),
                _ => false,
            }
        }));
        if !hit && w.state.world.game.units[&1].hp < hp {
            assert_eq!(
                w.state.world.game.units[&1].hp,
                100.0 - 30.0 * 0.25,
                "first genuine tank contact must exercise the original Retreat check"
            );
            let contact = positive_observers::retreat_hit(&report, tank, 1);
            w.record(json!({"stage":"positiveObserver","contact":contact}));
            hit = true;
            w.command(1, json!({"t":"stop","ids":[tank]}), None);
        }
    }
    assert!(hit && fast == BTreeSet::from([1, 2]) && retreat_ticks > 0);
    assert!(
        original_six_ticks.is_some(),
        "original six-tick checkpoint must actually execute"
    );
    for id in [1, 2] {
        let u = &w.state.world.game.units[&id];
        assert!(!u.retreating && u.path.is_empty() && u.world_goal.is_none());
    }
    let completion = [1, 2]
        .into_iter()
        .map(|id| {
            retreat_predicate::completion(
                &w.state,
                id,
                homes[&id],
                start_distance[&id],
                progress[&id],
            )
        })
        .collect::<Vec<_>>();
    w.record(json!({"stage":"retreatPredicate","completion":completion}));
    w.complete(json!({"originalSixTicks":original_six_ticks,"initialRoster":initial_roster,"quarterTankHit":hit,"sprintActors":fast,"retreatTicks":retreat_ticks,"groupImmediateDespiteQueue":true,"foreignPreserved":true}));
}

#[test]
#[ignore = "requires Root exact slot grant and new private ground input/report paths"]
fn genuine_scuttle_filters_carried_foreign_dead_and_retires_tank_and_building() {
    scuttle_case("scuttle", false);
}

#[test]
#[ignore = "requires a separate Root exact slot grant for the additive ordinary-Unload case"]
fn genuine_scuttle_after_ordinary_unload_retires_tank_and_building_without_bounty() {
    scuttle_case("scuttle-unloaded", true);
}

fn scuttle_case(name: &str, unload: bool) {
    let mut w = Witness::new(name);
    let (tank, site) = w.tank(0);
    w.move_to(0, tank, Point { x: 43., z: 29. });
    w.command(0, json!({"t":"board","ids":[1],"target":tank}), None);
    w.until(1000, "ordinary-rifle-boarded", |m| {
        m.world.game.units[&1].riding
    });
    assert_eq!(w.state.combat.get(1).unwrap().carrier, Some(tank));
    assert!(w.state.combat.get(tank).unwrap().cargo.contains(&1));
    for ids in [vec![1], vec![4], vec![999999], vec![1, 4, 999999]] {
        w.unchanged_command(0, json!({"t":"destroy","ids":ids}), Denial::Unseen);
    }
    if unload {
        w.command(0, json!({"t":"unload","ids":[tank]}), None);
        assert!(!w.state.world.game.units[&1].riding);
        assert_eq!(w.state.combat.get(1).unwrap().carrier, None);
        assert!(!w.state.combat.get(tank).unwrap().cargo.contains(&1));
    }
    let selected_before = w.snapshot();
    let unloaded_story = unload.then(|| w.state.story.clone());
    let unloaded_ids = unload.then(|| w.state.world.game.units.keys().copied().collect::<Vec<_>>());
    let preserved = selected_before["units"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|u| u["id"] == 1 || u["id"] == 4)
        .cloned()
        .collect::<Vec<_>>();
    let wallets = w
        .state
        .start
        .economy
        .players
        .iter()
        .map(|p| p.mp.to_bits())
        .collect::<Vec<_>>();
    // The scalar and list selectors both act. The carried and foreign rows stay alive.
    let destroy_ids = if unload {
        vec![tank, 4]
    } else {
        vec![tank, 1, 4]
    };
    w.command(0, json!({"t":"destroy","ids":destroy_ids,"id":site}), None);
    for id in [tank, site] {
        assert_eq!(w.state.world.game.units[&id].hp, 0.0);
        assert_eq!(w.state.combat.get(id).unwrap().last_hit, Some(0));
    }
    assert!(w.state.world.game.units[&1].hp > 0.0 && w.state.world.game.units[&4].hp > 0.0);
    let after = w.snapshot();
    assert_eq!(
        preserved,
        after["units"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|u| u["id"] == 1 || u["id"] == 4)
            .cloned()
            .collect::<Vec<_>>()
    );
    assert_eq!(
        wallets,
        w.state
            .start
            .economy
            .players
            .iter()
            .map(|p| p.mp.to_bits())
            .collect::<Vec<_>>()
    );
    for ids in [vec![tank], vec![site], vec![tank, site]] {
        w.unchanged_command(0, json!({"t":"destroy","ids":ids}), Denial::Unseen);
    }
    let report = w.step();
    for id in [tank, site] {
        assert!(!w.state.world.game.units.contains_key(&id));
        let retired = report
            .retired
            .iter()
            .find(|r| r.unit.id == id)
            .expect("actual next-tick retirement");
        assert_eq!(retired.last_hit, Some(0));
        assert_eq!(retired.enemy_killer, None);
    }
    assert!(w.state.world.game.wrecks.iter().any(|r| r.id == Some(tank)));
    // Conquest income is exactly inc * TICK, so any refund or enemy bounty is observable.
    let dt = w.state.rules.movement.tick;
    for (owner, bits) in wallets.iter().enumerate() {
        let p = &w.state.start.economy.players[owner];
        assert_eq!(
            p.mp.to_bits(),
            (f64::from_bits(*bits) + p.inc * dt).to_bits()
        );
    }
    assert!(w.state.world.game.units[&1].hp > 0.0 && !w.state.world.game.units[&1].riding);
    if unload {
        let observed = positive_observers::unloaded_scuttle(
            &w.state,
            &report,
            tank,
            site,
            1,
            unloaded_story.as_ref().unwrap(),
            unloaded_ids.as_ref().unwrap(),
        );
        w.record(json!({"stage":"positiveObserver","unloadedScuttle":observed}));
    }
    w.complete(json!({"carriedForeignDeadFiltered":true,"nextTickTankAndBuildingRetired":true,"wreck":tank,"noRefundOrBounty":true,"ordinaryUnload":unload}));
}
