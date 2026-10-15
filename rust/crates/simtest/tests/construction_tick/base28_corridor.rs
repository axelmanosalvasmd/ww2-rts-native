//! Paired corridor component, using natural Conquest actors and live terrain publication.
use super::*;
use std::io::Read;
use ww2_core::{
    game::{MOVE, SIGHT, Unit},
    navigation::{TerrainView, Versions},
    pathfinding::{PathRules, PathTerrain, Pathfinding},
};

const CELLS: [u32; 9] = [339, 340, 341, 379, 380, 381, 419, 420, 421];

fn local_cells() -> Vec<usize> {
    (5..14)
        .flat_map(|y| (16..25).map(move |x| y * 40 + x))
        .collect()
}

fn terrain_cell(m: &Match, c: usize) -> Value {
    let g = &m.world.game;
    json!({"c":c,"ch":char::from(g.chars[c]).to_string(),
        "ground":char::from(g.ground.as_ref().unwrap()[c]).to_string(),
        "object":char::from(g.objects.as_ref().unwrap()[c]).to_string(),
        "flags":g.flags[c],"level":g.height.as_ref().map_or(0, |h| h[c]),
        "hp32":g.cell_hp[c].to_bits(),"wear32":g.wear.as_ref().unwrap()[c].to_bits(),
        "scar32":m.world.scar[c].to_bits(),"state":m.world.cell_state[c],
        "burnt":m.world.burnt[c]})
}

fn query(m: &Match, paths: &mut Pathfinding) -> Vec<Point> {
    let g = &m.world.game;
    let mut near = vec![0; g.flags.len()];
    for (c, &flags) in g.flags.iter().enumerate() {
        if flags & (MOVE | SIGHT) != (MOVE | SIGHT) {
            continue;
        }
        let x = c % g.w;
        for dy in [-1_i64, 0, 1] {
            for dx in (if x > 0 { -1_i64 } else { 0 })..=(if x < g.w - 1 { 1 } else { 0 })
            {
                let n = i64::try_from(c).unwrap() + dy * i64::try_from(g.w).unwrap() + dx;
                if let Ok(n) = usize::try_from(n)
                    && n < near.len()
                {
                    near[n] = 1;
                }
            }
        }
    }
    assert!(m.world.mines.is_empty());
    let mines = vec![false; g.flags.len()];
    let fires = (0..g.flags.len())
        .map(|c| m.world.fires.contains_key(&c))
        .collect::<Vec<_>>();
    let bunkers = g
        .units
        .values()
        .filter(|u| m.rules.kinds[u.kind].name == "bunker" && u.hp > 0.0)
        .map(Unit::at)
        .collect::<Vec<_>>();
    let versions = m.world.versions;
    // The original untyped, ownerless query has the ordinary infantry MOVE mask.
    // Keep one cache and read every terrain/version input from the actual live World.
    let terrain = PathTerrain {
        navigation: TerrainView {
            width: g.w,
            height: g.h,
            flags: &g.flags,
            heights: g.height.as_deref(),
            world_known: false,
            knowledge_id: 0,
            versions: Versions {
                infantry: versions.infantry,
                vehicle: versions.vehicle,
                naval: versions.naval,
                obstruction: versions.obstruction,
                knowledge: 0,
            },
            changed: None,
            changes: &[],
        },
        near_walls: &near,
        known_mines: &mines,
        fires: &fires,
        bunkers: &bunkers,
        wear: g.wear.as_deref(),
        wetness: g.wet.unwrap_or(0.0),
        roads: m.world.roads,
    };
    paths
        .find_path(
            terrain,
            PathRules::compiled().unwrap(),
            &m.rules.kinds[m.rules.kind("rifle").unwrap()],
            Point { x: 5.0, z: 19.0 },
            Point { x: 75.0, z: 19.0 },
            None,
        )
        .unwrap()
        .waypoints
}

fn measured(
    m: &Match,
    label: &str,
    site: Option<&Unit>,
    debit: Option<f64>,
    route: Option<&[Point]>,
) -> Value {
    let site = site.map(|saved| m.world.game.units.get(&saved.id).unwrap_or(saved));
    let sections = site
        .map(|u| {
            u.cells
                .as_ref()
                .unwrap()
                .iter()
                .map(|&cell| {
                    let c = usize::try_from(cell).unwrap();
                    let state = &m.world.game.sections[&cell];
                    let meta = &m.world.sections[&c];
                    let structure = m
                        .world
                        .structures
                        .iter()
                        .find(|s| s.id == meta.structure_id)
                        .unwrap();
                    assert_eq!(structure.entity_id, Some(u.id));
                    let appearance = if state.failed {
                        "failed"
                    } else {
                        match meta.appearance {
                            Appearance::Intact => "intact",
                            Appearance::Damaged => "damaged",
                            Appearance::Burning => "burning",
                        }
                    };
                    json!({"c":c,"state":appearance,"hp":bits(state.hp),
                        "maxHp":bits(state.max_hp.unwrap()),"material":meta.material,
                        "failedAt":meta.failure.failed_at.map(bits),
                        "settledAt":meta.failure.settled_at.map(bits),"rubble":meta.rubble})
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({"label":label,"tick":m.world.game.tick,
        "route":route.map(|rows|rows.iter().copied().map(point).collect::<Vec<_>>()),
        "debit":debit.map(bits),
        "site":site.map(|u|json!({"type":m.rules.kinds[u.kind].name,
            "owner":u.owner,"hp":bits(u.hp),"built":bits(u.built.unwrap()),
            "cells":u.cells,"at":point(u.at()),"live":m.world.game.units.contains_key(&u.id),
            "cost":bits(150.0),"maxHp":bits(1500.0)})),
        "terrain":local_cells().into_iter().map(|c|terrain_cell(m,c)).collect::<Vec<_>>(),
        "sections":sections})
}

fn witness(m: &Match, label: &str) -> Value {
    let g = &m.world.game;
    let v = m.world.versions;
    let actors = g
        .units
        .values()
        .map(|u| {
            let structure = m
                .world
                .structures
                .iter()
                .find(|s| s.entity_id == Some(u.id))
                .map(|s| &s.id);
            json!({"id":u.id,"type":m.rules.kinds[u.kind].name,"owner":u.owner,
                "hp":bits(u.hp),"at":point(u.at()),"cells":u.cells,
                "built":u.built.map(bits),"structureId":structure})
        })
        .collect::<Vec<_>>();
    json!({"label":label,"tick":g.tick,"seed":m.start.environment_seed,
        "versions":[u64::from(g.terrain_version),v.infantry,v.vehicle,v.naval,v.obstruction],
        "wallet":m.start.economy.players.iter().map(|p|bits(p.mp)).collect::<Vec<_>>(),
        "actors":actors})
}

#[test]
fn base28_paid_stamp_and_settled_collapse_reopen_warmed_corridor() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/base28-corridor.json.gz");
    let mut bytes = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
        .read_to_string(&mut bytes)
        .unwrap();
    let f: Value = serde_json::from_str(&bytes).unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["original"]["measured"], f["amended"]["measured"]);
    assert!(f["original"]["validation"].is_string());
    assert!(f["amended"]["validation"].is_null());
    assert_eq!(f["original"]["steps"], 54);
    assert_eq!(f["amended"]["steps"], 54);
    assert_eq!(f["projection"]["settlementSteps"], 52);
    assert_eq!(f["projection"]["expectedCells"], json!(CELLS));
    assert_eq!(f["projection"]["localCells"], json!(local_cells()));
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&f["amended"]["map"]).unwrap())
        .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Default,
            players: vec![
                PlayerConfig {
                    name: "a".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "b".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: true,
            map_key: None,
            tech: false,
            supply: true,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    assert_eq!(
        json!(m.world.game.units.keys().copied().collect::<Vec<_>>()),
        f["amended"]["initialOrder"]
    );
    let builder = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    assert_eq!(json!(builder), f["amended"]["builder"]);
    let retained = m
        .world
        .game
        .units
        .values()
        .map(|u| (u.id, u.hp, u.cells.clone()))
        .collect::<Vec<_>>();
    m.start.economy.players[0].mp = 5000.0;
    let crew = m.world.game.units.get_mut(&builder).unwrap();
    crew.x = 37.0;
    crew.z = 19.0;
    m.initialize_tick_scene(0, TickPathConfig::default()).unwrap();
    let expected = f["amended"]["measured"].as_array().unwrap();
    let witnesses = f["amended"]["witness"].as_array().unwrap();
    assert_eq!(expected.len(), 61);
    assert_eq!(witnesses.len(), expected.len());
    let mut paths = Pathfinding::new();
    let mut site: Option<Unit> = None;
    let mut debit = None;
    let mut ticks = 0;
    for (index, frame) in expected.iter().enumerate() {
        let label = frame["label"].as_str().unwrap();
        let mut route = None;
        match label {
            "initial" => {}
            "open" | "closed" | "reopened" => {
                let found = query(&m, &mut paths);
                assert_eq!(found.is_empty(), label == "closed", "{label}");
                route = Some(found);
            }
            "preparation" | "destroyed" => {
                m.advance_tick().unwrap();
                ticks += 1;
            }
            "stamp" => {
                let before = m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
                let wallet = m.start.economy.players[0].mp;
                let cmd = Command::from_json(&json!({"t":"build","ids":[builder],
                    "kind":"barracks","x":41,"z":19}))
                .unwrap();
                assert_eq!(m.execute_command(0, &cmd, false).unwrap(), None);
                let born = m
                    .world
                    .game
                    .units
                    .values()
                    .filter(|u| !before.contains(&u.id))
                    .collect::<Vec<_>>();
                assert_eq!(born.len(), 1);
                let u = born[0];
                assert_eq!(json!(u.id), f["amended"]["newborn"]);
                assert_eq!(m.rules.kinds[u.kind].name, "barracks");
                assert_eq!(u.cells.as_deref(), Some(CELLS.as_slice()));
                assert_eq!((u.x, u.z, u.hp, u.built), (41.0, 19.0, 375.0, Some(0.0)));
                for &cell in &CELLS {
                    let c = usize::try_from(cell).unwrap();
                    assert_eq!(m.building_cells[&c].unit, u.id);
                    assert_eq!(m.world.game.chars[c], b'K');
                }
                site = Some(u.clone());
                let paid = wallet - m.start.economy.players[0].mp;
                assert_eq!(paid, 150.0);
                debit = Some(paid);
            }
            "stop" => {
                let cmd = Command::from_json(&json!({"t":"stop","ids":[builder]})).unwrap();
                assert_eq!(m.execute_command(0, &cmd, false).unwrap(), None);
            }
            "destroy-input" => {
                let saved = site.as_mut().unwrap();
                saved.hp = 0.0;
                m.world.game.units.get_mut(&saved.id).unwrap().hp = 0.0;
            }
            label if label.starts_with("settle:") => {
                m.advance_tick().unwrap();
                ticks += 1;
            }
            other => panic!("unrepresented corridor action {other}"),
        }
        for (id, hp, cells) in &retained {
            let u = &m.world.game.units[id];
            assert_eq!((u.hp, &u.cells), (*hp, cells));
            for &cell in cells.as_deref().unwrap_or_default() {
                assert_eq!(m.world.game.chars[usize::try_from(cell).unwrap()], b'K');
            }
        }
        assert_eq!(m.start.points.len(), 1);
        let p = &m.start.points[0];
        assert_eq!((p.owner, p.capper), (None, None));
        assert_eq!((p.progress, p.mp, p.vp, p.cut), (0.0, 0.0, 0.0, false));
        assert_eq!(
            measured(&m, label, site.as_ref(), debit, route.as_deref()),
            *frame,
            "complete measured corridor projection: {label}"
        );
        assert_eq!(witness(&m, label), witnesses[index], "{label}: native caller witness");
    }
    let site = site.unwrap();
    assert!(!m.world.game.units.contains_key(&site.id));
    let mut rubble = BTreeSet::new();
    for &cell in &CELLS {
        let c = usize::try_from(cell).unwrap();
        assert!(m.world.game.sections[&cell].failed);
        let meta = &m.world.sections[&c];
        assert!(meta.failure.settled_at.is_some_and(f64::is_finite));
        rubble.extend(meta.rubble.iter().copied());
    }
    assert!(!rubble.is_empty());
    for c in rubble {
        assert_eq!(m.world.game.objects.as_ref().unwrap()[c], b'R');
        assert!(CELLS.iter().any(|&origin| {
            let origin = usize::try_from(origin).unwrap();
            let dx = (c % 40).abs_diff(origin % 40) as f64;
            let dz = (c / 40).abs_diff(origin / 40) as f64;
            ww2_core::math::hypot(dx, dz) <= 3.0
        }));
    }
    assert_eq!(ticks, 54);
}
