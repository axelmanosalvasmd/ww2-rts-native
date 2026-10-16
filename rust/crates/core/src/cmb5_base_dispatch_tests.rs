//! Core base dispatch keeps canonical wallets, jobs and rally state usable by later ticks.
use crate::{
    economy::{Faction, Mode, Technology},
    economy_state::ArmySize,
    game::{CELL, Point},
    map::AuthoritativeMap,
    match_commands::{Command, Denial},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    orders::OrderKey,
    random::MatchRandom,
    recipient_snapshot::{SnapshotCache, SnapshotError},
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};
use serde_json::{Value, json};

fn options(mode: Mode) -> MatchOptions {
    MatchOptions {
        seed: 701,
        mode,
        players: (0..2)
            .map(|slot| PlayerConfig {
                name: format!("p{slot}"),
                team: slot,
                faction: if slot == 0 {
                    Faction::Usa
                } else {
                    Faction::Germany
                },
            })
            .collect(),
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
    }
}
fn scene(tech: bool) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Base dispatch", "w":60,"h":60,"rows":vec![".".repeat(60);60],
            "heights":vec!["0".repeat(60);60],
            "spawns":[{"x":4,"y":4},{"x":55,"y":55}],"points":[{"x":30,"y":30}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut config = options(Mode::Conquest);
    config.tech = tech;
    Match::new(&map, config).unwrap()
}
fn dispatch(m: &mut Match, owner: usize, raw: Value) -> Option<Denial> {
    m.execute_command(owner, &Command::from_json(&raw).unwrap(), false)
        .unwrap()
}
fn own(m: &Match, owner: usize, kind: &str) -> u32 {
    m.world
        .game
        .units
        .values()
        .find(|u| u.owner == owner && m.rules.kinds[u.kind].name == kind)
        .unwrap()
        .id
}
fn worker(m: &mut Match) -> u32 {
    let id = m.next_unit_id;
    let staged = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        0,
        m.rules.kind("engineer").unwrap(),
        id,
        m.world.game.units.len() as u32,
        m.start.players[0].spawn,
        Some(Point { x: 28.0, z: 24.0 }),
    )
    .unwrap();
    prepare_publication(
        PublicationParts {
            units: &mut m.world.game.units,
            grid: &mut m.grid,
            controls: &mut m.commands.units,
            recruitment: &mut m.recruit_metadata,
            next_id: &mut m.next_unit_id,
        },
        &m.rules,
        2,
        vec![staged],
    )
    .unwrap()
    .publish_with(|u, _| {
        m.combat.initialize(u.id);
        m.firing.initialize(u.id);
        m.abilities.initialize(u.id);
        m.recovery.initialize(u.id);
    });
    id
}
fn enqueue(m: &mut Match, tick: u64, sequence: u64, raw: Value) {
    m.enqueue_command(
        OrderKey {
            tick,
            issuer: 0,
            sequence,
        },
        Command::from_json(&raw).unwrap(),
    )
    .unwrap();
}

#[test]
fn cmb149_core_base_dispatch_survives_ticks_and_recipient_snapshots() {
    let mut m = scene(false);
    crate::cmb5_match_tests::prepare_finish(&mut m);
    let crew = worker(&mut m);
    let maker = own(&m, 0, "barracks");
    let enemy = own(&m, 1, "hq");
    let mut oversized = vec![-1; 50.max(m.world.game.units.len())];
    oversized.push(i64::from(maker));
    assert_eq!(
        dispatch(
            &mut m,
            0,
            json!({"t":"rally","ids":oversized,"x":35,"z":40})
        ),
        Some(Denial::Unseen)
    );
    assert_eq!(m.building_rallies[&maker], None);
    m.start.economy.players[0].mp = 5000.0;
    assert_eq!(
        dispatch(&mut m, 0, json!({"t":"rally","id":enemy,"x":35,"z":40})),
        Some(Denial::Unseen)
    );
    assert_eq!(
        dispatch(&mut m, 0, json!({"t":"rally","ids":[crew],"x":35,"z":40})),
        Some(Denial::Needs)
    );
    assert_eq!(
        dispatch(&mut m, 0, json!({"t":"rally","x":-1,"z":40})),
        Some(Denial::Blocked)
    );
    assert_eq!(
        dispatch(
            &mut m,
            0,
            json!({"t":"cancelProduction","id":maker,"job":1})
        ),
        Some(Denial::Needs)
    );
    assert_eq!(
        dispatch(&mut m, 0, json!({"t":"tech","kind":"tier"})),
        Some(Denial::Blocked)
    );
    let site = m.next_unit_id;
    assert_eq!(
        dispatch(
            &mut m,
            0,
            json!({"t":"build","kind":"flakpos","ids":[crew],"x":32,"z":24})
        ),
        None
    );
    assert_eq!(
        dispatch(&mut m, 0, json!({"t":"assist","ids":[crew],"id":site})),
        None
    );
    let charged = 5000.0 - m.start.economy.players[0].mp;
    assert!(charged > 0.0);
    assert_eq!(
        dispatch(&mut m, 1, json!({"t":"cancel","id":site})),
        Some(Denial::Unseen)
    );
    enqueue(&mut m, 1, 0, json!({"t":"cancel","id":site}));
    enqueue(
        &mut m,
        1,
        1,
        json!({"t":"rally","ids":[maker,crew],"x":35,"z":40}),
    );
    enqueue(&mut m, 1, 2, json!({"t":"rally","x":44,"z":40}));
    let report = m.advance_tick().unwrap();
    {
        let cache = SnapshotCache::new(&m).unwrap();
        for slot in 0..m.world.game.players.len() {
            cache.recipient(slot).unwrap();
        }
    }
    assert!(report.commands.iter().all(|r| r.denial.is_none()));
    assert!(m.start.economy.players[0].mp >= 5000.0 - charged * 0.25);
    assert!(!m.world.game.units.contains_key(&site));
    assert_eq!(m.commands.units[&crew].build_target, None);
    assert_eq!(m.building_rallies[&maker], Some(Point { x: 35.0, z: 40.0 }));
    assert_eq!(m.player_rallies[0], Some(Point { x: 44.0, z: 40.0 }));
    let born = m.next_unit_id;
    assert_eq!(
        dispatch(&mut m, 0, json!({"t":"buy","unit":"rifle","from":maker})),
        None
    );
    assert_eq!(m.recruit_metadata[&born].building, maker);
    for _ in 0..5 {
        m.advance_tick().unwrap();
        let cache = SnapshotCache::new(&m).unwrap();
        for owner in 0..2 {
            cache.recipient(owner).unwrap();
        }
    }
    assert_eq!(m.tick_state.fault(), None);
    let projection = SnapshotCache::new(&m).unwrap().recipient(0).unwrap();
    assert_eq!(projection.fields["rally"][0].as_f64(), Some(44.0));
    assert_eq!(projection.fields["rally"][1].as_f64(), Some(40.0));
    assert!(
        projection.fields["queues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r[0].as_u64() == Some(u64::from(maker))
                && r[2].as_f64() == Some(35.0)
                && r[3].as_f64() == Some(40.0))
    );
    crate::cmb5_match_tests::through_finish(&mut m, |_, _| {});
}

fn world() -> Match {
    let regions = (0..64)
        .map(|i| {
            let x = i % 8 * 64;
            let y = i / 8 * 64;
            let mut r = json!({"id":i,"x":x+32,"y":y+32,"bounds":[x,y,x+64,y+64],"kind":"rural"});
            if i == 0 {
                r["home"] = json!(0);
            }
            if i == 63 {
                r["home"] = json!(1);
            }
            r
        })
        .collect::<Vec<_>>();
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"World base dispatch","w":512,"h":512,"rows":vec![".".repeat(512);512],
            "spawns":[{"x":32,"y":32},{"x":480,"y":480}],"points":[],
            "world":{"size":"huge","total":64,"regionCells":64,"regions":regions}
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new_world(&map, options(Mode::World), MatchRandom::new(701)).unwrap()
}

#[test]
fn cmb149_world_waiting_job_cancellation_refunds_and_keeps_core_ticks_live() {
    let mut m = world();
    let maker = own(&m, 0, "hq");
    m.start.economy.players[0].mp = 5000.0;
    m.start.economy.players[0].fuel = 5000.0;
    for _ in 0..3 {
        assert_eq!(
            dispatch(&mut m, 0, json!({"t":"buy","unit":"rifle","from":maker})),
            None
        );
    }
    let jobs = m.production.building(maker).unwrap().jobs().to_vec();
    assert_eq!(jobs.len(), 3);
    assert_eq!(
        dispatch(
            &mut m,
            0,
            json!({"t":"cancelProduction","id":maker,"job":jobs[0].id})
        ),
        Some(Denial::NotWaiting)
    );
    assert_eq!(
        dispatch(
            &mut m,
            1,
            json!({"t":"cancelProduction","id":maker,"job":jobs[1].id})
        ),
        Some(Denial::Unseen)
    );
    assert_eq!(
        dispatch(
            &mut m,
            0,
            json!({"t":"cancelProduction","id":maker,"job":1.5})
        ),
        Some(Denial::Unseen)
    );
    assert_eq!(
        dispatch(
            &mut m,
            0,
            json!({"t":"cancelProduction","id":maker,"job":-1})
        ),
        Some(Denial::NotWaiting)
    );
    let before = m.start.economy.players[0].clone();
    let spending = m.story[0].mp_spent;
    assert_eq!(
        dispatch(
            &mut m,
            0,
            json!({"t":"cancelProduction","id":maker,"job":jobs[1].id})
        ),
        None
    );
    assert_eq!(m.start.economy.players[0].mp, before.mp + jobs[1].charge.mp);
    assert_eq!(
        m.start.economy.players[0].fuel,
        before.fuel + jobs[1].charge.fuel
    );
    assert_eq!(m.story[0].mp_spent, spending - jobs[1].charge.mp);
    enqueue(
        &mut m,
        1,
        0,
        json!({"t":"cancelProduction","id":maker,"job":jobs[2].id}),
    );
    enqueue(
        &mut m,
        1,
        1,
        json!({"t":"rally","id":maker,"x":-10,"z":9999}),
    );
    let report = m.advance_tick().unwrap();
    assert!(report.commands.iter().all(|r| r.denial.is_none()));
    assert_eq!(m.production.building(maker).unwrap().jobs().len(), 1);
    assert_eq!(
        m.story[0].mp_spent,
        spending - jobs[1].charge.mp - jobs[2].charge.mp
    );
    assert_eq!(
        m.building_rallies[&maker],
        Some(Point {
            x: 0.0,
            z: 512.0 * CELL
        })
    );
    for _ in 0..5 {
        m.advance_tick().unwrap();
    }
    assert_eq!(m.tick_state.fault(), None);
    // The existing snapshot mode guard still rejects World before any recipient state is emitted.
    assert!(matches!(
        SnapshotCache::new(&m),
        Err(SnapshotError::UnsupportedMode)
    ));
}

#[test]
fn cmb149_research_dispatch_charges_and_keeps_lab_visible_during_ticks() {
    let mut m = scene(true);
    crate::cmb5_match_tests::prepare_finish(&mut m);
    m.start.economy.players[0].mp = 5000.0;
    let spending = m.story[0].mp_spent;
    assert_eq!(dispatch(&mut m, 0, json!({"t":"tech","kind":"tier"})), None);
    let charged = 5000.0 - m.start.economy.players[0].mp;
    assert!(charged > 0.0);
    assert_eq!(m.story[0].mp_spent, spending + charged);
    let lab = m.production.player(0).unwrap().jobs();
    assert_eq!(lab.len(), 1);
    assert_eq!(lab[0].kind, Technology::Tier);
    assert_eq!(lab[0].progress, 0.0);
    assert_eq!(
        dispatch(&mut m, 0, json!({"t":"tech","kind":"tier"})),
        Some(Denial::Maximum)
    );
    for _ in 0..5 {
        m.advance_tick().unwrap();
        let cache = SnapshotCache::new(&m).unwrap();
        for owner in 0..2 {
            cache.recipient(owner).unwrap();
        }
    }
    assert!(m.production.player(0).unwrap().jobs()[0].progress > 0.0);
    assert_eq!(m.tick_state.fault(), None);
    crate::cmb5_match_tests::through_finish(&mut m, |_, _| {});
}
