//! Actual Classic constructor and base-component caller, never expected-state setup.
use super::*;
use crate::{
    economy::Faction, economy_state::ArmySize, map::AuthoritativeMap, match_commands::Command,
    match_runtime::PlayerConfig, production::JobStatus, weather::WeatherChoice,
};
use serde_json::{Value as Json, json};
fn bits(n: f64) -> String {
    format!("{:016x}", n.to_bits())
}
fn point(p: Option<Point>) -> Json {
    p.map_or(Json::Null, |p| json!([bits(p.x), bits(p.z)]))
}
fn make(input: &Json) -> Match {
    let mut map = input["map"].clone();
    let row = map["fill"]
        .as_str()
        .unwrap()
        .repeat(map["w"].as_u64().unwrap() as usize);
    map.as_object_mut().unwrap().remove("fill");
    map["rows"] = json!(vec![row; map["h"].as_u64().unwrap() as usize]);
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&map).unwrap()).unwrap();
    Match::new_classic(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Classic,
            players: input["names"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(i, name)| PlayerConfig {
                    name: name.as_str().unwrap().into(),
                    team: input["teams"][i].as_i64().unwrap() as i32,
                    faction: if input["factions"][i] == 0 {
                        Faction::Usa
                    } else {
                        Faction::Germany
                    },
                })
                .collect(),
            shuffle: false,
            army: if input["settings"]["army"] == "large" {
                ArmySize::Large
            } else {
                ArmySize::Standard
            },
            weather: WeatherChoice::Map,
            weather_seed: Some(217),
            living_weather: false,
            map_key: None,
            tech: true,
            supply: true,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}
fn numeric_bits(value: Json) -> Json {
    match value {
        Json::Number(n) => json!(bits(n.as_f64().unwrap())),
        Json::Array(a) => Json::Array(a.into_iter().map(numeric_bits).collect()),
        Json::Object(o) => Json::Object(o.into_iter().map(|(k, v)| (k, numeric_bits(v))).collect()),
        v => v,
    }
}
fn projection(m: &Match) -> Json {
    let state = m.classic_runtime.as_ref().unwrap();
    json!({"tick":m.world.game.tick,"next":m.next_unit_id,"nextJob":m.production.next_job_id(),
        "time":bits(state.time_left),"suddenDeath":state.sudden_death,"teams":state.initial_teams,
        "nodes":state.nodes.iter().map(|n| json!([n.cell,point(Some(n.at)),n.depot.unwrap_or(0),bits(n.rate),n.fuel])).collect::<Vec<_>>(),
        "wallets":m.start.economy.players.iter().map(|p| json!([bits(p.mp),bits(p.mun),bits(p.fuel),bits(p.inc),
            p.fuel_inc.map(bits),p.upkeep.map(bits)])).collect::<Vec<_>>(),
        "spent":m.story.iter().map(|s| bits(s.mp_spent)).collect::<Vec<_>>(),
        "producers":m.world.game.units.values().filter_map(|u| m.production.building(u.id).map(|b| json!([
            u.id,u.owner,bits(u.hp),bits(u.built.unwrap()),bits(b.progress()),point(m.building_rallies[&u.id]),
            b.queue().collect::<Vec<_>>(),b.rows().into_iter().map(|j| json!([j.job.id,j.job.kind,bits(j.job.charge.mp),bits(j.job.charge.fuel),
                if j.status == JobStatus::Active {"active"} else {"waiting"}])).collect::<Vec<_>>()]))).collect::<Vec<_>>(),
        "actors":m.world.game.units.values().map(|u| json!([u.id,u.owner,m.rules.kinds[u.kind].name])).collect::<Vec<_>>(),
        "rallyEnds":m.world.game.units.values().filter(|u| u.id as usize > m.start.players.len() * 3)
            .map(|u| json!([u.id,point(m.commands.units[&u.id].route_end)])).collect::<Vec<_>>(),

    })
}
fn check_recipients(m: &Match, expected: &Json) {
    for slot in 0..m.world.game.players.len() {
        let rows = crate::recipient_snapshot::production_for(m, slot).unwrap();
        for name in ["queues", "productionJobs"] {
            for row in rows[name].as_array().unwrap() {
                assert_eq!(
                    m.world.game.units[&(row[0].as_u64().unwrap() as u32)].owner,
                    slot
                );
            }
        }
        let actual = numeric_bits(Json::Object(rows));
        assert_eq!(actual, expected[slot]["cached"]);
        assert_eq!(actual, expected[slot]["uncached"]);
    }
}
fn replay(name: &str) -> Match {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/classic-base-components.json");
    let f: Json = serde_json::from_str(
        &std::fs::read_to_string(path)
            .expect("Root must grant exact Source recording before this comparison"),
    )
    .unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    assert_eq!(f["cases"].as_array().unwrap().len(), 3);
    let case = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap();
    let mut m = make(&case["input"]);
    assert_eq!(projection(&m), case["initial"]);
    check_recipients(&m, &case["initialRecipients"]);
    let poses = m
        .world
        .game
        .units
        .values()
        .map(|u| {
            json!([
                u.id,
                bits(u.x),
                bits(u.z),
                bits(u.rot),
                bits(m.commands.units[&u.id].aim),
            ])
        })
        .collect::<Vec<_>>();
    assert_eq!(json!(poses), case["initialPoses"]);
    let mut ticks = 0;
    for (index, row) in case["records"].as_array().unwrap().iter().enumerate() {
        match row["op"].as_str().unwrap() {
            "command" => {
                let before = projection(&m);
                let command = Command::from_json(&row["cmd"]).unwrap();
                let result = m
                    .execute_command(row["slot"].as_u64().unwrap() as usize, &command, false)
                    .unwrap();
                assert_eq!(json!(result.map(|d| d.name())), row["result"]);
                if !row["result"].is_null() {
                    assert_eq!(projection(&m), before);
                }
            }
            "steps" => {
                let count = row["count"].as_u64().unwrap();
                for _ in 0..count {
                    m.advance_classic_base_tick().unwrap();
                }
                ticks += count;
            }
            _ => panic!("unexpected Source component action"),
        }
        assert_eq!(projection(&m), row["expected"], "{name} record {index}");
        check_recipients(&m, &row["recipients"]);
    }
    assert_eq!(json!(ticks), case["ticks"]);
    m
}
#[test]
fn classic_actual_hqs_force_nodes_scaled_large_wallet_and_allied_private_rows() {
    let m = replay("classic-start-wallet-large");
    let charge = m.production.building(1).unwrap().jobs()[0].charge;
    let income = Catalog::compiled()
        .unwrap()
        .table("CFG")
        .unwrap()
        .get("armies")
        .unwrap()
        .get("large")
        .unwrap()
        .get("income")
        .and_then(Value::number)
        .unwrap();
    let start = 200. * income;
    assert_eq!(m.start.economy.players[0].mp + charge.mp, start);
    assert!(
        m.start.economy.players[1..]
            .iter()
            .all(|p| p.mp == start && p.mun == 0. && p.fuel == 0.)
    );
}
#[test]
fn classic_actual_income_paid_five_slots_waiter_cancel_and_rifle_birth() {
    let m = replay("classic-real-queue-cancel-and-birth");
    assert_eq!(m.next_unit_id, 8);
    assert_eq!(m.production.building(1).unwrap().jobs().len(), 3);
}
#[test]
fn classic_actual_startup_clock_reaches_deadline_clears_once_without_refund_and_decays() {
    let m = replay("classic-genuine-deadline-clear-and-decay");
    assert!(m.classic_sudden_death());
    assert!(
        m.production
            .buildings()
            .all(|(_, b)| b.jobs().is_empty() && b.progress() == 0.)
    );
    assert!(m.world.game.tick >= 30_000);
}

#[test]
fn classic_actual_paid_queue_and_waiter_story_failures_preserve_state() {
    for cancel in [false, true] {
        let mut m = make(
            &json!({"map":{"name":"Classic canonical bases","w":128,"h":128,"fill":".",
            "spawns":[{"x":8,"y":8},{"x":112,"y":112}],"points":[]},
            "settings":{"army":"standard"},"names":["A","B"],"teams":[0,1],"factions":[0,1]}),
        );
        let buy = Command::from_json(&json!({"t":"buy","unit":"rifle","from":1})).unwrap();
        for _ in 0..if cancel { 2 } else { 1 } {
            assert_eq!(m.execute_command(0, &buy, false).unwrap(), None);
        }
        let command = if cancel {
            let id = m.production.building(1).unwrap().jobs()[1].id;
            Command::from_json(&json!({"t":"cancelProduction","id":1,"job":id})).unwrap()
        } else {
            buy
        };
        // Bookkeeping-only fault after real payment, never a fabricated job,
        // altered wallet, actor pose, health, deadline or random stream.
        m.story.clear();
        let before = projection(&m);
        let paid = m.production.clone();
        let wallets = m.start.economy.clone();
        let controls = m.commands.units.clone();
        let logs = (
            m.world.cell_log().to_vec(),
            m.world.new_cells().to_vec(),
            m.world.journal().to_vec(),
            m.world.journal_base(),
            m.world.journal_end().unwrap(),
        );
        let pending = (0..m.world.game.players.len())
            .map(crate::world::Recipient::Player)
            .chain(std::iter::once(crate::world::Recipient::Spectators))
            .map(|r| m.world.pending(r).cloned())
            .collect::<Vec<_>>();
        assert!(m.execute_command(0, &command, false).is_err());
        assert_eq!(projection(&m), before);
        assert_eq!(m.production, paid);
        assert_eq!(m.start.economy, wallets);
        assert_eq!(m.commands.units, controls);
        assert!(m.story.is_empty());
        assert_eq!(
            (
                m.world.cell_log().to_vec(),
                m.world.new_cells().to_vec(),
                m.world.journal().to_vec(),
                m.world.journal_base(),
                m.world.journal_end().unwrap()
            ),
            logs
        );
        assert_eq!(
            (0..m.world.game.players.len())
                .map(crate::world::Recipient::Player)
                .chain(std::iter::once(crate::world::Recipient::Spectators))
                .map(|r| m.world.pending(r).cloned())
                .collect::<Vec<_>>(),
            pending
        );
    }
}
