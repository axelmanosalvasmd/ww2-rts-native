//! Full public leg results recorded from the reference, including cache history.
use serde_json::Value;
use ww2_core::navigation::{
    Navigation, NavigationError, Options, TerrainChange, TerrainView, Versions,
};

struct Change {
    from: u64,
    to: u64,
    cells: Vec<usize>,
}
impl Change {
    fn borrowed(&self) -> TerrainChange<'_> {
        TerrainChange {
            from: self.from,
            to: self.to,
            cells: &self.cells,
        }
    }
}
fn integer(value: &Value, key: &str) -> usize {
    value[key].as_u64().unwrap() as usize
}

#[test]
fn navigation_legs_match_reference_including_mutation_history() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/navigation.json")).unwrap();
    assert_eq!(fixture["reference"], "e4e579a");
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let mut queries = 0;
    let mut limited = 0;
    let mut disconnected = 0;
    let mut patches = 0;
    for scenario in fixture["scenarios"].as_array().unwrap() {
        let name = scenario["name"].as_str().unwrap();
        let width = integer(scenario, "w");
        let height = integer(scenario, "h");
        let mut flags: Vec<u16> = scenario["flags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u16)
            .collect();
        let mut heights: Option<Vec<i8>> = scenario["height"]
            .as_array()
            .map(|a| a.iter().map(|v| v.as_i64().unwrap() as i8).collect());
        let mut versions = Versions::default();
        let mut knowledge_id = 1;
        let mut direct: Option<Change> = None;
        let mut history: Vec<Change> = Vec::new();
        let mut navigation = Navigation::new();
        for (step, action) in scenario["actions"].as_array().unwrap().iter().enumerate() {
            if action["op"] == "patch" {
                patches += 1;
                let cells: Vec<usize> = action["cells"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|row| {
                        let c = row[0].as_u64().unwrap() as usize;
                        flags[c] = row[1].as_u64().unwrap() as u16;
                        if let Some(heights) = &mut heights {
                            heights[c] = row[2].as_i64().unwrap() as i8;
                        }
                        c
                    })
                    .collect();
                direct = None;
                let from = versions.knowledge;
                match action["mode"].as_str().unwrap() {
                    "direct" | "empty" => {
                        versions.knowledge += 1;
                        direct = Some(Change {
                            from,
                            to: from + 1,
                            cells,
                        });
                    }
                    "log" => {
                        versions.knowledge += 1;
                        history.push(Change {
                            from,
                            to: from + 1,
                            cells,
                        });
                    }
                    "gap" => {
                        versions.knowledge += 1;
                        history.clear();
                    }
                    "region" => {
                        versions.infantry += 1;
                        versions.vehicle += 1;
                        versions.naval += 1;
                    }
                    "obstruction" => {
                        versions.obstruction += 1;
                    }
                    "replace" => {
                        flags = flags.clone();
                        heights = heights.clone();
                    }
                    "knowledge" => {
                        knowledge_id += 1;
                    }
                    "none" => (),
                    other => panic!("unknown patch {other}"),
                }
                continue;
            }
            let changes: Vec<_> = history.iter().map(Change::borrowed).collect();
            let view = TerrainView {
                width,
                height,
                flags: &flags,
                heights: heights.as_deref(),
                world_known: scenario["worldKnown"].as_bool().unwrap(),
                knowledge_id,
                versions,
                changed: direct.as_ref().map(Change::borrowed),
                changes: &changes,
            };
            let result = navigation
                .leg(
                    view,
                    integer(action, "start"),
                    integer(action, "goal"),
                    integer(action, "block") as u16,
                    Options {
                        chunk_size: integer(action, "chunkSize"),
                        max_leg_cells: integer(action, "maxLegCells"),
                    },
                )
                .unwrap();
            let contains: String = (0..(width * height + 1).div_ceil(4))
                .map(|k| {
                    let mut nibble = 0u32;
                    for j in 0..4 {
                        if k * 4 + j <= width * height && result.contains(k * 4 + j) {
                            nibble |= 1 << j;
                        }
                    }
                    char::from_digit(nibble, 16).unwrap()
                })
                .collect();
            let actual = serde_json::json!({ "goal": result.goal, "connected": result.connected, "limited": result.limited,
                "allowed": result.allowed, "containsHex": contains });
            assert_eq!(
                actual, action["expected"],
                "{name}, step {step}, query {queries}"
            );
            queries += 1;
            limited += usize::from(result.limited);
            disconnected += usize::from(!result.connected);
        }
    }
    assert_eq!(
        (queries, limited, disconnected, patches),
        (4888, 1372, 1268, 277),
        "reference coverage changed"
    );
}

#[test]
fn invalid_shapes_and_cells_are_rejected_before_cache_access() {
    let mut navigation = Navigation::new();
    let mut view = TerrainView {
        width: 1,
        height: 1,
        flags: &[0],
        heights: None,
        world_known: false,
        knowledge_id: 0,
        versions: Versions::default(),
        changed: None,
        changes: &[],
    };
    assert_eq!(
        navigation
            .leg(view, 1, 0, 1, Options::default())
            .unwrap_err(),
        NavigationError::CellOutOfBounds
    );
    view.height = 0;
    assert_eq!(
        navigation
            .leg(view, 0, 0, 1, Options::default())
            .unwrap_err(),
        NavigationError::InvalidShape
    );
    view.height = 2;
    assert_eq!(
        navigation
            .leg(view, 0, 0, 1, Options::default())
            .unwrap_err(),
        NavigationError::InvalidShape
    );
    view.height = usize::MAX;
    view.width = 2;
    assert_eq!(
        navigation
            .leg(view, 0, 0, 1, Options::default())
            .unwrap_err(),
        NavigationError::InvalidShape
    );
    view.width = 1;
    view.height = 1;
    view.heights = Some(&[]);
    assert_eq!(
        navigation
            .leg(view, 0, 0, 1, Options::default())
            .unwrap_err(),
        NavigationError::InvalidShape
    );
}
