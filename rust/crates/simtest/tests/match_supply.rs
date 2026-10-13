use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    logistics::Stock,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn optional(n: Option<f64>) -> Value {
    n.map_or(Value::Null, bits)
}
fn stock(s: Stock) -> Value {
    json!([bits(s.ammo), bits(s.provisions), bits(s.fuel)])
}
fn hash(bytes: impl IntoIterator<Item = u8>) -> Value {
    let mut h = 0xcbf29ce484222325u64;
    for b in bytes {
        h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3);
    }
    json!(format!("{h:016x}"))
}
fn options(c: &Value, map_key: &str) -> MatchOptions {
    MatchOptions {
        seed: c["seed"].as_str().unwrap().parse().unwrap(),
        mode: Mode::Conquest,
        players: c["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| PlayerConfig {
                name: p["name"].as_str().unwrap().to_owned(),
                team: p["team"].as_i64().unwrap() as i32,
                faction: Faction::try_from(p["faction"].as_u64().unwrap() as u8).unwrap(),
            })
            .collect(),
        shuffle: c["shuffle"].as_bool().unwrap(),
        army: match c["army"].as_str().unwrap() {
            "standard" => ArmySize::Standard,
            "large" => ArmySize::Large,
            "massive" => ArmySize::Massive,
            "endless" => ArmySize::Endless,
            _ => unreachable!(),
        },
        weather: WeatherChoice::from_reference(c["weather"].as_str()),
        weather_seed: c["weatherSeed"].as_u64().map(|v| v as u32),
        living_weather: c["living"].as_bool().unwrap(),
        map_key: Some(map_key.to_owned()),
        tech: c["tech"].as_bool().unwrap(),
        supply: true,
        logistics: false,
        sandbox: false,
    }
}
#[test]
fn real_conquest_logistics_initialization_matches_original_create_game() {
    let base: Value = serde_json::from_str(include_str!("fixtures/match-start.json")).unwrap();
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-supply.json")).unwrap();
    for row in fixture["rows"].as_array().unwrap() {
        let i = row["sourceCase"].as_u64().unwrap() as usize;
        let c = &base["cases"][i];
        let input = &base["maps"][c["map"].as_u64().unwrap() as usize];
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&input["map"]).unwrap()).unwrap();
        let mut instance = Match::new(&map, options(c, input["name"].as_str().unwrap())).unwrap();
        instance.options.logistics = true;
        instance.initialize_logistics().unwrap();
        let reserves = instance
            .logistics
            .iter()
            .map(|(id, r)| {
                json!([
                    id,
                    [
                        bits(r.ammo),
                        bits(r.ammo_max),
                        bits(r.ammo_price),
                        optional(r.ammo_credit),
                        bits(r.provisions),
                        optional(r.fuel),
                        optional(r.emergency),
                        optional(r.warning),
                        r.forced,
                        r.stranded,
                        r.exhausted,
                        optional(r.supply_rate),
                        optional(r.cut_for)
                    ]
                ])
            })
            .collect::<Vec<_>>();
        assert_eq!(json!(reserves), row["reserves"], "case{i} reserves");
        assert_eq!(
            instance.supply_net.teams.len(),
            row["net"].as_array().unwrap().len()
        );
        for net in row["net"].as_array().unwrap() {
            let team = net["team"].as_i64().unwrap() as i32;
            let actual = instance
                .supply_net
                .teams
                .iter()
                .find(|n| n.team == team)
                .unwrap();
            assert_eq!(actual.steps.len(), net["cells"].as_u64().unwrap() as usize);
            assert_eq!(
                hash(actual.steps.iter().flat_map(|n| n.to_le_bytes())),
                net["stepsHash"],
                "case{i} steps team{team}"
            );
            assert_eq!(
                hash(actual.closed.iter().copied()),
                net["closedHash"],
                "case{i} closed"
            );
        }
        let state = instance.convoys.as_ref().unwrap();
        let stores = state
            .stores
            .iter()
            .map(|s| {
                json!([
                    s.id.key(),
                    s.owner,
                    [bits(s.at.x), bits(s.at.z)],
                    s.source,
                    s.entity,
                    stock(s.capacity),
                    s.buckets
                        .iter()
                        .map(|(p, b)| json!([p, stock(*b)]))
                        .collect::<Vec<_>>(),
                    s.donations
                        .iter()
                        .map(|(p, rs)| json!([
                            p,
                            rs.iter()
                                .map(|(q, b)| json!([q, stock(*b)]))
                                .collect::<Vec<_>>()
                        ]))
                        .collect::<Vec<_>>(),
                    s.active,
                    s.support,
                    s.point
                ])
            })
            .collect::<Vec<_>>();
        assert_eq!(json!(stores), row["stores"], "case{i} stores");
        assert_eq!(
            json!(
                state
                    .jobs
                    .iter()
                    .map(|(p, n)| json!([p, n]))
                    .collect::<Vec<_>>()
            ),
            row["jobs"],
            "case{i} jobs"
        );
        assert_eq!(json!(state.next_schedule), row["nextSchedule"]);
        assert_eq!(
            json!(
                state
                    .next_spawn
                    .iter()
                    .map(|(p, n)| json!([p, n]))
                    .collect::<Vec<_>>()
            ),
            row["nextSpawn"]
        );
        assert_eq!(
            json!(state.trucks.keys().copied().collect::<Vec<_>>()),
            row["trucks"]
        );
        let misc = json!({"losses":state.losses.iter().map(|(p,ns)|json!([p,ns])).collect::<Vec<_>>(),"events":[],"alerts":[],"nextEvent":state.next_event,"refreshQueue":state.refresh_queue,"scheduleQueue":state.schedule_queue,"scheduled":state.scheduled,"assignments":[],"routed":state.routed});
        assert_eq!(misc, row["misc"], "case{i} schedule fields");
        assert!(state.events.is_empty() && state.alerts.is_empty() && state.assignments.is_empty());
    }
}
