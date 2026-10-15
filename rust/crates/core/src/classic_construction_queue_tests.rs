//! Literal queued construction and training checks over independent Classic state.
use super::*;

// Typed canonical preservation is independent of the recorded Source projection.
// The inherited observer also retains controls, paid jobs, wallets, structures,
// sections, grid queries, metadata, terrain logs and recipient pending sets.
struct Preservation {
    state: AtomicState,
    classic: Option<(u64, usize, bool)>,
    tick: i64,
    next: u32,
    chars: Vec<u8>,
}
impl Preservation {
    fn capture(m: &Match) -> Self {
        Self {
            state: AtomicState::capture(m),
            classic: m.classic_runtime.as_ref().map(|state|
                (state.time_left.to_bits(), state.initial_teams, state.sudden_death)),
            tick: m.world.game.tick,
            next: m.next_unit_id,
            chars: m.world.game.chars.clone(),
        }
    }
    fn unchanged(self, m: &Match) {
        assert_eq!(m.classic_runtime.as_ref().map(|state|
            (state.time_left.to_bits(), state.initial_teams, state.sudden_death)), self.classic);
        assert_eq!(m.world.game.tick, self.tick);
        assert_eq!(m.next_unit_id, self.next);
        assert_eq!(m.world.game.chars, self.chars);
        self.state.unchanged(m);
    }
}
fn number_bits(v: &Json) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn source_unit(raw: &Json, id: UnitId) -> &Json {
    raw["units"].as_array().unwrap().iter().find(|u| u["id"] == id).unwrap()
}
fn maybe_own(m: &Match, owner: usize, kind: &str) -> Option<UnitId> {
    m.world.game.units.values()
        .find(|u| u.owner == owner && m.rules.kinds[u.kind].name == kind).map(|u| u.id)
}
fn compare_nodes(m: &Match, raw: &Json) {
    let actual = m.classic_runtime.as_ref().unwrap().nodes.iter().map(|n| {
        json!([n.cell, n.depot.unwrap_or(0), bits(n.rate), n.fuel])
    }).collect::<Vec<_>>();
    assert_eq!(json!(actual), raw["nodes"]);
}
fn compare_orders(m: &Match, raw: &Json) {
    for (id, control) in &m.commands.units {
        let source = source_unit(raw, *id);
        let expected = source["orders"].as_array().unwrap();
        assert_eq!(control.orders.len(), expected.len());
        for (order, source) in control.orders.iter().zip(expected) {
            match order {
                Command::Move { orders, control_ids, attack_move, queue, together, face } => {
                    assert_eq!(source["t"], "move");
                    assert!(control_ids.is_none() && !attack_move && !queue && !together);
                    assert!(face.is_none());
                    assert_eq!(orders.len(), 1);
                    assert_eq!(orders[0].unit, i64::from(*id));
                    assert_eq!(source["orders"], json!([[id, orders[0].at.x, orders[0].at.z]]));
                }
                Command::Assist { ids, target, at, queue } => {
                    assert_eq!(source["t"], "assist");
                    assert_eq!(ids.as_slice(), [i64::from(*id)]);
                    assert!(!queue);
                    assert_eq!(json!(target), source["id"]);
                    let at = at.unwrap();
                    assert_eq!(at.x.to_bits(), source["x"].as_f64().unwrap().to_bits());
                    assert_eq!(at.z.to_bits(), source["z"].as_f64().unwrap().to_bits());
                }
                _ => panic!("unrepresented waiting command"),
            }
        }
        assert_eq!(json!(control.build_target), source["build"]);
    }
}
fn compare_jobs(m: &Match, raw: &Json) {
    let catalog = Catalog::compiled().unwrap();
    for (id, b) in m.production.buildings() {
        let unit = &m.world.game.units[&id];
        let def = catalog.unit(&m.rules.kinds[unit.kind].name).unwrap();
        // Depot bookkeeping is canonical internally but Source has no queue on
        // a building without a makes list. Compare actual producers only.
        if def.get("makes").is_none() { continue; }
        let source = source_unit(raw, id);
        assert_eq!(json!(b.queue().collect::<Vec<_>>()), source["queue"]);
        assert_eq!(bits(b.progress()), source["prog"].as_str().unwrap());
        let jobs = source["jobs"].as_array().unwrap();
        let rows = b.rows();
        assert_eq!(rows.len(), jobs.len());
        for (row, source) in rows.iter().zip(jobs) {
            assert_eq!(json!(row.job.id), source["id"]);
            assert_eq!(json!(row.job.kind), source["type"]);
            assert_eq!(row.job.charge.mp.to_bits(), source["mp"].as_f64().unwrap().to_bits());
            assert_eq!(row.job.charge.fuel.to_bits(), source["fuel"].as_f64().unwrap().to_bits());
            assert_eq!(source["status"], if row.status == JobStatus::Active { "active" } else { "waiting" });
        }
    }
}
fn completed_site(m: &Match, raw: &Json, kind: &str) {
    let id = own(m, 0, kind);
    let site = &m.world.game.units[&id];
    let source = source_unit(raw, id);
    let hp = Catalog::compiled().unwrap().unit(kind).unwrap()
        .get("hpPer").and_then(Value::number).unwrap();
    assert_eq!(site.built, Some(1.));
    assert_eq!(number_bits(&source["built"]), 1.);
    assert_eq!(site.hp, hp);
    assert_eq!(bits(site.hp), source["hp"].as_str().unwrap());
    assert_eq!(m.story[0].built, 1.);
    for &cell in site.cells.as_ref().unwrap() {
        assert_eq!(m.world.game.chars[cell as usize], b'K');
        assert_eq!(bits(f64::from(m.world.game.cell_hp[cell as usize])),
            raw["cellHp"][cell as usize].as_str().unwrap());
    }
}
fn replay(shared: bool) -> Match {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/classic-construction-queue.json");
    let fixture: Json = serde_json::from_str(&std::fs::read_to_string(path)
        .expect("the sixth Source recipe requires a new exact-head grant before native replay")).unwrap();
    assert_eq!(fixture["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    let name = if shared { "allied-engineer-assist-queue-and-owner-cancel" }
        else { "full-queue-barracks-retry-and-real-mg" };
    let case = cases.iter().find(|case| case["name"] == name).unwrap();
    assert_eq!(case["input"], input(shared));
    let mut m = make(&input(shared));
    let engineer = own(&m, 0, "engineer");
    let ally = shared.then(|| own(&m, 1, "engineer"));
    compare_nodes(&m, &case["initial"]);
    compare_orders(&m, &case["initial"]);
    compare_jobs(&m, &case["initial"]);
    let (mut commands, mut steps) = (0, 0);
    let mut refused_build = None;
    let mut phases = BTreeSet::new();
    for row in case["records"].as_array().unwrap() {
        assert_eq!(json!(m.world.game.tick), row["before"]["tick"]);
        match row["op"].as_str().unwrap() {
            "command" => {
                let slot = row["slot"].as_u64().unwrap() as usize;
                let command = Command::from_json(&row["cmd"]).unwrap();
                let before = Preservation::capture(&m);
                let mp = m.start.economy.players[slot].mp;
                let fuel = m.start.economy.players[slot].fuel;
                let next = m.next_unit_id;
                let cells = match command {
                    Command::CancelConstruction { id: Some(id) } => m.world.game.units[&id].cells.clone(),
                    _ => None,
                };
                let result = m.execute_command(slot, &command, false).unwrap();
                assert_eq!(json!(result.map(|d| d.name())), row["result"]);
                if result.is_some() {
                    before.unchanged(&m);
                    if matches!(command, Command::Build { queue: true, .. }) {
                        assert_eq!(result, Some(Denial::QueueFull));
                        assert_eq!(m.commands.units[&engineer].orders.len(), 8);
                        refused_build = Some(command.clone());
                    }
                } else {
                    match &command {
                        Command::Build { kind, queue, .. } => {
                            if *queue {
                                assert_eq!(refused_build.as_ref(), Some(&command));
                                assert_eq!(m.commands.units[&engineer].orders.len(), 1);
                            }
                            let site = &m.world.game.units[&next];
                            let def = Catalog::compiled().unwrap().unit(kind).unwrap();
                            let cost = def.get("cost").and_then(Value::number).unwrap();
                            assert_eq!(site.built, Some(0.));
                            assert_eq!(site.hp, def.get("hpPer").and_then(Value::number).unwrap() * 0.25);
                            assert_eq!(bits(site.hp), source_unit(&row["after"], next)["hp"].as_str().unwrap());
                            assert_eq!(m.start.economy.players[slot].mp, mp - cost);
                            assert_eq!(number_bits(&row["after"]["wallets"][slot][0]),
                                number_bits(&row["before"]["wallets"][slot][0]) - cost);
                            assert_eq!(site.cells.as_ref().unwrap().len(), if kind == "barracks" { 9 } else { 4 });
                        }
                        Command::Stop { .. } => {
                            assert!(m.commands.units[&engineer].orders.is_empty());
                            assert_eq!(m.commands.units[&engineer].build_target, None);
                        }
                        Command::Assist { .. } => {
                            assert_eq!(m.start.economy.players[slot].mp, mp);
                            assert_eq!(m.start.economy.players[slot].fuel, fuel);
                        }
                        Command::Buy { kind, from } => {
                            assert_eq!(kind, "mg");
                            let site = from.unwrap();
                            assert_eq!(m.world.game.units[&site].built, Some(1.));
                            let jobs = m.production.building(site).unwrap().jobs();
                            assert_eq!(jobs.len(), 1);
                            assert_eq!(m.start.economy.players[slot].mp, mp - jobs[0].charge.mp);
                            assert_eq!(m.start.economy.players[slot].fuel, fuel - jobs[0].charge.fuel);
                        }
                        Command::CancelConstruction { id: Some(id) } => {
                            assert_eq!(slot, 0);
                            assert!(!m.world.game.units.contains_key(id));
                            assert!(!m.commands.units.contains_key(id));
                            assert!(m.production.building(*id).is_none());
                            assert!(!m.world.structures.iter().any(|s| s.entity_id == Some(*id)));
                            assert!(m.classic_runtime.as_ref().unwrap().nodes.iter().all(|n| n.depot != Some(*id)));
                            assert_eq!(m.start.economy.players[0].mp, mp + 45.);
                            for cell in cells.unwrap() {
                                assert_eq!(m.world.game.chars[cell as usize], b'.');
                                assert!(!m.world.sections.contains_key(&(cell as usize)));
                                assert!(!m.world.game.sections.contains_key(&cell));
                            }
                        }
                        _ => {}
                    }
                }
                commands += 1;
            }
            "advance" => {
                let count = row["count"].as_u64().unwrap();
                for _ in 0..count { m.advance_classic_depot_tick().unwrap(); }
                steps += count;
                let phase = row["phase"].as_str().unwrap();
                assert!(phases.insert(phase.to_owned()));
                match phase {
                    "earned" => {
                        let catalog = Catalog::compiled().unwrap();
                        let cost = |kind| catalog.unit(kind).unwrap()
                            .get("cost").and_then(Value::number).unwrap();
                        assert!(m.start.economy.players[0].mp >= cost("barracks") + cost("mg"));
                    }
                    "moves-still-waiting" => {
                        assert_eq!(m.commands.units[&engineer].orders.len(), 8);
                        m.validate_classic_depot_components().unwrap();
                    }
                    "queued-assist-start" => {
                        assert_eq!(m.commands.units[&engineer].build_target, Some(own(&m, 0, "barracks")));
                        assert!(m.commands.units[&engineer].orders.is_empty());
                    }
                    "barracks-complete" => completed_site(&m, &row["after"], "barracks"),
                    "still-training" => {
                        assert!(maybe_own(&m, 0, "mg").is_none());
                        let producer = m.production.building(own(&m, 0, "barracks")).unwrap();
                        assert_eq!(producer.jobs().len(), 1);
                        assert_eq!(producer.jobs()[0].kind, "mg");
                        assert!(producer.progress() > 0. && producer.progress() < 18.);
                    }
                    "mg-born" => {
                        let id = own(&m, 0, "mg");
                        let source = source_unit(&row["after"], id);
                        assert_eq!(source["type"], "mg");
                        assert_eq!(bits(m.world.game.units[&id].hp), source["hp"].as_str().unwrap());
                        let barracks = own(&m, 0, "barracks");
                        assert!(m.production.building(barracks).unwrap().jobs().is_empty());
                        assert_eq!(m.commands.units[&id].route_end, m.building_rallies[&barracks]);
                        assert_eq!(point(m.commands.units[&id].route_end), source["routeEnd"]);
                    }
                    "cancelled-assist-cleared" => {
                        for id in [engineer, ally.unwrap()] {
                            assert_eq!(m.commands.units[&id].build_target, None);
                            assert!(m.commands.units[&id].orders.is_empty());
                        }
                        assert!(maybe_own(&m, 0, "depot").is_none());
                    }
                    "allied-complete-and-queue-cleared" => {
                        completed_site(&m, &row["after"], "depot");
                        for id in [engineer, ally.unwrap()] {
                            assert_eq!(m.commands.units[&id].build_target, None);
                            assert!(m.commands.units[&id].orders.is_empty());
                        }
                        assert_eq!(m.story[0].mp_spent, 75.);
                        assert_eq!(m.story[1].mp_spent, 0.);
                    }
                    "warmup" => {}
                    _ => panic!("unexpected queue scene phase"),
                }
            }
            _ => panic!("unexpected queue scene record"),
        }
        assert_eq!(json!(m.world.game.tick), row["after"]["tick"]);
        compare_nodes(&m, &row["after"]);
        compare_orders(&m, &row["after"]);
        compare_jobs(&m, &row["after"]);
        assert!(m.validate_construction_tick().is_err());
    }
    assert_eq!((commands, steps), if shared { (25, 502) } else { (16, 2182) });
    assert_eq!(phases.len(), if shared { 3 } else { 6 });
    m
}

#[test]
fn base26_classic_eight_moves_refuse_paid_barracks_stop_retries_and_real_mg_trains() {
    let m = replay(false);
    assert_eq!(m.world.game.units[&own(&m, 0, "barracks")].built, Some(1.));
    assert!(maybe_own(&m, 0, "mg").is_some());
}

#[test]
fn base29_classic_allied_assist_owner_cancel_and_natural_completion_clear_waiting_work() {
    let m = replay(true);
    assert_eq!(m.world.game.units[&own(&m, 0, "depot")].built, Some(1.));
    assert_eq!(m.story[0].losses, 0.);
    assert_eq!(m.story[1].losses, 0.);
}

#[test]
fn base26_classic_queued_barracks_story_failure_preserves_real_eight_moves_and_site_state() {
    let mut m = make(&input(false));
    for _ in 0..1000 { m.advance_classic_depot_tick().unwrap(); }
    let engineer = own(&m, 0, "engineer");
    let hq = m.world.game.units[&own(&m, 0, "hq")].at();
    let tick = m.world.game.tick as u64 + 1;
    let enqueue = |m: &mut Match, tick, sequence, value: Json| {
        m.enqueue_command(crate::orders::OrderKey { tick, issuer: 0, sequence },
            Command::from_json(&value).unwrap()).unwrap();
    };
    enqueue(&mut m, tick, 0,
        json!({"t":"move","orders":[[engineer,hq.x,hq.z+20.]]}));
    for i in 0..8 {
        enqueue(&mut m, tick, i + 1,
            json!({"t":"move","orders":[[engineer,hq.x+i as f64*2.,hq.z+24.]],"queue":true}));
    }
    let results = m.advance_classic_depot_tick().unwrap();
    assert_eq!(results.len(), 9);
    assert!(results.iter().all(|result| result.denial.is_none()));
    assert_eq!(m.commands.units[&engineer].orders.len(), 8);
    let build = Command::from_json(&json!({"t":"build","kind":"barracks","ids":[engineer],
        "x":hq.x+20.,"z":hq.z,"queue":true})).unwrap();
    let before = Preservation::capture(&m);
    assert_eq!(m.execute_command(0, &build, false).unwrap(), Some(Denial::QueueFull));
    before.unchanged(&m);
    // The actual recorded boundary must also consume Build refusal then Stop.
    let tick = m.world.game.tick as u64 + 1;
    let next = m.next_unit_id;
    m.enqueue_command(crate::orders::OrderKey { tick, issuer: 0, sequence: 9 }, build.clone()).unwrap();
    enqueue(&mut m, tick, 10, json!({"t":"stop","ids":[engineer]}));
    let results = m.advance_classic_depot_tick().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].denial, Some(Denial::QueueFull));
    assert_eq!(results[1].denial, None);
    assert_eq!(m.next_unit_id, next);
    assert!(m.commands.units[&engineer].orders.is_empty());
    assert!(maybe_own(&m, 0, "barracks").is_none());
    // Only story bookkeeping changes. The real queued moves, Stop, wallet,
    // actors, terrain, canonical nodes and fixed clock supplied this scene.
    m.story.clear();
    let before = Preservation::capture(&m);
    let error = m.execute_command(0, &build, false).unwrap_err();
    assert!(error.contains("story"), "must reach the final staged story guard: {error}");
    before.unchanged(&m);
}
