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
