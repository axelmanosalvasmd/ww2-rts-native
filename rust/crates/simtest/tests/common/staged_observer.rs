use serde_json::{Value, json};
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{Point, Wreck},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    random::MatchRandom,
    route_knowledge::{LiveRouting, RouteKnowledge, RouteQuery},
    unit_spawn,
    weather::WeatherChoice,
    world::CellChange,
};
pub(crate) fn idx(v: &Value) -> usize {
    v.as_u64().unwrap() as usize
}
pub(crate) fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
pub(crate) fn bits(v: f64) -> String {
    format!("{:016x}", v.to_bits())
}
pub(crate) fn runs(values: impl IntoIterator<Item = Value>) -> Value {
    let mut out: Vec<[Value; 2]> = Vec::new();
    for value in values {
        if let Some(last) = out.last_mut().filter(|r| r[0] == value) {
            last[1] = json!(idx(&last[1]) + 1);
        } else {
            out.push([value, json!(1)]);
        }
    }
    json!(out)
}
pub(crate) fn options(c: Option<&Value>) -> MatchOptions {
    MatchOptions {
        seed: c.map_or(17, |c| c["seed"].as_u64().unwrap()),
        mode: if c.is_some() {
            Mode::World
        } else {
            Mode::Conquest
        },
        players: c.map_or_else(
            || {
                vec![
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
                ]
            },
            |c| {
                c["players"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| PlayerConfig {
                        name: p["name"].as_str().unwrap().into(),
                        team: p["team"].as_i64().unwrap() as i32,
                        faction: Faction::try_from(idx(&p["faction"]) as u8).unwrap(),
                    })
                    .collect()
            },
        ),
        shuffle: c.is_some_and(|c| c["settings"]["shuffle"].as_bool().unwrap()),
        army: ArmySize::Standard,
        weather: c.map_or(WeatherChoice::Map, |c| {
            WeatherChoice::from_reference(c["settings"]["weather"].as_str())
        }),
        weather_seed: c.and_then(|c| c["weatherSeed"].as_u64().map(|v| v as u32)),
        living_weather: c.is_none_or(|c| c["settings"]["living"].as_bool().unwrap()),
        map_key: None,
        tech: c.is_some_and(|c| c["settings"]["tech"].as_bool().unwrap()),
        supply: c.is_some(),
        logistics: false,
        sandbox: false,
    }
}
pub(crate) fn load(c: &Value) -> Match {
    let starts: Value = serde_json::from_str(include_str!("../fixtures/world-start.json")).unwrap();
    let start = (!c["startIndex"].is_null()).then(|| &starts["cases"][idx(&c["startIndex"])]);
    let mut map = start.map_or_else(|| c["map"].clone(), |c| c["map"].clone());
    if let Some(runs) = map["world"]["regionMapRuns"].as_array() {
        let rows: Vec<_> = runs
            .iter()
            .flat_map(|r| std::iter::repeat_n(r[0].clone(), idx(&r[1])))
            .collect();
        map["world"]
            .as_object_mut()
            .unwrap()
            .remove("regionMapRuns");
        map["world"]["regionMap"] = json!(rows);
    }
    let map =
        ww2_core::map::AuthoritativeMap::from_json(&serde_json::to_vec(&map).unwrap()).unwrap();
    let o = options(start);
    if start.is_some() {
        Match::new_world(&map, o.clone(), MatchRandom::new(o.seed)).unwrap()
    } else {
        Match::new(&map, o).unwrap()
    }
}
pub(crate) fn view(
    knowledge: &RouteKnowledge,
    q: RouteQuery<'_>,
    live: LiveRouting<'_>,
    wrecks: &[Wreck],
) -> Value {
    let n = q.navigation();
    let count = n.width * n.height;
    json!({"version":n.versions.knowledge,"chars":runs((0..count).map(|c|json!(char::from(knowledge.cell(c).unwrap().character).to_string()))),"flags":runs(n.flags.iter().map(|v|json!(v))),"height":runs(n.heights.unwrap().iter().map(|v|json!(v))),"fires":live.fires.iter().enumerate().filter_map(|(c,v)|v.then_some(c)).collect::<Vec<_>>(),"bunkers":live.bunkers.iter().map(|p|[bits(p.x),bits(p.z)]).collect::<Vec<_>>(),"wrecks":wrecks.iter().map(|w|json!([w.id.unwrap(),bits(w.x),bits(w.z),w.moving])).collect::<Vec<_>>(),"wet":live.wetness})
}
pub(crate) fn setup(m: &mut Match, c: &Value) {
    let s = &c["setup"];
    m.world.game.units.clear();
    let building = &s["building"];
    let kind = m.rules.kind(building["kind"].as_str().unwrap()).unwrap();
    let id = idx(&building["id"]) as u32;
    let mut u = unit_spawn::initial_unit(
        &m.rules,
        Catalog::compiled().unwrap(),
        kind,
        id,
        0,
        point(&building["at"]),
    )
    .unwrap();
    u.built = Some(1.0);
    m.world.game.units.insert(id, u);
    if !s["scout"].is_null() {
        let scout = &s["scout"];
        let u = unit_spawn::initial_unit(
            &m.rules,
            Catalog::compiled().unwrap(),
            m.rules.kind(scout["kind"].as_str().unwrap()).unwrap(),
            idx(&scout["id"]) as u32,
            0,
            point(&scout["at"]),
        )
        .unwrap();
        m.world.game.units.insert(u.id, u);
    }
    m.next_unit_id = idx(&c["id"]) as u32;
    m.grid.rebuild(&m.world.game.units);
    for change in s["changes"].as_array().unwrap() {
        m.world
            .set_cell(
                idx(&change["c"]),
                CellChange {
                    ch: change["ch"].as_str().unwrap().as_bytes()[0],
                    height: None,
                    state: None,
                },
            )
            .unwrap();
    }
    let w = &s["wreck"];
    m.world.game.wrecks.push(Wreck {
        id: Some(idx(&w["id"]) as u32),
        x: w["x"].as_f64().unwrap(),
        z: w["z"].as_f64().unwrap(),
        cell: None,
        moving: false,
    });
    if s["warm"] == "fire" {
        m.world.fires.insert(idx(&s["fireCell"]), 20.0);
    }
}
