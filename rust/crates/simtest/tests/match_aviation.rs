use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{PlaneState, Point},
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};
fn fixture() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("fixtures/match-aviation.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn bits(value: f64) -> Value {
    json!(format!("{:016x}", value.to_bits()))
}
fn make(map: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(map).unwrap()).unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 811,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "A".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "B".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}
#[test]
fn explicit_aircraft_factory_matches_original_spawn_and_preserves_ordinary_policy() {
    let f = fixture();
    for row in f["constructors"].as_array().unwrap() {
        let mut m = make(&row["map"]);
        m.world.game.units.clear();
        m.commands.units.clear();
        m.recruit_metadata.clear();
        m.next_unit_id = 1000;
        m.grid.rebuild(&m.world.game.units);
        let owner = row["owner"].as_u64().unwrap() as usize;
        let kind = m.rules.kind(row["kind"].as_str().unwrap()).unwrap();
        let index = row["index"].as_u64().unwrap() as u32;
        let spawn = Point {
            x: row["spawn"]["x"].as_f64().unwrap(),
            z: row["spawn"]["z"].as_f64().unwrap(),
        };
        let base = Point {
            x: row["base"]["x"].as_f64().unwrap(),
            z: row["base"]["z"].as_f64().unwrap(),
        };
        assert!(
            StagedUnit::ordinary(
                &m.world.game,
                &m.rules,
                owner,
                kind,
                1000,
                index,
                spawn,
                None
            )
            .is_err()
        );
        let draft = StagedUnit::aircraft(
            &m.world.game,
            &m.rules,
            owner,
            kind,
            1000,
            index,
            spawn,
            base,
        )
        .unwrap();
        let u = &draft.unit;
        let c = &draft.control;
        assert!(c.orders.is_empty());
        let got = json!({"id":u.id,"owner":u.owner,"kind":m.rules.kinds[u.kind].name,"x":bits(u.x),"z":bits(u.z),"rot":bits(u.rot),"hp":bits(u.hp),"supp":bits(u.supp),"air":if u.air==Some(PlaneState::Base){"base"}else{"unexpected"},"still":bits(u.still.unwrap()),"vx":bits(u.vx.unwrap()),"vz":bits(u.vz.unwrap()),"moveSpeed":bits(u.move_speed),"travelDir":bits(u.travel_dir.unwrap()),"path":u.path.iter().map(|p|json!({"x":p.x,"z":p.z})).collect::<Vec<_>>(),"orders":Vec::<Value>::new(),"auto":c.auto_cast,"autoRetreat":c.auto_retreat,"aim":bits(c.aim),"cooldown":bits(0.0),"retarget":bits(0.0),"targetId":0,"hitBy":0,"hitAt":bits(-1e9)});
        assert_eq!(got, row["expected"], "{}", row["kind"]);
        let ids = prepare_publication(
            PublicationParts {
                units: &mut m.world.game.units,
                grid: &mut m.grid,
                controls: &mut m.commands.units,
                recruitment: &mut m.recruit_metadata,
                next_id: &mut m.next_unit_id,
            },
            &m.rules,
            2,
            vec![draft],
        )
        .unwrap()
        .publish_with(|u, _| {
            m.combat.initialize(u.id).hit_by = Some(0);
            m.firing.initialize(u.id);
        });
        assert_eq!(ids, [1000]);
        assert_eq!(m.next_unit_id, 1001);
        assert_eq!(m.world.game.units[&1000].at(), base);
    }
    assert_eq!(f["constructors"].as_array().unwrap().len(), 18);
}
