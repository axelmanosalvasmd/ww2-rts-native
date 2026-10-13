#[allow(dead_code)]
#[path = "common/staged_observer.rs"]
mod common;
use common::{bits, idx, load};
use serde_json::{Value, json};
use ww2_core::{
    catalog::Catalog,
    game::Point,
    match_runtime::Match,
    match_world_memory::{WorldRoutingContext, prepare_world_routing},
    unit_spawn,
    world::Recipient,
    world_route_retention::prepare_world_route_retention,
};
fn context(m: &Match) -> WorldRoutingContext<'_> {
    let w = m.world_runtime.as_ref().unwrap();
    WorldRoutingContext {
        world: &m.world,
        rules: &m.rules,
        start: &m.start,
        rails: &w.rails,
        regions: &w.regions,
        known_buildings: &w.known_buildings,
        local_defender: &w.local_defender,
    }
}
#[test]
fn prepared_removed_objects_match_original_cached_world_views_without_refreshing_memory() {
    let f: Value =
        serde_json::from_str(include_str!("fixtures/world-route-retention.json")).unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    let mut m = load(&json!({"startIndex":f["startIndex"],"mode":"world"}));
    m.world.game.units.clear();
    for row in f["units"].as_array().unwrap() {
        let u = unit_spawn::initial_unit(
            &m.rules,
            Catalog::compiled().unwrap(),
            m.rules.kind(row["kind"].as_str().unwrap()).unwrap(),
            idx(&row["id"]) as u32,
            idx(&row["owner"]),
            Point {
                x: row["x"].as_f64().unwrap(),
                z: row["z"].as_f64().unwrap(),
            },
        )
        .unwrap();
        m.world.game.units.insert(u.id, u);
    }
    m.world.game.players[0].visible = vec![idx(&f["units"][2]["id"]) as u32];
    m.world.game.players[1].visible = f["units"].as_array().unwrap()[..2]
        .iter()
        .map(|r| idx(&r["id"]) as u32)
        .collect();
    m.grid.rebuild(&m.world.game.units);
    let mut memory = std::mem::take(&mut m.world_runtime.as_mut().unwrap().memory);
    for owner in 0..2 {
        let p = prepare_world_routing(context(&m), &mut memory, owner, None).unwrap();
        let (_, commit) = p.prepare_publication(&mut m.world).unwrap();
        commit.commit();
        assert_eq!(
            json!(
                memory
                    .live_for(owner)
                    .unwrap()
                    .live()
                    .bunkers
                    .iter()
                    .map(|p| [bits(p.x), bits(p.z)])
                    .collect::<Vec<_>>()
            ),
            f["before"][owner]
        );
    }
    let buffers: Vec<_> = m
        .world
        .game
        .players
        .iter()
        .map(|p| {
            let n = memory.team(p.team).unwrap().query().navigation();
            (
                n.flags.as_ptr(),
                n.heights.unwrap().as_ptr(),
                n.versions.knowledge,
            )
        })
        .collect();
    let journal = (m.world.journal_base(), m.world.journal().to_vec());
    let pending = m.world.pending(Recipient::Player(0)).cloned();
    let finals: Vec<_> = f["final"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let mut u = m.world.game.units[&(idx(&r["id"]) as u32)].clone();
            u.hp = r["hp"].as_f64().unwrap();
            u.x = r["x"].as_f64().unwrap();
            u.z = r["z"].as_f64().unwrap();
            u
        })
        .collect();
    let refs: Vec<_> = finals.iter().collect();
    let mut other = load(&json!({"startIndex":f["startIndex"],"mode":"world"}));
    other.world.game.units.clear();
    for row in f["units"].as_array().unwrap() {
        let u = unit_spawn::initial_unit(
            &other.rules,
            Catalog::compiled().unwrap(),
            other.rules.kind(row["kind"].as_str().unwrap()).unwrap(),
            idx(&row["id"]) as u32,
            idx(&row["owner"]),
            Point {
                x: row["x"].as_f64().unwrap(),
                z: row["z"].as_f64().unwrap(),
            },
        )
        .unwrap();
        other.world.game.units.insert(u.id, u);
    }
    assert!(prepare_world_route_retention(context(&other), &mut memory, &refs).is_err());
    for (p, expected) in m.world.game.players.iter().zip(&buffers) {
        let n = memory.team(p.team).unwrap().query().navigation();
        assert_eq!(
            (
                n.flags.as_ptr(),
                n.heights.unwrap().as_ptr(),
                n.versions.knowledge
            ),
            *expected
        );
    }

    let guard = prepare_world_route_retention(context(&m), &mut memory, &refs).unwrap();
    drop(guard);
    for owner in 0..2 {
        assert_eq!(
            json!(
                memory
                    .live_for(owner)
                    .unwrap()
                    .live()
                    .bunkers
                    .iter()
                    .map(|p| [bits(p.x), bits(p.z)])
                    .collect::<Vec<_>>()
            ),
            f["before"][owner]
        );
    }
    assert!(
        prepare_world_route_retention(context(&m), &mut memory, &[&finals[0], &finals[0]]).is_err()
    );
    for field in 0..4 {
        let mut bad = finals[0].clone();
        match field {
            0 => bad.id = 0,
            1 => bad.owner = 1,
            2 => bad.kind = m.rules.kind("rifle").unwrap(),
            _ => bad.hp = f64::NAN,
        };
        assert!(prepare_world_route_retention(context(&m), &mut memory, &[&bad]).is_err());
    }
    let guard = prepare_world_route_retention(context(&m), &mut memory, &refs).unwrap();
    for u in &finals {
        m.world.game.units.remove(&u.id);
    }
    guard.commit();
    for (p, expected) in m.world.game.players.iter().zip(&buffers) {
        let n = memory.team(p.team).unwrap().query().navigation();
        assert_eq!(
            (
                n.flags.as_ptr(),
                n.heights.unwrap().as_ptr(),
                n.versions.knowledge
            ),
            *expected
        );
    }
    assert_eq!(
        (m.world.journal_base(), m.world.journal().to_vec()),
        journal
    );
    assert_eq!(m.world.pending(Recipient::Player(0)), pending.as_ref());
    // Ordinary same-tick querying reads the retained object values, while the
    // original membership still includes each removed cached unit.
    for owner in 0..2 {
        let p = prepare_world_routing(context(&m), &mut memory, owner, None).unwrap();
        assert_eq!(
            json!(
                p.live()
                    .bunkers
                    .iter()
                    .map(|p| [bits(p.x), bits(p.z)])
                    .collect::<Vec<_>>()
            ),
            f["after"][owner]
        );
        drop(p);
    }
}
