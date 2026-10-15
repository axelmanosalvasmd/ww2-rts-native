//! Pending IMPL_BASES union: crate-local canonical claim/cancel prefix consumer.
//! Not registered on f7. This does not cover destruction, Shipyard or enemy memory.
use crate::{
    economy::{Faction, Mode}, economy_state::ArmySize,
    map::AuthoritativeMap, match_commands::Command,
    match_runtime::{Match, MatchOptions, PlayerConfig}, weather::WeatherChoice,
};
use serde_json::{Value, json};
fn bits(n: f64) -> String { format!("{:016x}", n.to_bits()) }
fn point(p: Option<crate::game::Point>) -> Value {
    p.map_or(Value::Null, |p| json!([bits(p.x),bits(p.z)]))
}
fn selected(m: &Match) -> Value {
    json!({"tick":m.world.game.tick,"seed":m.start.environment_seed,"next":m.next_unit_id,
        "money":m.start.economy.players.iter().map(|p|
            json!([bits(p.mp),bits(p.mun),bits(p.fuel)])).collect::<Vec<_>>(),
        "spent":m.story.iter().map(|p| bits(p.mp_spent)).collect::<Vec<_>>(),
        "nodes":m.classic_runtime.as_ref().unwrap().nodes.iter().map(|n|
            json!([n.cell,point(Some(n.at)),n.depot.unwrap_or(0),bits(n.rate),n.fuel])).collect::<Vec<_>>(),
        "units":m.world.game.units.values().map(|u| json!({"id":u.id,"owner":u.owner,
            "type":m.rules.kinds[u.kind].name,"hp":bits(u.hp),"built":u.built.map(bits),
            "at":point(Some(u.at())),"path":u.path.iter().map(|p|point(Some(*p))).collect::<Vec<_>>(),
            "goal":point(u.world_goal),"cells":u.cells})).collect::<Vec<_>>()})
}
fn source_selected(raw: &Value) -> Value {
    json!({"tick":raw["tick"],"seed":raw["seed"],"next":raw["next"],
        "money":raw["money"],"spent":raw["spent"],"nodes":raw["nodes"],"units":raw["units"]})
}
#[test]
fn canonical_depot_claim_duplicate_refusal_and_cancel_match_original_prefix() {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../simtest/tests/fixtures/base12-node-claims.json");
    let f: Value = serde_json::from_str(&std::fs::read_to_string(file)
        .expect("Root must grant authentic BASE12 Source recording before this dependent test")).unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["seed"], 701);
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&f["input"]["map"]).unwrap()).unwrap();
    let mut m = Match::new_classic(&map, MatchOptions {
        seed:701, mode:Mode::Classic,
        players:vec![PlayerConfig{name:"A".into(),team:0,faction:Faction::Usa},
            PlayerConfig{name:"B".into(),team:1,faction:Faction::Germany}],
        shuffle:false,army:ArmySize::Standard,weather:WeatherChoice::Map,weather_seed:Some(1),
        living_weather:false,map_key:None,tech:true,supply:true,logistics:false,sandbox:false,
    }).unwrap();
    assert_eq!(selected(&m), source_selected(&f["initial"]));
    let random = m.start.random.snapshot();
    let mut seen = Vec::new();
    for row in f["records"].as_array().unwrap() {
        match row["op"].as_str().unwrap() {
            "command" => {
                assert_eq!(selected(&m), source_selected(&row["before"]));
                let before = (m.world.game.units.clone(),m.commands.units.clone(),
                    m.production.clone(),m.story.clone(),m.next_unit_id,
                    m.classic_runtime.as_ref().unwrap().nodes.clone(),
                    m.start.economy.players.iter().map(|p|[p.mp.to_bits(),p.mun.to_bits(),p.fuel.to_bits()]).collect::<Vec<_>>(),
                    m.building_cells.clone(),m.world.game.chars.clone(),m.world.game.cell_hp.clone());
                let command = Command::from_json(&row["cmd"]).unwrap();
                let result = m.execute_command(row["slot"].as_u64().unwrap() as usize,&command,false).unwrap();
                assert_eq!(json!(result.map(|d|d.name())),row["result"]);
                assert_eq!(selected(&m),source_selected(&row["after"]));
                if result.is_some() {
                    assert_eq!(row["result"],"blocked");
                    let after = (m.world.game.units.clone(),m.commands.units.clone(),
                        m.production.clone(),m.story.clone(),m.next_unit_id,
                        m.classic_runtime.as_ref().unwrap().nodes.clone(),
                        m.start.economy.players.iter().map(|p|[p.mp.to_bits(),p.mun.to_bits(),p.fuel.to_bits()]).collect::<Vec<_>>(),
                        m.building_cells.clone(),m.world.game.chars.clone(),m.world.game.cell_hp.clone());
                    assert_eq!(before,after,"duplicate Depot refusal must preserve canonical bindings and publication state");
                }
                seen.push(row["cmd"]["t"].as_str().unwrap().to_owned());
            }
            "placement" => {
                // Original preview predicates are retained; this prefix does not
                // claim a native preview, Shipyard or remembered-occupancy implementation.
                assert_eq!(row["ok"],false);
                assert_eq!(row["reason"],if row["kind"] == "depot" {"notVisible"} else {"blocked"});
            }
            "observe" => {
                assert_eq!(row["label"],"cancel-frees-node");
                assert_eq!(selected(&m),source_selected(&row["expected"]));
                assert!(m.classic_runtime.as_ref().unwrap().nodes.iter().all(|n|n.depot.is_none()));
                assert_eq!(m.start.economy.players[0].mp,185.0);
                assert_eq!(m.story[0].mp_spent,15.0);
                assert_eq!(seen,["stance","stance","build","build","cancel"]);
                assert_eq!(m.start.random.snapshot(),random);
                return;
            }
            _ => panic!("claim/cancel prefix ended without its named observation"),
        }
        assert_eq!(m.start.random.snapshot(),random);
    }
    panic!("missing cancel-frees-node observation");
}
