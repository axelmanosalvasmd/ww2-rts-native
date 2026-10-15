//! Literal Depot lifecycle checks over actual constructors and component consumers.
use super::*;

fn number_bits(v: &Json) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn source_unit(raw: &Json, id: UnitId) -> Option<&Json> {
    raw["units"].as_array().unwrap().iter().find(|u| u["id"] == id)
}
fn prepare_site() -> (Match, usize, UnitId) {
    let mut m = make(&input(false));
    m.advance_classic_depot_tick().unwrap();
    let (index, at) = choose(&m);
    let engineer = own(&m, 0, "engineer");
    let id = m.next_unit_id;
    let command = Command::from_json(&json!({"t":"build","ids":[engineer],"kind":"depot","x":at.x,"z":at.z})).unwrap();
    assert_eq!(m.execute_command(0, &command, false).unwrap(), None);
    assert!(!m.world.game.units[&engineer].path.is_empty());
    (m, index, id)
}
fn nodes_match(m: &Match, raw: &Json) {
    let expected = raw["nodes"].as_array().unwrap();
    let nodes = &m.classic_runtime.as_ref().unwrap().nodes;
    assert_eq!(nodes.len(), expected.len());
    for (node, source) in nodes.iter().zip(expected) {
        assert_eq!(json!(node.cell), source["c"]);
        assert_eq!(bits(node.at.x), source["x"].as_str().unwrap());
        assert_eq!(bits(node.at.z), source["z"].as_str().unwrap());
        assert_eq!(json!(node.depot.unwrap_or(0)), source["depot"]);
        assert_eq!(bits(node.rate), source["rate"].as_str().unwrap());
        assert_eq!(json!(node.fuel), source["fuel"]);
    }
}
fn replay_lifecycle(name: &str) -> Match {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/classic-depot-lifecycle.json");
    let fixture: Json = serde_json::from_str(&std::fs::read_to_string(path)
        .expect("Root must grant the separate lifecycle Source recipe before replay")).unwrap();
    assert_eq!(fixture["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 2);
    let case = fixture["cases"].as_array().unwrap().iter().find(|c| c["name"] == name).unwrap();
    assert_eq!(case["input"], input(false));
    let mut m = make(&case["input"]);
    nodes_match(&m, &case["initial"]);
    let mut commands = 0;
    let mut steps = 0;
    for row in case["records"].as_array().unwrap() {
        assert_eq!(json!(m.world.game.tick), row["before"]["tick"]);
        match row["op"].as_str().unwrap() {
            "advance" => {
                let count = row["count"].as_u64().unwrap();
                let wallet = m.start.economy.players[0].mp;
                for _ in 0..count { m.advance_classic_depot_tick().unwrap(); }
                steps += count;
                for site in row["after"]["units"].as_array().unwrap().iter().filter(|u| u["type"] == "depot") {
                    let actual = &m.world.game.units[&(site["id"].as_u64().unwrap() as u32)];
                    let progress = number_bits(&site["built"]);
                    if progress == 1. {
                        // The original literal completes within its work window. Whole-match
                        // poses, routes and the earlier income timeline are not this check.
                        assert_eq!(actual.built, Some(1.));
                        assert_eq!(bits(actual.hp), site["hp"].as_str().unwrap());
                        assert_eq!(m.story[actual.owner].built, 1.);
                        for &cell in actual.cells.as_ref().unwrap() {
                            assert_eq!(m.world.game.chars[cell as usize], b'K');
                            assert_eq!(bits(f64::from(m.world.game.cell_hp[cell as usize])),
                                row["after"]["cellHp"][cell as usize].as_str().unwrap());
                            assert_eq!(m.world.game.sections[&cell].hp, actual.hp / 4.);
                        }
                    } else if progress > 0. {
                        assert!(actual.built.is_some_and(|b| b > 0. && b < 1.));
                    }
                }
                if count == 1 && commands == 2 && name == "natural-completion-and-finished-refusal" {
                    let source_delta = number_bits(&row["after"]["wallets"][0][0])
                        - number_bits(&row["before"]["wallets"][0][0]);
                    assert!((m.start.economy.players[0].mp - wallet - source_delta).abs() < 1e-10);
                    assert_eq!(bits(m.start.economy.players[0].inc), row["after"]["income"][0][0].as_str().unwrap());
                }
            }
            "command" => {
                let before = AtomicState::capture(&m);
                let wallet = m.start.economy.players[0].mp;
                let command = Command::from_json(&row["cmd"]).unwrap();
                let id = row["cmd"]["id"].as_u64().map(|id| id as u32);
                let cells = id.and_then(|id| m.world.game.units.get(&id))
                    .and_then(|u| u.cells.clone());
                let denial = m.execute_command(row["slot"].as_u64().unwrap() as usize, &command, false).unwrap();
                assert_eq!(json!(denial.map(|d| d.name())), row["result"]);
                if denial.is_some() { before.unchanged(&m); }
                else {
                    let delta = number_bits(&row["after"]["wallets"][0][0])
                        - number_bits(&row["before"]["wallets"][0][0]);
                    assert_eq!(m.start.economy.players[0].mp, wallet + delta);
                    assert_eq!(m.story[0].mp_spent, row["after"]["story"][0]["mpSpent"].as_f64().unwrap());
                    assert_eq!(m.story[0].losses, 0.);
                    if let Command::CancelConstruction { id: Some(id) } = command {
                        assert!(source_unit(&row["after"], id).is_none());
                        assert!(!m.world.game.units.contains_key(&id));
                        assert!(!m.commands.units.contains_key(&id));
                        assert!(m.production.building(id).is_none());
                        assert!(!m.building_rallies.contains_key(&id));
                        assert!(m.firing.get(id).is_none());
                        assert!(m.abilities.get(id).is_none());
                        assert!(m.recovery.get(id).is_none());
                        assert!(!m.world.structures.iter().any(|s| s.entity_id == Some(id)));
                        for cell in cells.unwrap() {
                            assert_eq!(m.world.game.chars[cell as usize], b'.');
                            assert!(!m.world.sections.contains_key(&(cell as usize)));
                            assert!(!m.world.game.sections.contains_key(&cell));
                        }
                        assert!(m.environment.falling_sections().is_empty());
                        assert_eq!(m.world.game.units[&own(&m, 0, "engineer")].build,
                            source_unit(&row["after"], own(&m, 0, "engineer")).unwrap()["build"].is_number());
                    }
                }
                commands += 1;
            }
            _ => panic!("unexpected lifecycle record"),
        }
        assert_eq!(json!(m.world.game.tick), row["after"]["tick"]);
        nodes_match(&m, &row["after"]);
    }
    assert_eq!((commands, steps), if name == "natural-completion-and-finished-refusal" { (2, 702) } else { (6, 202) });
    m
}

#[test]
fn base26_classic_actual_engineer_completes_depot_sections_and_earns_node_income() {
    let m = replay_lifecycle("natural-completion-and-finished-refusal");
    let site = &m.world.game.units[&own(&m, 0, "depot")];
    assert_eq!(site.built, Some(1.));
    assert_eq!(site.hp, 600.);
    assert!(m.validate_construction_tick().is_err());
}

#[test]
fn base26_classic_actual_partial_cancel_refunds_releases_node_and_allows_rebuild() {
    let m = replay_lifecycle("partial-cancel-release-rebuild-and-worker-clear");
    assert!(m.classic_runtime.as_ref().unwrap().nodes.iter().all(|n| n.depot.is_none()));
    assert_eq!(m.commands.units[&own(&m, 0, "engineer")].build_target, None);
    assert_eq!(m.story[0].mp_spent, 30.);
    assert_eq!(m.story[0].losses, 0.);
}

#[test]
fn base26_classic_actual_cancel_story_failure_preserves_site_node_cells_wallet_and_logs() {
    let (mut m, index, id) = prepare_site();
    m.story.clear();
    let before = AtomicState::capture(&m);
    let error = m.execute_command(0, &Command::CancelConstruction { id: Some(id) }, false).unwrap_err();
    assert!(error.contains("story"), "must reach prepared story boundary: {error}");
    before.unchanged(&m);
    assert_eq!(m.classic_runtime.as_ref().unwrap().nodes[index].depot, Some(id));
}

#[test]
fn base26_classic_actual_cancel_nonfinite_wallet_preserves_site_node_and_logs() {
    let (mut m, _, id) = prepare_site();
    // Wallet bookkeeping only. Actors, terrain, node, HP and clock stay genuine.
    m.start.economy.players[0].mp = f64::INFINITY;
    let before = AtomicState::capture(&m);
    let error = m.execute_command(0, &Command::CancelConstruction { id: Some(id) }, false).unwrap_err();
    assert!(error.contains("wallet"));
    before.unchanged(&m);
}

#[test]
fn base26_classic_actual_completion_story_guard_preserves_captured_site_and_sections() {
    let (mut m, _, id) = prepare_site();
    // Reach the final crew turn through actual fixed ticks and actual walking.
    let duration = Catalog::compiled().unwrap().unit("depot").unwrap()
        .get("buildTime").and_then(Value::number).unwrap();
    let rate = 1. / duration * m.rules.movement.tick;
    for _ in 0..700 {
        let progress = m.world.game.units[&id].built.unwrap();
        if progress < 1. && progress + rate >= 1. { break; }
        m.advance_classic_depot_tick().unwrap();
    }
    let progress = m.world.game.units[&id].built.unwrap();
    assert!(progress < 1. && progress + rate >= 1.);
    let engineer = own(&m, 0, "engineer");
    let mut frame = ConstructionFrame { tick: m.world.game.tick,
        sites: Vec::new(), indexes: BTreeMap::new(), workers: BTreeSet::new() };
    m.step_construction_worker_in_context(engineer, &mut frame).unwrap();
    assert_eq!(frame.sites[0], (id, vec![engineer]));
    m.story.clear();
    let before = AtomicState::capture(&m);
    let error = m.finish_construction_in_context(frame).unwrap_err();
    assert!(error.contains("story"), "must reach completion story guard: {error}");
    before.unchanged(&m);
}
