//! Base geography against source fingerprints and independently walked invariants.
use serde_json::{Value, json};
use std::collections::BTreeSet;
use ww2_core::{
    math,
    random::{MatchRandom, StreamId},
    world_generation::{self, GeneratedGeography, GenerationError, WorldSize},
};
fn hash(bytes: impl IntoIterator<Item = u8>) -> String {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in bytes {
        h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
    }
    format!("{h:016x}")
}
fn metadata(g: &GeneratedGeography) -> Value {
    let regions: Vec<_> = g.regions.iter().map(|r| {
        let mut value = json!({"id":r.id,"kind":r.kind.name(),"x":r.x,"y":r.y,"bounds":r.bounds,"name":r.name});
        if let Some(home) = r.home { value["home"] = json!(home); }
        if let Some(team) = r.team { value["team"] = json!(team); }
        value
    }).collect();
    json!({"regions":regions,"spawns":g.spawns.iter().map(|p|json!({"x":p.x,"y":p.y})).collect::<Vec<_>>(),
    "channels":g.waterways.rivers.iter().map(|r|json!({"id":r.id,"parent":r.parent,"pathHash":hash(r.path.iter().flat_map(|p|[p.x as u32,p.y as u32,p.radius as u32]).flat_map(u32::to_le_bytes))})).collect::<Vec<_>>(),
    "crossings":g.waterways.crossings.iter().map(|c|json!({"x":c.x,"y":c.y,"type":char::from(c.kind).to_string(),"river":c.river})).collect::<Vec<_>>(),
    "lake":{"x":g.waterways.lake.x,"y":g.waterways.lake.y},"passes":g.waterways.passes.iter().map(|p|json!({"x":p.x,"y":p.y})).collect::<Vec<_>>()})
}
fn reachable(g: &GeneratedGeography, destroy_bridges: bool, wide: bool) -> Vec<bool> {
    let grid = &g.grid;
    let w = grid.width;
    let h = grid.height;
    let mut safe = vec![true; w * h];
    for y in 0..h {
        for x in 0..w {
            let c = y * w + x;
            safe[c] = if wide {
                x > 0
                    && y > 0
                    && x + 1 < w
                    && y + 1 < h
                    && (y - 1..=y + 1).all(|yy| {
                        (x - 1..=x + 1).all(|xx| !b"WB=RYQ".contains(&grid.chars[yy * w + xx]))
                    })
            } else {
                !matches!(grid.chars[c], b'W' | b'B' | b'Y')
                    && (!destroy_bridges || grid.chars[c] != b'=')
            };
        }
    }
    let first = g.regions[0].y * w + g.regions[0].x;
    let mut seen = vec![false; w * h];
    seen[first] = true;
    let mut queue = vec![first];
    let mut at = 0;
    while at < queue.len() {
        let c = queue[at];
        at += 1;
        let x = c % w;
        let y = c / w;
        for n in [
            (x > 0).then(|| c - 1),
            (x + 1 < w).then_some(c + 1),
            (y > 0).then(|| c - w),
            (y + 1 < h).then_some(c + w),
        ]
        .into_iter()
        .flatten()
        {
            if !seen[n]
                && safe[n]
                && (i16::from(grid.heights[n]) - i16::from(grid.heights[c])).abs() <= 1
            {
                seen[n] = true;
                queue.push(n);
            }
        }
    }
    seen
}
fn invariants(g: &GeneratedGeography, teams: &[i32]) {
    let grid = &g.grid;
    let w = grid.width;
    let h = grid.height;
    assert_eq!(
        (w, h, g.regions.len()),
        match g.size {
            WorldSize::Huge => (512, 512, 64),
            WorldSize::Massive => (1024, 512, 128),
        }
    );
    assert_eq!(g.spawns.len(), teams.len());
    assert_eq!(g.region_map.len(), w * h);
    assert!(grid.heights.contains(&4));
    assert!(grid.chars.contains(&b'='));
    assert!(
        grid.chars
            .chunks(w)
            .any(|r| r.windows(20).any(|v| v.iter().all(|&c| c == b'W')))
    );
    assert!(grid.heights.iter().all(|v| (0..=4).contains(v)));
    let names: BTreeSet<_> = g.regions.iter().map(|r| &r.name).collect();
    assert_eq!(names.len(), g.regions.len());
    for (c, &id) in g.region_map.iter().enumerate() {
        assert!((-1..g.regions.len() as i16).contains(&id));
        if grid.chars[c] == b'W' {
            assert_eq!(id, -1);
        }
    }
    for wide in [false, true] {
        let seen = reachable(g, true, wide);
        for r in &g.regions {
            assert!(
                seen[r.y * w + r.x],
                "seed{} region{} wide{wide}",
                g.terrain_seed,
                r.id
            );
        }
    }
    let main = g
        .waterways
        .rivers
        .iter()
        .filter(|r| r.parent.is_none())
        .count();
    assert!((1..=3).contains(&main));
    assert_eq!(g.waterways.crossings.len(), main * 4);
    for river in &g.waterways.rivers {
        if let Some(parent) = river.parent {
            let end = river.path.last().unwrap();
            let join = &g.waterways.rivers[parent].path[end.y];
            assert_eq!((end.x, end.y), (join.x, join.y));
            assert!(join.radius > g.waterways.rivers[parent].path[0].radius);
        } else {
            assert_eq!(river.path.first().unwrap().y, 0);
            assert_eq!(river.path.last().unwrap().y, h - 1);
            let crossings: Vec<_> = g
                .waterways
                .crossings
                .iter()
                .filter(|c| c.river == river.id)
                .collect();
            assert_eq!(crossings.iter().filter(|c| c.kind == b'F').count(), 2);
            assert_eq!(crossings.iter().filter(|c| c.kind == b'=').count(), 2);
            assert_eq!(
                crossings.iter().map(|c| c.y).collect::<BTreeSet<_>>().len(),
                4
            );
        }
        for (i, p) in river.path.iter().enumerate() {
            assert!(matches!(grid.chars[p.y * w + p.x], b'W' | b'F' | b'='));
            assert_eq!(grid.heights[p.y * w + p.x], 0);
            if i > 0 {
                assert!(p.x.abs_diff(river.path[i - 1].x) <= 1);
                assert_eq!(p.y, river.path[i - 1].y + 1);
            }
        }
    }
    let mut building_sites = Vec::new();
    for (slot, spawn) in g.spawns.iter().enumerate() {
        let home = g.regions.iter().find(|r| r.home == Some(slot)).unwrap();
        assert_eq!(g.region_map[home.y * w + home.x], home.id as i16);
        assert_eq!(home.team, Some(teams[slot]));
        assert_eq!((home.x, home.y), (spawn.x, spawn.y));
        let nearest = g
            .regions
            .iter()
            .filter(|r| r.home.is_none())
            .map(|r| math::hypot(r.x as f64 - home.x as f64, r.y as f64 - home.y as f64))
            .fold(f64::INFINITY, f64::min);
        assert!(
            (50.0..=70.0).contains(&nearest),
            "seed{} home{slot} expansion{nearest}",
            g.terrain_seed
        );
        // Source objective reserve. Count flat, same-region 3x3 footprints nearby,
        // excluding the central 3x3 HQ footprint that World startup would occupy.
        let mut count = 0;
        for dy in -5isize..=5 {
            for dx in -5isize..=5 {
                let x = (home.x as isize + dx) as usize;
                let y = (home.y as isize + dy) as usize;
                let cells = (y..y + 3)
                    .flat_map(|yy| (x..x + 3).map(move |xx| yy * w + xx))
                    .collect::<Vec<_>>();
                let overlaps = (x..x + 3).any(|xx| (home.x - 1..=home.x + 1).contains(&xx))
                    && (y..y + 3).any(|yy| (home.y - 1..=home.y + 1).contains(&yy));
                if !overlaps
                    && cells.iter().all(|&c| {
                        grid.heights[c] == 0
                            && matches!(grid.chars[c], b'.' | b'D' | b'O')
                            && g.region_map[c] == home.id as i16
                    })
                {
                    count += 1;
                }
            }
        }
        assert!(count >= 20);
        building_sites.push(count);
    }
    assert!(building_sites.iter().max().unwrap() - building_sites.iter().min().unwrap() <= 5);
}

#[test]
fn seeded_world_sweep_preserves_geography_and_fair_homes() {
    // The source stress gate's same 80 settings, with bare geography explicitly selected.
    for i in 0u32..80 {
        let size = if i % 2 != 0 {
            WorldSize::Huge
        } else {
            WorldSize::Massive
        };
        let seed = (i + 1).wrapping_mul(2_654_435_761);
        let teams: Vec<_> = (0..1 + i % 6)
            .map(|j| if i % 3 != 0 { j as i32 } else { (j % 2) as i32 })
            .collect();
        let first = world_generation::generate_from_terrain_seed(seed, size, &teams).unwrap();
        invariants(&first, &teams);
        assert_eq!(
            first,
            world_generation::generate_from_terrain_seed(seed, size, &teams).unwrap(),
            "{size:?} seed {seed}"
        );
    }
}
#[test]
fn original_base_geography_and_source_acceptance_invariants() {
    let f: Value = serde_json::from_str(include_str!("fixtures/world-generation.json")).unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    assert_eq!(f["cases"].as_array().unwrap().len(), 10);
    for row in f["cases"].as_array().unwrap() {
        let size = if row["size"] == "huge" {
            WorldSize::Huge
        } else {
            WorldSize::Massive
        };
        let seed = row["seed"].as_u64().unwrap() as u32;
        let teams: Vec<_> = row["teams"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_i64().unwrap() as i32)
            .collect();
        let g = world_generation::generate_from_terrain_seed(seed, size, &teams).unwrap();
        assert_eq!(
            hash(g.grid.chars.iter().copied()),
            row["charsHash"],
            "{size:?} seed{seed} chars"
        );
        assert_eq!(
            hash(g.grid.heights.iter().map(|&n| n as u8)),
            row["heightsHash"],
            "{size:?} seed{seed} heights"
        );
        assert_eq!(
            hash(g.region_map.iter().flat_map(|n| n.to_le_bytes())),
            row["regionMapHash"],
            "{size:?} seed{seed} territories"
        );
        let actual = metadata(&g);
        for key in [
            "regions",
            "spawns",
            "channels",
            "crossings",
            "lake",
            "passes",
        ] {
            assert_eq!(actual[key], row[key], "{size:?} seed{seed} {key}");
        }
        invariants(&g, &teams);
        let input = g.into_world_map_input();
        assert_eq!(hash(input.chars), row["charsHash"]);
        assert_eq!(
            hash(input.heights.unwrap().into_iter().map(|h| h as u8)),
            row["heightsHash"]
        );
        assert_eq!(
            hash(
                input
                    .geography
                    .region_map
                    .unwrap()
                    .into_iter()
                    .flat_map(i16::to_le_bytes)
            ),
            row["regionMapHash"]
        );
        assert_eq!(input.geography.seed, Some(seed));
        assert_eq!(
            input.geography.generator_version,
            Some(world_generation::GENERATOR_VERSION)
        );
        assert_eq!(input.geography.regions.len(), size.region_count());
        assert!(input.layers.is_none() && input.geography.biomes.is_none());
        assert!(!input.geography.metadata.contains_key("rails"));
        let water = &input.geography.metadata["waterways"];
        assert_eq!(water["crossings"], row["crossings"]);
        assert_eq!(water["lake"], row["lake"]);
        for (river, source) in water["rivers"]
            .as_array()
            .unwrap()
            .iter()
            .zip(row["channels"].as_array().unwrap())
        {
            assert_eq!(river["id"], source["id"]);
            assert_eq!(river["parent"], source["parent"]);
            assert_eq!(
                hash(
                    river["path"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .flat_map(|p| ["x", "y", "radius"].map(|k| p[k].as_u64().unwrap() as u32))
                        .flat_map(u32::to_le_bytes)
                ),
                source["pathHash"]
            );
        }
    }
}
#[test]
fn recorded_terrain_seed_isolated_and_invalid_requests_do_not_draw() {
    let mut a = MatchRandom::new(0x3141_5926);
    let mut b = a.clone();
    for _ in 0..32 {
        b.next_u64(StreamId::Combat);
        b.next_u64(StreamId::Ai(3));
    }
    let before = a.snapshot();
    assert_eq!(
        world_generation::generate(&mut a, WorldSize::Huge, &[]),
        Err(GenerationError::Players)
    );
    assert_eq!(a.snapshot(), before);
    assert_eq!(
        world_generation::generate(&mut a, WorldSize::Huge, &[6]),
        Err(GenerationError::Teams)
    );
    assert_eq!(a.snapshot(), before);
    let teams = [0, 1, 2, 3];
    let first = world_generation::generate(&mut a, WorldSize::Huge, &teams).unwrap();
    let mut expected = MatchRandom::restore(before).unwrap();
    assert_eq!(
        first.terrain_seed,
        expected.next_u64(StreamId::Terrain) as u32
    );
    assert_eq!(
        a.snapshot(),
        expected.snapshot(),
        "only one Terrain draw is consumed"
    );
    let second = world_generation::generate(&mut b, WorldSize::Huge, &teams).unwrap();
    assert_eq!(first, second);
    invariants(&first, &teams);
    assert!(first.spawns.iter().all(|p| math::hypot(
        p.x as f64 - first.grid.width as f64 / 2.0,
        p.y as f64 - first.grid.height as f64 / 2.0
    ) > first.grid.height as f64 * 0.25));
    assert_eq!(
        first,
        world_generation::generate_from_terrain_seed(first.terrain_seed, WorldSize::Huge, &teams)
            .unwrap()
    );
}

fn full_metadata(world: &world_generation::GeneratedWorld) -> Value {
    let mut actual = metadata(&world.geography);
    if let Some(scenery) = &world.scenery {
        for (r, biome) in actual["regions"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .zip(&scenery.regions)
        {
            r["biome"] = json!(biome.name());
        }
        actual["biomesHash"] = json!(hash(scenery.cells.iter().copied()));
        actual["buildings"] = json!(
            scenery
                .buildings
                .iter()
                .map(|b| json!({"x":b.x,"y":b.y,"kind":b.kind}))
                .collect::<Vec<_>>()
        );
    } else {
        actual["biomesHash"] = Value::Null;
        actual["buildings"] = Value::Null;
    }
    actual["rails"] = json!({"stations":world.rails.stations.iter().map(|s|json!({"region":s.region,"x":s.x,"y":s.y})).collect::<Vec<_>>(),
        "lines":world.rails.lines.iter().map(|l|json!({"a":l.a,"b":l.b,"regions":l.regions,"length":l.cells.len(),"cellsHash":hash(l.cells.iter().flat_map(|&c|(c as u32).to_le_bytes()))})).collect::<Vec<_>>()});
    actual
}
fn full_invariants(
    world: &world_generation::GeneratedWorld,
    bare: &GeneratedGeography,
    teams: &[i32],
) {
    use world_generation::{Biome, RegionKind};
    let g = &world.geography;
    invariants(g, teams);
    assert_eq!(g.regions, bare.regions);
    assert_eq!(g.spawns, bare.spawns);
    assert_eq!(g.region_map, bare.region_map);
    assert_eq!(g.waterways, bare.waterways);
    assert_eq!(g.road_mask, bare.road_mask);
    assert_eq!(g.grid.heights, bare.grid.heights);
    let w = g.grid.width;
    for (c, &ch) in g.grid.chars.iter().enumerate() {
        if ch != bare.grid.chars[c] {
            assert!(matches!(bare.grid.chars[c], b'.' | b'O'));
            assert!(matches!(ch, b'B' | b'R' | b'D' | b'H' | b'O' | b'M'));
            if g.road_mask[c] != 0 {
                // Source city squares pave open road verges too; building lots still avoid them.
                assert_eq!((bare.grid.chars[c], ch), (b'.', b'D'));
                let (x, y) = (c % w, c / w);
                assert!(g.regions.iter().any(|r| r.kind == RegionKind::City
                    && x.abs_diff(r.x) <= 7
                    && y.abs_diff(r.y) <= 7
                    && x.abs_diff(r.x) + y.abs_diff(r.y) <= 12));
            }
        }
    }
    for wide in [false, true] {
        let seen = reachable(g, true, wide);
        assert!(
            g.regions.iter().all(|r| seen[r.y * w + r.x]),
            "dressed seed {} disconnected",
            g.terrain_seed
        );
    }
    if let Some(scenery) = &world.scenery {
        assert_eq!(scenery.regions.len(), g.regions.len());
        assert_eq!(scenery.cells.len(), w * g.grid.height);
        for (r, &biome) in g.regions.iter().zip(&scenery.regions) {
            if r.home.is_some() {
                assert_ne!(biome, Biome::Marsh);
            }
        }
        for (c, &b) in scenery.cells.iter().enumerate() {
            assert!(b < 6 || b == 255);
            assert_eq!(b == 255, g.region_map[c] < 0);
        }
        for b in &scenery.buildings {
            assert!(matches!(
                b.kind.as_str(),
                "church" | "factory" | "mine head"
            ));
            assert_eq!(g.grid.chars[b.y * w + b.x], b'B');
        }
    } else {
        assert_eq!(g, bare);
    }
    let mut stations = BTreeSet::new();
    for s in &world.rails.stations {
        assert!(stations.insert(s.region));
        let r = &g.regions[s.region];
        assert!(r.home.is_some() || matches!(r.kind, RegionKind::City | RegionKind::Industrial));
        assert_eq!(g.region_map[s.y * w + s.x], s.region as i16);
        let level = g.grid.heights[s.y * w + s.x];
        for y in s.y - 2..=s.y + 2 {
            for x in s.x - 2..=s.x + 2 {
                assert!(matches!(g.grid.chars[y * w + x], b'.' | b'O' | b'D'));
                assert_eq!(g.grid.heights[y * w + x], level);
            }
        }
    }
    for l in &world.rails.lines {
        assert!(stations.contains(&l.a) && stations.contains(&l.b));
        assert!(!l.cells.is_empty());
        assert!(l.cells.iter().all(|&c| c < w * g.grid.height));
        let a = world
            .rails
            .stations
            .iter()
            .find(|s| s.region == l.a)
            .unwrap();
        let b = world
            .rails
            .stations
            .iter()
            .find(|s| s.region == l.b)
            .unwrap();
        assert_eq!(l.cells.first(), Some(&(a.y * w + a.x)));
        assert_eq!(l.cells.last(), Some(&(b.y * w + b.x)));
        for pair in l.cells.windows(2) {
            assert_ne!(pair[0], pair[1]);
            assert!((pair[0] % w).abs_diff(pair[1] % w) <= 1);
            assert!((pair[0] / w).abs_diff(pair[1] / w) <= 1);
        }
        let mut known = BTreeSet::new();
        let order: Vec<_> = l
            .cells
            .iter()
            .filter_map(|&c| {
                let id = g.region_map[c];
                (id >= 0 && known.insert(id)).then_some(id as usize)
            })
            .collect();
        assert_eq!(l.regions, order);
    }
}
#[test]
fn original_dressed_geography_and_ordered_rail_paths() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/world-scenery.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 20);
    for row in fixture["cases"].as_array().unwrap() {
        let size = if row["size"] == "huge" {
            WorldSize::Huge
        } else {
            WorldSize::Massive
        };
        let seed = row["seed"].as_u64().unwrap() as u32;
        let teams: Vec<_> = row["teams"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap() as i32)
            .collect();
        let dressed = row["scenery"].as_bool().unwrap();
        let world = world_generation::generate_world_from_terrain_seed(seed, size, &teams, dressed)
            .unwrap();
        assert_eq!(
            hash(world.geography.grid.chars.iter().copied()),
            row["charsHash"],
            "{size:?}/{seed} scenery{dressed} characters"
        );
        assert_eq!(
            hash(world.geography.grid.heights.iter().map(|&h| h as u8)),
            row["heightsHash"]
        );
        assert_eq!(
            hash(
                world
                    .geography
                    .region_map
                    .iter()
                    .flat_map(|n| n.to_le_bytes())
            ),
            row["regionMapHash"]
        );
        let actual = full_metadata(&world);
        for key in ["regions", "spawns", "buildings", "biomesHash", "rails"] {
            assert_eq!(
                actual[key], row[key],
                "{size:?}/{seed} scenery{dressed} {key}"
            );
        }
        let bare = world_generation::generate_from_terrain_seed(seed, size, &teams).unwrap();
        full_invariants(&world, &bare, &teams);
        let input = world.into_world_map_input();
        assert_eq!(hash(input.chars.iter().copied()), row["charsHash"]);
        let loaded = ww2_core::map::AuthoritativeMap::from_world(input.clone()).unwrap();
        let live = ww2_core::world::World::from_map(&loaded).unwrap();
        assert_eq!(live.geography(), loaded.geography());
        assert!(live.game.units.is_empty() && live.game.players.is_empty());
        assert_eq!(
            hash(live.game.flags.iter().flat_map(|f| f.to_le_bytes())),
            row["terrain"]["flagsHash"]
        );
        assert_eq!(
            hash(live.game.house.iter().copied()),
            row["terrain"]["houseHash"]
        );
        assert_eq!(
            hash(
                live.game
                    .cell_hp
                    .iter()
                    .flat_map(|f| f.to_bits().to_le_bytes())
            ),
            row["terrain"]["cellHpHash"]
        );
        assert_eq!(
            hash(
                live.game
                    .wear
                    .as_ref()
                    .unwrap()
                    .iter()
                    .flat_map(|f| f.to_bits().to_le_bytes())
            ),
            row["terrain"]["wearHash"]
        );
        assert_eq!(
            input.geography.generator_version,
            Some(world_generation::WORLD_GENERATOR_VERSION)
        );
        assert_eq!(
            input
                .geography
                .biomes
                .map(hash)
                .map_or(Value::Null, |s| json!(s)),
            row["biomesHash"]
        );
        assert_eq!(
            json!(
                input
                    .buildings
                    .iter()
                    .map(|b| json!({"x":b.x,"y":b.y,"kind":b.kind}))
                    .collect::<Vec<_>>()
            ),
            row.get("buildings").cloned().unwrap_or_else(|| json!([]))
        );
        let hints: Vec<_> = input
            .geography
            .regions
            .iter()
            .map(|r| r.biome.clone())
            .collect();
        let expected: Vec<_> = row["regions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.get("biome").map(|b| b.as_str().unwrap().to_owned()))
            .collect();
        assert_eq!(hints, expected);
        assert_eq!(
            input.geography.metadata["rails"]["stations"],
            row["rails"]["stations"]
        );
        let lines = input.geography.metadata["rails"]["lines"]
            .as_array()
            .unwrap();
        for (line, expected) in lines.iter().zip(row["rails"]["lines"].as_array().unwrap()) {
            assert_eq!(line["a"], expected["a"]);
            assert_eq!(line["b"], expected["b"]);
            assert_eq!(line["regions"], expected["regions"]);
            assert_eq!(
                hash(
                    line["cells"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .flat_map(|c| (c.as_u64().unwrap() as u32).to_le_bytes())
                ),
                expected["cellsHash"]
            );
        }
    }
}
#[test]
fn scenery_isolated_across_seeded_sizes_and_seats() {
    for i in 0u32..80 {
        let seed = (i + 1).wrapping_mul(2654435761);
        let size = if i % 2 == 0 {
            WorldSize::Massive
        } else {
            WorldSize::Huge
        };
        let teams: Vec<_> = (0..1 + i % 6)
            .map(|j| if i % 3 == 0 { (j % 2) as i32 } else { j as i32 })
            .collect();
        let bare = world_generation::generate_from_terrain_seed(seed, size, &teams).unwrap();
        let a =
            world_generation::generate_world_from_terrain_seed(seed, size, &teams, true).unwrap();
        let b =
            world_generation::generate_world_from_terrain_seed(seed, size, &teams, true).unwrap();
        assert_eq!(a, b);
        full_invariants(&a, &bare, &teams);
    }
    let mut random = MatchRandom::new(20261003);
    let mut expected = random.clone();
    let seed = expected.next_u64(StreamId::Terrain) as u32;
    let world =
        world_generation::generate_world(&mut random, WorldSize::Huge, &[0, 1], true).unwrap();
    assert_eq!(random.snapshot(), expected.snapshot());
    assert_eq!(
        world,
        world_generation::generate_world_from_terrain_seed(seed, WorldSize::Huge, &[0, 1], true)
            .unwrap()
    );
}
