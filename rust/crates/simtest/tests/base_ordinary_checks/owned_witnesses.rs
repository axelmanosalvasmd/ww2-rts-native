//! Unrun witness compositions. No missing fixture is treated as passing coverage.
use super::*;
use ww2_core::match_tick::TickPhase;

#[path = "actual_consumers.rs"]
mod actual_consumers;

fn projection(m: &Match) -> Value {
    assert_isolated_point(m);
    json!({
        "tick": m.world.game.tick, "nextId": m.next_unit_id,
        "actors": m.world.game.units.values().map(|u| {
            let structure = m.world.structures.iter().find(|s| s.entity_id == Some(u.id));
            if m.rules.kinds[u.kind].building {
                let structure = structure.expect("canonical building structure");
                let cells = u.cells.as_ref().expect("canonical actor footprint");
                let indices = cells.iter().map(|cell| usize::try_from(*cell).expect("actor cell fits usize")).collect::<Vec<_>>();
                assert_eq!(structure.cells, indices);
                for cell in indices { assert_eq!(m.building_cells[&cell].unit, u.id); }
            }
            json!([u.id, u.owner, m.rules.kinds[u.kind].name, bits(u.hp), bits(u.x), bits(u.z),
                u.built.map(bits), u.cells, structure.map(|s| s.id.as_str())])
        }).collect::<Vec<_>>(),
        "controls": m.world.game.units.keys().map(|id| {
            let c = &m.commands.units[id];
            json!([id, c.orders.len(), c.build_target])
        }).collect::<Vec<_>>(),
        "wallets": m.start.economy.players.iter().map(|p| bits(p.mp)).collect::<Vec<_>>(),
        "story": m.story.iter().map(|p| json!([bits(p.kills), bits(p.losses), bits(p.mp_spent)])).collect::<Vec<_>>(),
        "chars": String::from_utf8(m.world.game.chars.clone()).unwrap(),
        "buildingCells": m.building_cells.iter().map(|(cell, b)| json!([cell, b.unit])).collect::<Vec<_>>(),
        "structures": m.world.structures.iter().map(|s| json!([s.id, s.entity_id, s.cells])).collect::<Vec<_>>(),
    })
}

fn replay(name: &str) -> Match {
    let base: Value =
        serde_json::from_str(include_str!("../fixtures/base-ordinary-checks.json")).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/base-owned-witnesses.json");
    let fixture: Value = serde_json::from_str(
        &std::fs::read_to_string(path)
            .expect("Root must grant authentic Source generation/check before these five tests"),
    )
    .unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    assert_eq!(fixture["seed"], 701);
    assert_eq!(
        fixture["scope"],
        "Five ordinary Conquest component witnesses; no Classic or whole-row equivalence claim"
    );
    assert_eq!(
        fixture["input"],
        json!({"map":base["map"], "opts":base["opts"], "inputs":base["inputs"], "setup":base["setup"]})
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    let case = cases.iter().find(|c| c["name"] == name).unwrap();
    let labels: Vec<String> = match name {
        "base26_conquest_queue_refusal_stop_retry" => std::iter::once("active-move".into())
            .chain((0..8).map(|i| format!("waiting-{i}")))
            .chain(["full-queue".into(), "stop".into(), "retry".into()])
            .collect(),
        "base27_visible_blocked" | "base27_hidden_masks_blocked" => vec!["placement".into()],
        "recruitment_real_makers_and_prerequisites" => [
            "ineligible-maker",
            "ineligible-maker",
            "missing-motorpool",
            "real-site",
            "unfinished-explicit-maker",
            "scuttle-site",
            "dead-explicit-maker",
            "real-maker-positive",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        "base32_scuttle_owned_hq_and_unit_next_tick" => {
            vec!["foreign-refused".into(), "own-scuttle".into()]
        }
        other => panic!("unexpected witness {other}"),
    };
    assert_eq!(
        case["records"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|r| r["label"].as_str())
            .map(String::from)
            .collect::<Vec<_>>(),
        labels
    );
    let poses = usize::from(name == "base26_conquest_queue_refusal_stop_retry");
    let ticks = usize::from(name == "base32_scuttle_owned_hq_and_unit_next_tick");
    assert_eq!(
        case["records"].as_array().unwrap().len(),
        labels.len() + poses + ticks
    );
    let mut m = make(&base, &base["cases"][0]);
    assert_eq!(
        json!(m.world.game.units.keys().copied().collect::<Vec<_>>()),
        case["originalActors"]
    );
    assert_eq!(
        projection(&m),
        case["initial"],
        "{name}: constructor and declared setup"
    );
    for record in case["records"].as_array().unwrap() {
        let before = projection(&m);
        let orders = m
            .commands
            .units
            .iter()
            .map(|(id, c)| (*id, c.orders.clone()))
            .collect::<BTreeMap<_, _>>();
        match record["op"].as_str().unwrap() {
            "pose" => {
                let builder = base["cases"][0]["initialAliases"]["builder"]
                    .as_u64()
                    .unwrap();
                let u = m
                    .world
                    .game
                    .units
                    .get_mut(&u32::try_from(builder).unwrap())
                    .unwrap();
                u.x = record["x"].as_f64().unwrap();
                u.z = record["z"].as_f64().unwrap();
                refresh(&mut m);
            }
            "command" => {
                let command = Command::from_json(&record["cmd"]).unwrap();
                let result = m.execute_command(0, &command, false).unwrap();
                assert_eq!(
                    json!(result.map(|d| d.name())),
                    record["expect"],
                    "{name}: {}",
                    record["label"]
                );
                if matches!(
                    record["expect"].as_str(),
                    Some("queueFull" | "blocked" | "notVisible" | "needs" | "unseen")
                ) {
                    assert_eq!(
                        projection(&m),
                        before,
                        "refused command changes no measured state"
                    );
                    for (id, waiting) in orders {
                        assert_eq!(m.commands.units[&id].orders, waiting);
                    }
                }
            }
            "tick" => {
                let tick = m.world.game.tick;
                let report = m.advance_tick().unwrap();
                assert_eq!(report.tick, tick + 1);
                for phase in [
                    TickPhase::Construction,
                    TickPhase::Death,
                    TickPhase::PointCaptureIncome,
                    TickPhase::Victory,
                ] {
                    assert!(report.phases.contains(&phase));
                    assert!(!report.unavailable.contains(&phase));
                }
            }
            other => panic!("unexpected Source action {other}"),
        }
        assert_eq!(
            projection(&m),
            record["expected"],
            "{name}: full declared projection at {}",
            record["label"]
        );
    }
    m
}

#[test]
fn base26_conquest_queue_refusal_stop_retry() {
    let m = replay("base26_conquest_queue_refusal_stop_retry");
    let site = &m.world.game.units[&(m.next_unit_id - 1)];
    assert_eq!(m.rules.kinds[site.kind].name, "barracks");
    assert_eq!(site.built, Some(0.0));
    assert_eq!(m.start.economy.players[0].mp, 10000.0 - 150.0);
    let helper = m
        .commands
        .units
        .values()
        .find(|c| matches!(c.orders.front(), Some(Command::Assist { target, .. }) if *target == i64::from(site.id)))
        .unwrap();
    assert_eq!(helper.orders.len(), 1);
}

#[test]
fn base27_visible_blocked() {
    replay("base27_visible_blocked");
}

#[test]
fn base27_hidden_masks_blocked() {
    replay("base27_hidden_masks_blocked");
}

#[test]
fn recruitment_real_makers_and_prerequisites() {
    let m = replay("recruitment_real_makers_and_prerequisites");
    let id = m.next_unit_id - 1;
    let purchased = &m.world.game.units[&id];
    assert_eq!(m.rules.kinds[purchased.kind].name, "mg");
    assert_eq!(purchased.owner, 0);
    let producer = &m.world.game.units[&m.recruit_metadata[&id].building];
    assert_eq!(m.rules.kinds[producer.kind].name, "barracks");
    assert_eq!(producer.owner, 0);
    assert_eq!(producer.built, Some(1.0));
    assert!(producer.hp > 0.0);
    assert!(ww2_core::math::hypot(purchased.x - producer.x, purchased.z - producer.z) < 12.0);
    assert!(
        m.production
            .building(producer.id)
            .unwrap()
            .jobs()
            .is_empty()
    );
}

#[test]
fn base32_scuttle_owned_hq_and_unit_next_tick() {
    let m = replay("base32_scuttle_owned_hq_and_unit_next_tick");
    assert_eq!(m.story[0].losses, 2.0);
    assert_eq!(m.story[1].kills, 0.0);
    assert_eq!(m.story[0].mp_spent, 0.0);
    assert!(
        !m.world
            .game
            .units
            .values()
            .any(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "hq")
    );
    assert!(
        m.world
            .game
            .units
            .values()
            .any(|u| u.owner == 1 && m.rules.kinds[u.kind].name == "hq" && u.hp > 0.0)
    );
}
