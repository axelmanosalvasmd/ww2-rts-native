//! Unrun supplemental CMB-131 witnesses for the two verified review findings.

use serde_json::{Value, json};
use simtest::load::load_game;
use ww2_core::fog::Fog;
use ww2_core::game::Game;
use ww2_core::grid::SpatialGrid;
use ww2_core::rules::Rules;
use ww2_core::vision::{update_vision, update_vision_threaded, vision_range};

fn fixture() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/vision-fog-witnesses.json");
    let bytes = std::fs::read(path).expect("generate supplemental fog witnesses with a Root slot");
    let fixture: Value = serde_json::from_slice(&bytes).expect("supplemental fog fixture JSON");
    assert_eq!(fixture["pin"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    fixture
}

fn observe(g: &Game, rules: &Rules, fog: &mut Fog, threaded: bool) -> Value {
    if threaded {
        fog.teams_threaded(g, rules, &[0, 1]);
    }
    json!([0, 1].map(|team| {
        let f = fog.team(g, rules, team);
        json!({"team":team,"vis":f.vis,"explored":f.explored,
            "version":f.version,"deltas":f.deltas})
    }))
}

fn advance(frame: &Value, rules: &Rules, threaded: bool) -> Game {
    let mut g = load_game(&frame["game"], rules).unwrap().game;
    let grid = SpatialGrid::new(&g.units, 16.0);
    if threaded {
        update_vision_threaded(&mut g, rules, &grid);
    } else {
        update_vision(&mut g, rules, &grid);
    }
    let visible: Vec<_> = g.players.iter().map(|p| &p.visible).collect();
    assert_eq!(json!(visible), frame["visible"], "{}", frame["name"]);
    let mut post = frame["game"].clone();
    post["visionTick"] = json!(g.vision_tick.unwrap());
    for (slot, p) in g.players.iter().enumerate() {
        post["players"][slot]["visible"] = json!({"$set":p.visible});
    }
    assert_eq!(post, frame["postVisionInput"], "only actual vision outputs change");
    g
}

fn changed_cells(before: &Value, after: &Value) -> Vec<usize> {
    let before = before.as_array().unwrap();
    let after = after.as_array().unwrap();
    assert_eq!(before.len(), after.len());
    before
        .iter()
        .zip(after)
        .enumerate()
        .filter_map(|(cell, (a, b))| (a != b).then_some(cell))
        .collect()
}

fn assert_cold(g: &Game, rules: &Rules, cached: &Value, expected: &Value, threaded: bool) {
    let cold = observe(g, rules, &mut Fog::default(), threaded);
    assert_eq!(cold, *expected);
    for team in 0..2 {
        assert_eq!(cached[team]["vis"], cold[team]["vis"]);
    }
}

fn assert_height_sight_domain(g: &Game, rules: &Rules, case: &Value) {
    use ww2_core::game::{CELL, SIGHT, WOOD};
    use ww2_core::math;

    let observer = &g.units[&(case["observer"].as_u64().unwrap() as u32)];
    assert_eq!(rules.kinds[observer.kind].name, "rifle");
    assert!(observer.garrison.is_none() && observer.air.is_none());
    assert_eq!(g.level_at(observer.x, observer.z), 0);
    assert_eq!(g.rain, 0.0);
    let reach = math::max(6.0, vision_range(g, rules, observer));
    assert_eq!(reach, 36.0);
    let domain = &case["sightDomain"];
    assert_eq!(domain["reach"].as_f64().unwrap(), reach);
    let bases = [(10, 0, 5, 5, 97), (11, 0, 17, 5, 103),
        (12, 1, 5, 123, 5761), (13, 1, 17, 123, 5767),
        (14, 2, 187, 123, 5852), (15, 2, 175, 123, 5846)];
    let expected_bases: Vec<_> = bases.iter().map(|&(id, owner, x, z, c)| {
        json!({"id":id,"owner":owner,"x":x,"z":z,"c":c})
    }).collect();
    assert_eq!(json!(expected_bases), domain["bases"]);
    let mut canonical: Vec<usize> = bases.iter().flat_map(|&(_, _, _, _, c)| {
        (0..9).map(move |i| c + i % 3 + i / 3 * g.w)
    }).collect();
    canonical.sort_unstable();
    let flagged: Vec<_> = g.flags.iter().enumerate()
        .filter_map(|(cell, flags)| (flags & (SIGHT | WOOD) != 0).then_some(cell)).collect();
    assert_eq!(flagged, canonical);
    assert_eq!(flagged.len(), 54);
    assert_eq!(g.sections.len(), 54);
    let exceptions = domain["exceptions"].as_array().unwrap();
    let structures = domain["structures"].as_array().unwrap();
    assert_eq!(exceptions.len(), 54);
    assert_eq!(structures.len(), 6);
    let mut minimum = f64::INFINITY;
    for (cell, declared) in flagged.into_iter().zip(exceptions) {
        assert_eq!(declared["cell"].as_u64().unwrap() as usize, cell);
        let (x, y) = (cell % g.w, cell / g.w);
        let dx = math::max(math::max(x as f64 * CELL - observer.x, 0.0), observer.x - (x + 1) as f64 * CELL);
        let dz = math::max(math::max(y as f64 * CELL - observer.z, 0.0), observer.z - (y + 1) as f64 * CELL);
        let distance = math::hypot(dx, dz);
        assert!(distance > reach, "entire flagged cell outside possible sight domain");
        assert_eq!(distance.to_bits(), declared["distance"].as_f64().unwrap().to_bits());
        minimum = math::min(minimum, distance);
        assert_eq!(g.flags[cell], 1027);
        assert_eq!(g.flags[cell] & WOOD, 0);
        assert_eq!(declared["flags"], 1027);
        assert_eq!(declared["chars"], "K");
        assert_eq!(declared["ground"], ".");
        assert_eq!(declared["object"], "K");
        let &(id, owner, bx, bz, _) = bases.iter().find(|&&(_, _, _, _, c)| {
            (0..9).any(|i| c + i % 3 + i / 3 * g.w == cell)
        }).unwrap();
        assert_eq!(declared["building"], json!({"id":id,"owner":owner,"x":bx,"z":bz}));
        let section = &declared["section"];
        assert_eq!(section["id"], json!(format!("building:{id}:{cell}")));
        assert_eq!(section["structureId"], json!(format!("building:{id}")));
        assert_eq!(section["entityId"], id);
        assert_eq!(section["c"].as_u64().unwrap() as usize, cell);
        assert_eq!(section["state"], "intact");
        let actual = &g.sections[&(cell as u32)];
        assert!(!actual.failed && actual.hp > 0.0);
        assert_eq!(actual.hp.to_bits(), section["hp"].as_f64().unwrap().to_bits());
        let structure = structures.iter().find(|s| s["entityId"] == id).unwrap();
        assert_eq!(structure["id"], section["structureId"]);
        assert_eq!(structure["kind"], "building");
        let sections = structure["sections"].as_array().unwrap();
        assert_eq!(sections.len(), 9);
        assert!(sections.contains(section));
        assert!(!g.units.contains_key(&(id as u32)));
    }
    assert_eq!(minimum.to_bits(), domain["minimumRectangleDistance"].as_f64().unwrap().to_bits());
    for key in ["editCell", "probeCell"] {
        assert_eq!(g.flags[case[key].as_u64().unwrap() as usize] & (SIGHT | WOOD), 0);
    }
}

#[test]
fn cmb131_seen_enemy_cell_requires_actual_visible_fact() {
    let fixture = fixture();
    let rules = Rules::compiled().unwrap();
    let case = &fixture["seenGround"];
    let target = case["target"].as_u64().unwrap() as u32;
    let target_cell = case["targetCell"].as_u64().unwrap() as usize;
    let frames = case["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 3);
    for threaded in [false, true] {
        let (mut actual_fog, mut control_fog) = (Fog::default(), Fog::default());
        for frame in frames {
            let g = advance(frame, &rules, threaded);
            assert!(g.players[0].visible.contains(&target));
            assert!(g.players[1].visible.contains(&target));
            assert!(g.smokes.is_empty() && g.strikes.is_empty());
            let dust_age = g.tick as f64 - g.units[&target].dust.unwrap();
            assert!((0.0..20.0).contains(&dust_age));
            let actual = observe(&g, &rules, &mut actual_fog, threaded);
            assert_eq!(actual, frame["expected"]);
            assert_cold(&g, &rules, &actual, &frame["coldExpected"], threaded);

            let allied: Vec<_> = g.units.values().filter(|u| g.team_of(u) == 0).collect();
            let sources = frame["ordinarySources"].as_array().unwrap();
            assert_eq!(allied.len(), 2);
            assert_eq!(sources.len(), allied.len());
            for (source, recorded) in allied.iter().zip(sources) {
                assert_eq!(json!(source.id), recorded["source"]);
                assert_eq!(recorded["kind"], "counterfactual-single-source-mask");
                let mut only = g.clone();
                only.units.retain(|id, _| *id == source.id);
                for p in &mut only.players {
                    p.visible.clear();
                }
                let mut declared_input = frame["postVisionInput"].clone();
                declared_input["units"]["$map"].as_array_mut().unwrap()
                    .retain(|entry| entry[0] == json!(source.id));
                for p in declared_input["players"].as_array_mut().unwrap() {
                    p["visible"] = json!({"$set":[]});
                }
                assert_eq!(declared_input, recorded["game"]);
                let ordinary = observe(&only, &rules, &mut Fog::default(), threaded);
                assert_eq!(ordinary, recorded["expected"]);
                assert_eq!(ordinary[0]["vis"][target_cell], 0, "every source excludes target cell");
            }

            let control_record = &frame["control"];
            assert_eq!(control_record["kind"], "counterfactual-remove-only-target-visible-fact");
            assert_eq!(control_record["admittedMatch"], false);
            let mut control = g.clone();
            let mut declared_input = frame["postVisionInput"].clone();
            for (slot, p) in control.players.iter_mut().enumerate() {
                if p.team == 0 {
                    p.visible.retain(|id| *id != target);
                    declared_input["players"][slot]["visible"] = json!({"$set":p.visible});
                }
            }
            assert_eq!(declared_input, control_record["game"], "only target's visible fact removed");
            let absent = observe(&control, &rules, &mut control_fog, threaded);
            assert_eq!(absent, control_record["expected"]);
            assert_cold(&control, &rules, &absent, &control_record["coldExpected"], threaded);
            assert_eq!(absent[0]["vis"][target_cell], 0);
            assert_eq!(actual[0]["vis"][target_cell], 1);
            let changed = changed_cells(&absent[0]["vis"], &actual[0]["vis"]);
            assert_eq!(changed, vec![target_cell]);
            assert_eq!(json!(changed), control_record["changedCells"]);
        }
        for fog in [&actual_fog, &control_fog] {
            for source in [1, 2] {
                assert!(fog.kept[&source].seen.as_ref().unwrap().cells.len() > 0);
            }
        }
    }
}

#[test]
fn cmb131_height_only_edit_invalidates_warmed_cells() {
    let fixture = fixture();
    let rules = Rules::compiled().unwrap();
    let case = &fixture["heightOnly"];
    let observer = case["observer"].as_u64().unwrap() as u32;
    let edit_cell = case["editCell"].as_u64().unwrap() as usize;
    let probe_cell = case["probeCell"].as_u64().unwrap() as usize;
    let frames = case["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 4);
    for threaded in [false, true] {
        let mut fog = Fog::default();
        let mut before = Value::Null;
        let mut prior_game: Option<Game> = None;
        for (index, frame) in frames.iter().enumerate() {
            let g = advance(frame, &rules, threaded);
            assert_eq!(g.units.len(), 1);
            assert!(g.smokes.is_empty() && g.strikes.is_empty());
            assert_height_sight_domain(&g, &rules, case);
            let actual = observe(&g, &rules, &mut fog, threaded);
            assert_eq!(actual, frame["expected"]);
            assert_cold(&g, &rules, &actual, &frame["coldExpected"], threaded);
            if index == 1 || index == 2 {
                assert_eq!(actual, before, "stable scene retains masks, versions and deltas");
                assert!(fog.kept[&observer].seen.as_ref().unwrap().cells.len() > 0);
            }
            if index == 3 {
                let previous = prior_game.as_ref().unwrap();
                let mut restored = frame["game"].clone();
                let prior_input = &frames[2]["postVisionInput"];
                for key in ["height", "tick", "visionTick", "terrainVersion"] {
                    restored[key] = prior_input[key].clone();
                }
                assert_eq!(restored, *prior_input, "only declared height and clocks/version change");
                let old_height = previous.height.as_ref().unwrap();
                let new_height = g.height.as_ref().unwrap();
                let edits: Vec<_> = old_height.iter().zip(new_height).enumerate()
                    .filter_map(|(cell, (a, b))| (a != b).then_some(cell)).collect();
                assert_eq!(edits, vec![edit_cell]);
                assert_eq!(json!(old_height[edit_cell]), case["oldHeight"]);
                assert_eq!(json!(new_height[edit_cell]), case["newHeight"]);
                assert_eq!(g.tick, previous.tick + 4);
                assert_eq!(g.terrain_version, previous.terrain_version + 1);
                assert_eq!(before[0]["vis"][probe_cell], 1);
                assert_eq!(actual[0]["vis"][probe_cell], 0);
                let changed = changed_cells(&before[0]["vis"], &actual[0]["vis"]);
                assert!(changed.contains(&probe_cell));
                assert_eq!(json!(changed), case["changedCells"]);
                let version = before[0]["version"].as_u64().unwrap();
                assert_eq!(actual[0]["version"].as_u64().unwrap(), version + 1);
                assert!(actual[0]["deltas"].as_array().unwrap().iter().any(|delta| {
                    delta[0].as_u64() == Some(version) && !delta[1].as_str().unwrap().is_empty()
                }));
            }
            before = actual;
            prior_game = Some(g);
        }
    }
}
