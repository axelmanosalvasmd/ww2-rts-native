//! Pure queries for explored World territory, matching the prototype's run order.
use std::collections::BTreeMap;

use crate::game::Point;

/// Runs and returned segments use cells, not world metres. Input order is retained.
pub fn territory_edges(runs: &[[usize; 3]]) -> Vec<[usize; 4]> {
    let mut rows: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
    for &[y, a, b] in runs {
        rows.entry(y).or_default().push((a, b));
    }
    let mut edges = Vec::new();
    for &[y, a, b] in runs {
        edges.extend([[a, y, a, y + 1], [b, y, b, y + 1]]);
        for (neighbor, line) in [(y.checked_sub(1), y), (y.checked_add(1), y + 1)] {
            let mut x = a;
            if let Some(intervals) = neighbor.and_then(|row| rows.get(&row)) {
                for &(lo, hi) in intervals {
                    if hi <= x || lo >= b {
                        continue;
                    }
                    if lo > x {
                        edges.push([x, line, lo.min(b), line]);
                    }
                    x = x.max(hi);
                    if x >= b {
                        break;
                    }
                }
            }
            if x < b {
                edges.push([x, line, b, line]);
            }
        }
    }
    edges
}

/// Present runs, including an empty list, override legacy bounds in metres.
pub fn inside_known_region(
    runs: Option<&[[usize; 3]]>,
    bounds: Option<&[f64; 4]>,
    at: Point,
    cell: f64,
) -> bool {
    if let Some(runs) = runs {
        let (x, y) = ((at.x / cell).floor(), (at.z / cell).floor());
        return runs
            .iter()
            .any(|&[row, a, b]| y == row as f64 && x >= a as f64 && x < b as f64);
    }
    bounds.is_some_and(|b| at.x >= b[0] && at.z >= b[1] && at.x < b[2] && at.z < b[3])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map60_territory_edges_match_prototype_order_and_remove_seams() {
        // Exact values from shared/world-territories.js at reference e4e579a.
        assert!(territory_edges(&[]).is_empty());
        assert_eq!(
            territory_edges(&[[2, 3, 5], [3, 3, 5]]),
            vec![
                [3, 2, 3, 3],
                [5, 2, 5, 3],
                [3, 2, 5, 2],
                [3, 3, 3, 4],
                [5, 3, 5, 4],
                [3, 4, 5, 4],
            ]
        );
        assert_eq!(
            territory_edges(&[[2, 2, 8], [1, 3, 4], [1, 6, 7], [3, 1, 3], [3, 5, 9]]),
            vec![
                [2, 2, 2, 3],
                [8, 2, 8, 3],
                [2, 2, 3, 2],
                [4, 2, 6, 2],
                [7, 2, 8, 2],
                [3, 3, 5, 3],
                [3, 1, 3, 2],
                [4, 1, 4, 2],
                [3, 1, 4, 1],
                [6, 1, 6, 2],
                [7, 1, 7, 2],
                [6, 1, 7, 1],
                [1, 3, 1, 4],
                [3, 3, 3, 4],
                [1, 3, 2, 3],
                [1, 4, 3, 4],
                [5, 3, 5, 4],
                [9, 3, 9, 4],
                [8, 3, 9, 3],
                [5, 4, 9, 4],
            ]
        );
        assert_eq!(
            territory_edges(&[[4, 2, 3], [2, 7, 9]]),
            vec![
                [2, 4, 2, 5],
                [3, 4, 3, 5],
                [2, 4, 3, 4],
                [2, 5, 3, 5],
                [7, 2, 7, 3],
                [9, 2, 9, 3],
                [7, 2, 9, 2],
                [7, 3, 9, 3],
            ]
        );
    }

    #[test]
    fn map58_known_region_matches_prototype_runs_and_legacy_bounds() {
        let runs = [[2, 3, 5], [3, 3, 5]];
        let bounds = [0.0, 0.0, 30.0, 30.0];
        for ((x, z), expected) in [
            ((6.0, 4.0), true),
            ((9.999, 7.999), true),
            ((10.0, 5.0), false),
            ((7.0, 8.0), false),
            ((-0.1, 5.0), false),
            ((0.0, 0.0), false),
        ] {
            assert_eq!(
                inside_known_region(Some(&runs), Some(&bounds), Point { x, z }, 2.0),
                expected
            );
        }
        let mut known = crate::world_memory::KnownRegion {
            id: 31,
            name: None,
            kind: None,
            team: 7,
            at: Point { x: 7.0, z: 5.0 },
            bounds: None,
            runs: runs.to_vec(),
            progress: 0.0,
            capper: None,
            locked: false,
            contested: false,
        };
        assert!(known.contains(known.at));
        known.runs.clear();
        assert!(!known.contains(known.at));
        known.bounds = Some(bounds);
        assert!(known.contains(known.at));
        let at = Point { x: 7.0, z: 5.0 };
        assert!(!inside_known_region(Some(&[]), Some(&bounds), at, 2.0));
        assert!(inside_known_region(None, Some(&bounds), at, 2.0));
        assert!(!inside_known_region(None, None, at, 2.0));
        assert!(!inside_known_region(
            None,
            Some(&bounds),
            Point { x: 30.0, z: 5.0 },
            2.0
        ));
        assert!(inside_known_region(
            Some(&runs),
            None,
            Point { x: 3.0, z: 2.0 },
            1.0
        ));
    }
}
