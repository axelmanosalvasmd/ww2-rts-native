//! Original Tutorial callbacks on declared facts, not Tutorial match execution.
use serde_json::{Value, json};
use ww2_core::tutorial_steps::{CompletionView, SpawnOrder, TutorialPlayer, TutorialStep, TutorialUnit};

fn fixture() -> Value {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tutorial-steps.json");
    let bytes = std::fs::read(path)
        .expect("Root must grant a Source slot and generate tutorial-steps.json first");
    let f: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["kind"], "tutorial-step-components-not-match-runtime");
    assert_eq!(f["format"], 1);
    assert_eq!(
        f["sourceSha256"],
        "3aaa5e6affd936dae0ae4741d5173eee5d006132a1705d4b553097d3d8fabf42"
    );
    assert_eq!(f["definitions"].as_array().unwrap().len(), 9);
    assert_eq!(f["cases"].as_array().unwrap().len(), 50);
    f
}

fn check(index: usize, required: &[&str]) {
    let f = fixture();
    let step = TutorialStep::ALL[index];
    let entry = step.entry();
    let point = |p: ww2_core::game::Point| json!({"x":p.x,"z":p.z});
    let actual = json!({
        "goal":entry.goal, "at":point(entry.at), "manpowerFloor":entry.manpower_floor,
        "spawns":entry.spawns.iter().map(|s| json!({
            "kind":s.kind,"at":point(s.at),"healthShare":s.health_share,"autoRetreat":s.auto_retreat,
            "order":match s.order {
                SpawnOrder::None => Value::Null,
                SpawnOrder::AttackMove(at) => json!({"kind":"amove","at":point(at)}),
                SpawnOrder::Garrison(at) => json!({"kind":"garrison","at":point(at)}),
            },
        })).collect::<Vec<_>>(),
    });
    assert_eq!(actual, f["definitions"][index], "step {} entry", index + 1);
    let rows: Vec<_> = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["step"].as_u64() == Some(index as u64))
        .collect();
    let names: Vec<_> = rows.iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        required,
        "complete boundary inventory for step {}",
        index + 1
    );
    for c in rows {
        let input = &c["input"];
        let players: Vec<_> = input["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| TutorialPlayer {
                team: p["team"].as_i64().unwrap() as i32,
                artillery_cooldown: p["artillery"].as_f64().unwrap(),
            })
            .collect();
        let units: Vec<_> = input["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| TutorialUnit {
                id: u["id"].as_u64().unwrap() as u32,
                owner: u["owner"].as_u64().unwrap() as usize,
                kind: u["kind"].as_str().unwrap(),
                hp: u["hp"].as_f64().unwrap(),
                x: u["x"].as_f64().unwrap(),
                retreating: u["retreating"].as_bool().unwrap(),
            })
            .collect();
        let points: Vec<_> = input["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| usize::try_from(p.as_i64().unwrap()).ok())
            .collect();
        let spawned: Vec<_> = input["spawned"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_u64().unwrap() as u32)
            .collect();
        let view = CompletionView {
            players: &players,
            units: &units,
            point_owners: &points,
            spawned: &spawned,
        };
        assert_eq!(
            step.complete(&view).unwrap(),
            c["expected"].as_bool().unwrap(),
            "{}",
            c["name"]
        );
    }
}

const CLEARED: &[&str] = &[
    "both-alive",
    "one-dead",
    "both-dead",
    "retired-and-dead",
    "both-retired-unrelated-enemy-lives",
    "no-spawned-ids",
];
const HELD: &[&str] = &[
    "neutral",
    "enemy",
    "slot-zero",
    "allied-slot",
    "other-point-held",
];

#[test]
fn mode61_hedgerow() {
    check(
        0,
        &[
            "before-threshold",
            "at-threshold",
            "below-threshold",
            "ally-behind",
            "dead-behind",
            "empty-team-zero",
        ],
    );
}
#[test]
fn mode62_orchard() {
    check(1, CLEARED);
}
#[test]
fn mode63_crossroads() {
    check(2, HELD);
}
#[test]
fn mode64_recruit() {
    check(
        3,
        &["no-at", "enemy-at", "dead-at", "owned-at", "allied-at"],
    );
}
#[test]
fn mode65_tank() {
    check(4, CLEARED);
}
#[test]
fn mode66_trench() {
    check(5, CLEARED);
}
#[test]
fn mode67_retreat() {
    check(
        6,
        &[
            "no-retreat",
            "enemy-retreat",
            "dead-retreat",
            "owned-retreat",
            "allied-retreat",
        ],
    );
}
#[test]
fn mode68_artillery() {
    check(
        7,
        &[
            "no-call",
            "enemy-call",
            "zero-cooldown",
            "negative-cooldown",
            "owned-call",
            "allied-call",
        ],
    );
}
#[test]
fn mode69_bridgehead() {
    check(8, HELD);
}

#[test]
fn malformed_completion_facts_are_refused() {
    let players = [TutorialPlayer {
        team: 0,
        artillery_cooldown: 0.0,
    }];
    let unit = TutorialUnit {
        id: 1,
        owner: 0,
        kind: "rifle",
        hp: 100.0,
        x: 24.0,
        retreating: false,
    };
    let check = |units: &[TutorialUnit<'_>], point_owners: &[Option<usize>]| {
        let view = CompletionView {
            players: &players,
            units,
            point_owners,
            spawned: &[],
        };
        TutorialStep::Hedgerow.complete(&view)
    };
    assert_eq!(check(&[unit], &[]), Ok(true));
    assert!(check(&[TutorialUnit { owner: 1, ..unit }], &[]).is_err());
    assert!(check(&[TutorialUnit { hp: f64::NAN, ..unit }], &[]).is_err());
    assert!(check(&[TutorialUnit { x: f64::INFINITY, ..unit }], &[]).is_err());
    assert!(check(&[unit, unit], &[]).is_err());
    assert!(check(&[unit], &[Some(1)]).is_err());
    let view = CompletionView {
        players: &players,
        units: &[unit],
        point_owners: &[],
        spawned: &[],
    };
    assert!(TutorialStep::Bridgehead.complete(&view).is_err());
    let invalid = [TutorialPlayer {
        artillery_cooldown: f64::NAN,
        ..players[0]
    }];
    let view = CompletionView {
        players: &invalid,
        units: &[],
        point_owners: &[],
        spawned: &[],
    };
    assert!(TutorialStep::Artillery.complete(&view).is_err());
}
