//! Replays the operation log `harness/grid-fixture.mjs` recorded from `shared/grid.js` at the pin and
//! checks every answer, ids and their order, against the JavaScript's.

use std::collections::BTreeMap;

use ww2_core::grid::{Placed, SpatialGrid, UnitId};

struct TestUnit {
    x: f64,
    z: f64,
    owner: i32,
    heavy: bool,
}

impl Placed for TestUnit {
    fn x(&self) -> f64 {
        self.x
    }
    fn z(&self) -> f64 {
        self.z
    }
    fn owner(&self) -> i32 {
        self.owner
    }
    fn heavy(&self) -> bool {
        self.heavy
    }
}

fn fnv(ids: &[UnitId]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for id in ids {
        for byte in id.to_le_bytes() {
            h ^= u64::from(byte);
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    format!("{h:016x}")
}

fn pred(name: &str) -> impl Fn(UnitId, &TestUnit) -> bool {
    let name = name.to_owned();
    move |id, u| match name.as_str() {
        "all" => true,
        "odd" => id % 2 == 1,
        "owner0" => u.owner == 0,
        "owner2" => u.owner == 2,
        other => panic!("unknown predicate {other}"),
    }
}

#[test]
fn grid_answers_match_the_javascript() {
    let log = include_str!("fixtures/grid.txt");
    let mut units: BTreeMap<UnitId, TestUnit> = BTreeMap::new();
    let mut grid: Option<SpatialGrid> = None;
    let mut queries = 0;
    for (n, line) in log.lines().enumerate().filter(|(_, l)| !l.starts_with('#')) {
        let f: Vec<&str> = line.split(' ').collect();
        let num = |i: usize| f[i].parse::<f64>().unwrap();
        let id = |i: usize| f[i].parse::<UnitId>().unwrap();
        let grid_ref = || grid.as_ref().expect("grid built");
        match f[0] {
            "add" => {
                units.insert(
                    id(1),
                    TestUnit {
                        x: num(2),
                        z: num(3),
                        owner: f[4].parse().unwrap(),
                        heavy: f[5] == "1",
                    },
                );
            }
            "new" => grid = Some(SpatialGrid::new(&units, num(1))),
            "move" | "teleport" => {
                let u = units.get_mut(&id(1)).unwrap();
                (u.x, u.z) = (num(2), num(3));
                if f[0] == "move" {
                    grid.as_mut().unwrap().update(id(1), &units[&id(1)]);
                }
            }
            "update" => grid.as_mut().unwrap().update(id(1), &units[&id(1)]),
            "del" => {
                units.remove(&id(1));
            }
            "rebuild" => grid.as_mut().unwrap().rebuild(&units),
            "q" => {
                queries += 1;
                let g = grid_ref();
                let context = format!("line {}: {line}", n + 1);
                if f[1] == "ownedBy" {
                    let ids = g.owned_by(&units, f[2].parse().unwrap());
                    assert_eq!(
                        (ids.len().to_string(), fnv(&ids)),
                        (f[3].to_owned(), f[4].to_owned()),
                        "{context}"
                    );
                    continue;
                }
                let (at, radius, test) = ((num(2), num(3)), num(4), pred(f[5]));
                match f[1] {
                    "has" | "hasHeavy" => {
                        let found = if f[1] == "has" {
                            g.has(&units, at, radius, test)
                        } else {
                            g.has_heavy(&units, at, radius, test)
                        };
                        assert_eq!(if found { "1" } else { "0" }, f[6], "{context}");
                    }
                    "nearest" => {
                        let found = g.nearest(&units, at, radius, test).unwrap_or(0);
                        assert_eq!(found.to_string(), f[6], "{context}");
                    }
                    kind => {
                        let ids = match kind {
                            "inOrder" => g.in_order(&units, at, radius, test),
                            "unordered" => g.unordered(&units, at, radius, test),
                            "heavy" => g.heavy(&units, at, radius, test),
                            "radius" => g.radius(&units, at, radius, test),
                            other => panic!("unknown query {other}"),
                        };
                        assert_eq!(
                            (ids.len().to_string(), fnv(&ids)),
                            (f[6].to_owned(), f[7].to_owned()),
                            "{context}"
                        );
                    }
                }
            }
            other => panic!("unknown operation {other} on line {}", n + 1),
        }
    }
    assert!(
        queries > 500,
        "the log should exercise the queries ({queries})"
    );
}
