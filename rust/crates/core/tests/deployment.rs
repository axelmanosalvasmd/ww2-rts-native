//! Expected values come from original pinned JavaScript helpers with supplied randomness.
use serde_json::Value;
use ww2_core::{
    deployment::{self, Error, LayoutSelection, Spawn, TerrainView},
    economy::Mode,
};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/deployment.json")).unwrap()
}
fn array(v: &Value) -> &[Value] {
    v.as_array().unwrap()
}
fn number(v: &Value) -> usize {
    v.as_u64().unwrap() as usize
}
fn teams(v: &Value) -> Vec<i32> {
    array(v)
        .iter()
        .map(|v| v.as_i64().unwrap() as i32)
        .collect()
}
fn indices(v: &Value) -> Vec<usize> {
    array(v).iter().map(number).collect()
}
fn expand<T: Copy>(v: &Value, convert: impl Fn(&Value) -> T) -> Vec<T> {
    array(v)
        .iter()
        .flat_map(|r| std::iter::repeat_n(convert(&r[0]), number(&r[1])))
        .collect()
}
fn bits(actual: f64, expected: &Value, label: &str) {
    assert_eq!(
        format!("{:016x}", actual.to_bits()),
        expected.as_str().unwrap(),
        "{label}"
    );
}

#[test]
fn raw_authoring_distances_and_mode_filter_match_javascript() {
    let f = fixture();
    assert_eq!(array(&f["terrain"]).len(), 41);
    for row in array(&f["terrain"]) {
        let flags = expand(&row["flags"], |v| number(v) as u16);
        let heights = (!row["heights"].is_null())
            .then(|| expand(&row["heights"], |v| v.as_i64().unwrap() as i8));
        let terrain = TerrainView {
            width: number(&row["w"]),
            height: number(&row["h"]),
            flags: &flags,
            heights: heights.as_deref(),
        };
        let spawns: Vec<_> = array(&row["spawns"])
            .iter()
            .map(|v| Spawn {
                x: number(&v["x"]),
                y: number(&v["y"]),
                assault: v["assault"].as_bool().unwrap_or(false),
            })
            .collect();
        for group in array(&row["groups"]) {
            let selected =
                deployment::spawns_for(&spawns, Mode::from_kind(group["mode"].as_str())).unwrap();
            assert_eq!(selected, indices(&group["indices"]), "{}", row["name"]);
            let result = deployment::spawn_distances(&terrain, &spawns, &selected).unwrap();
            assert_eq!(result.len(), array(&group["distances"]).len());
            for (actual, expected) in result.into_iter().zip(array(&group["distances"])) {
                bits(actual, expected, row["name"].as_str().unwrap());
            }
        }
    }
}

#[test]
fn layouts_and_vp_goals_match_javascript() {
    let f = fixture();
    assert!(array(&f["layouts"]).len() > 900);
    for row in array(&f["layouts"]) {
        let dist = (!row["dist"].is_null()).then(|| {
            array(&row["dist"])
                .iter()
                .flat_map(|r| array(r).iter().map(|v| v.as_f64().unwrap()))
                .collect::<Vec<_>>()
        });
        let selection = row["fraction"]
            .as_f64()
            .map_or(LayoutSelection::First, LayoutSelection::RandomFraction);
        let actual = deployment::spawn_slots(
            number(&row["n"]),
            &teams(&row["teams"]),
            dist.as_deref(),
            selection,
        )
        .unwrap();
        assert_eq!(
            actual,
            indices(&row["slots"]),
            "{} {}",
            row["label"],
            row["fraction"]
        );
    }
    for row in array(&f["goals"]) {
        bits(
            deployment::win_vp(&teams(&row["teams"])).unwrap(),
            &row["expected"],
            "VP goal",
        );
    }
}

#[test]
fn malformed_inputs_fail_before_search_or_layout_enumeration() {
    let first = LayoutSelection::First;
    for count in [0, 7, usize::MAX] {
        assert_eq!(
            deployment::spawn_slots(count, &[0], None, first),
            Err(Error::SpawnCount)
        );
    }
    for teams in [&[][..], &[0; 7][..]] {
        assert_eq!(
            deployment::spawn_slots(1, teams, None, first),
            Err(Error::PlayerCount)
        );
        assert_eq!(deployment::win_vp(teams), Err(Error::PlayerCount));
    }
    for fraction in [-0.1, 1.0, f64::INFINITY, f64::NAN] {
        assert_eq!(
            deployment::spawn_slots(1, &[0], None, LayoutSelection::RandomFraction(fraction)),
            Err(Error::RandomFraction)
        );
    }
    assert_eq!(
        deployment::spawn_slots(2, &[0, 1], Some(&[0.0]), first),
        Err(Error::DistanceShape)
    );
    for value in [-1.0, f64::INFINITY, f64::NAN] {
        assert_eq!(
            deployment::spawn_slots(1, &[0], Some(&[value]), first),
            Err(Error::DistanceValue)
        );
    }
    assert_eq!(
        deployment::spawn_slots(2, &[0, 1, 2], Some(&[f64::MAX; 4]), first),
        Err(Error::ScoreRange)
    );
    let spawns = [Spawn {
        x: 0,
        y: 0,
        assault: false,
    }];
    let good = TerrainView {
        width: 1,
        height: 1,
        flags: &[0],
        heights: None,
    };
    assert_eq!(
        deployment::spawn_distances(&good, &spawns, &[]).unwrap(),
        Vec::<f64>::new()
    );
    assert_eq!(
        deployment::spawn_distances(&good, &spawns, &[1]),
        Err(Error::SpawnIndex(1))
    );
    assert_eq!(
        deployment::spawn_distances(&good, &spawns, &[0; 7]),
        Err(Error::SpawnCount)
    );
    assert_eq!(
        deployment::spawns_for(&[], Mode::Default),
        Err(Error::SpawnCount)
    );
    for bad in [
        TerrainView {
            width: usize::MAX,
            ..good
        },
        TerrainView { height: 0, ..good },
        TerrainView { flags: &[], ..good },
        TerrainView {
            heights: Some(&[]),
            ..good
        },
    ] {
        assert_eq!(
            deployment::spawn_distances(&bad, &spawns, &[0]),
            Err(Error::TerrainShape)
        );
    }
    let bad = TerrainView {
        heights: Some(&[5]),
        ..good
    };
    assert_eq!(
        deployment::spawn_distances(&bad, &spawns, &[0]),
        Err(Error::HeightRange)
    );
    let bad = TerrainView {
        flags: &[1],
        ..good
    };
    assert_eq!(
        deployment::spawn_distances(&bad, &spawns, &[0]),
        Err(Error::SpawnCell(0))
    );
    let bad_spawn = [Spawn {
        x: 1,
        y: 0,
        assault: false,
    }];
    assert_eq!(
        deployment::spawn_distances(&good, &bad_spawn, &[0]),
        Err(Error::SpawnCell(0))
    );
}
