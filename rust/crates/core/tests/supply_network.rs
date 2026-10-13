use serde_json::Value;
use ww2_core::{
    game::Point,
    navigation::{TerrainView, Versions},
    objectives::{Objective, ObjectivePlayer, ObjectiveUnit},
    supply_network::{GroundSupplyNetwork, SupplyInput, SupplyUnit},
    unit_spawn::SpawnPlacer,
};
fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn expand(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .flat_map(|p| {
            std::iter::repeat_n(
                p[0].as_u64().unwrap() as u16,
                p[1].as_u64().unwrap() as usize,
            )
        })
        .collect()
}
#[test]
fn ground_network_and_refill_rates_match_original_functions() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/supply-network.json")).unwrap();
    let objectives: Value = serde_json::from_str(include_str!("fixtures/objectives.json")).unwrap();
    for row in fixture["rows"].as_array().unwrap() {
        let input = if let Some(i) = row["sourceCase"].as_u64() {
            &objectives["supplies"][i as usize]
        } else {
            &row["input"]
        };
        let w = input["w"].as_u64().unwrap() as usize;
        let h = input["h"].as_u64().unwrap() as usize;
        let flags: Vec<_> = input["flags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u16)
            .collect();
        let heights = input["heights"].as_array().map(|a| {
            a.iter()
                .map(|v| v.as_i64().unwrap() as i8)
                .collect::<Vec<_>>()
        });
        let initial_heights = input["initialHeights"].as_array().map(|a| {
            a.iter()
                .map(|v| v.as_i64().unwrap() as i8)
                .collect::<Vec<_>>()
        });
        let chars = input["chars"].as_array().map_or_else(
            || vec![b'.'; w * h],
            |a| {
                a.iter()
                    .map(|v| v.as_str().unwrap().as_bytes()[0])
                    .collect()
            },
        );
        let initial_chars = input["initialChars"].as_array().map_or_else(
            || vec![b'.'; w * h],
            |a| {
                a.iter()
                    .map(|v| v.as_str().unwrap().as_bytes()[0])
                    .collect()
            },
        );
        let players: Vec<_> = input["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| ObjectivePlayer {
                team: p["team"].as_i64().unwrap() as i32,
                out: p["out"].as_bool().unwrap_or(false),
                spawn: point(&p["spawn"]),
            })
            .collect();
        let points: Vec<_> = input["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| Objective {
                id: p["id"].clone(),
                at: point(p),
                vp: 0.0,
                mp: 0.0,
                kind: p["kind"].as_str().map(str::to_owned),
                needs: None,
                owner: p["owner"].as_i64().filter(|v| *v >= 0).map(|v| v as usize),
                capper: None,
                progress: 0.0,
                cut: p["cut"].as_bool().unwrap_or(false),
                on_point: vec![],
                contested: p["contested"].as_bool().unwrap_or(false),
            })
            .collect();
        let units: Vec<_> = input["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| SupplyUnit {
                fact: ObjectiveUnit {
                    id: u["id"].as_u64().unwrap() as u32,
                    owner: u["owner"].as_u64().unwrap() as usize,
                    kind: u["type"].as_str().unwrap(),
                    at: point(u),
                    hp: u["hp"].as_f64().unwrap(),
                    airborne: !u["air"].is_null(),
                    riding: u["riding"].as_i64().unwrap_or(0) != 0,
                    retreating: u["retreating"].as_bool().unwrap_or(false),
                    forced_reserve: false,
                },
                built: u["built"].as_f64(),
            })
            .collect();
        let view = TerrainView {
            width: w,
            height: h,
            flags: &flags,
            heights: heights.as_deref(),
            world_known: false,
            knowledge_id: 0,
            versions: Versions::default(),
            changed: None,
            changes: &[],
        };
        let result = GroundSupplyNetwork::compute(
            SupplyInput {
                view,
                chars: &chars,
                initial_chars: &initial_chars,
                initial_heights: initial_heights.as_deref(),
                points: &points,
                units: &units,
                players: &players,
            },
            &mut SpawnPlacer::new(),
        )
        .unwrap();
        assert_eq!(result.teams.len(), row["net"].as_array().unwrap().len());
        for net in row["net"].as_array().unwrap() {
            let team = net["team"].as_i64().unwrap() as i32;
            let actual = result.teams.iter().find(|t| t.team == team).unwrap();
            assert_eq!(
                actual.steps,
                expand(&net["steps"]),
                "{} team{}",
                row["name"],
                team
            );
            assert_eq!(
                actual
                    .closed
                    .iter()
                    .map(|v| u16::from(*v))
                    .collect::<Vec<_>>(),
                expand(&net["closed"]),
                "{} closed{}",
                row["name"],
                team
            );
        }
        for (owner, p) in players.iter().enumerate() {
            for (i, at) in row["queries"].as_array().unwrap().iter().enumerate() {
                let actual = result.rate_at(Some(p.team), point(at)).unwrap();
                let expected =
                    u64::from_str_radix(row["rates"][owner][i].as_str().unwrap(), 16).unwrap();
                assert_eq!(
                    actual.to_bits(),
                    expected,
                    "{} owner{} query{}",
                    row["name"],
                    owner,
                    i
                );
            }
        }
    }
}

#[test]
fn malformed_network_queries_and_input_shapes_return_errors() {
    let mut net = GroundSupplyNetwork::default();
    assert_eq!(net.rate_at(None, Point { x: 1.0, z: 1.0 }).unwrap(), 0.0);
    assert!(
        net.rate_at(
            None,
            Point {
                x: f64::NAN,
                z: 1.0
            }
        )
        .is_err()
    );
    net.width = 1;
    net.height = 1;
    net.teams.push(ww2_core::supply_network::TeamNetwork {
        team: 0,
        steps: vec![],
        closed: vec![],
    });
    assert!(net.rate_at(Some(0), Point { x: 1.0, z: 1.0 }).is_err());
    let flags = [];
    let view = TerrainView {
        width: 1,
        height: 1,
        flags: &flags,
        heights: None,
        world_known: false,
        knowledge_id: 0,
        versions: Versions::default(),
        changed: None,
        changes: &[],
    };
    assert!(
        GroundSupplyNetwork::compute(
            SupplyInput {
                view,
                chars: &[],
                initial_chars: &[],
                initial_heights: None,
                points: &[],
                units: &[],
                players: &[]
            },
            &mut SpawnPlacer::new()
        )
        .is_err()
    );
}
