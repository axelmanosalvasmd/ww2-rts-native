//! Second owned MOV window. Register after the first frozen batch is tested.
use super::*;
use crate::match_support::{FighterCover, SupportDenial, SupportEvent};

fn support(m: &mut Match, owner: usize, kind: &str, at: Point) {
    for cd in m.start.players[owner].support.values_mut() {
        *cd = 0.0;
    }
    assert_eq!(m.admit_support(owner, kind, at, Some(0.0)).unwrap(), None);
}
fn strikes(m: &mut Match, n: usize) -> Vec<SupportEvent> {
    let mut events = Vec::new();
    for _ in 0..n {
        let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
        m.step_support_strikes(&ids, 0.05).unwrap();
        events.extend(m.support.take_events());
    }
    events
}
// MOV-91: reference/ww2-rts/shared/sim.js:3680-3688.
#[test]
fn mov_91_support_charges_once_and_refuses_cooldown_and_short_wallet() {
    let mut m = scene();
    let before = m.start.economy.players[0].mp;
    support(&mut m, 0, "recon", p(80.0, 80.0));
    assert_eq!(m.start.economy.players[0].mp, before - 60.0);
    assert_eq!(m.start.players[0].support["recon"], 45.0);
    let paid = m.start.economy.players[0].mp;
    assert_eq!(
        m.admit_support(0, "recon", p(80.0, 80.0), None).unwrap(),
        Some(SupportDenial::Cooldown)
    );
    assert_eq!(m.start.economy.players[0].mp, paid);
    m.start.economy.players[0].mp = 0.0;
    assert_eq!(
        m.admit_support(0, "bombing", p(80.0, 80.0), None).unwrap(),
        Some(SupportDenial::Manpower)
    );
    assert_eq!(
        m.admit_support(0, "toString", p(80.0, 80.0), None).unwrap(),
        Some(SupportDenial::Blocked)
    );
}
// MOV-92: reference/ww2-rts/shared/sim.js:5935,6052,5249.
#[test]
fn mov_92_incoming_strike_announced_to_both_ordinary_players() {
    let mut m = scene();
    support(&mut m, 0, "artillery", p(80.0, 80.0));
    let cache = crate::recipient_snapshot::SnapshotCache::new(&m).unwrap();
    for owner in 0..2 {
        let state = cache.recipient(owner).unwrap();
        assert_eq!(
            state.fields["strikes"],
            json!([["artillery", 80.0, 80.0, 0.0, 5.0, 0]])
        );
    }
}
// MOV-94: reference/ww2-rts/shared/sim.js:4692-4712.
#[test]
fn mov_94_fighter_cover_intercepts_only_one_same_tick_plane() {
    let mut m = scene();
    m.support.covers.push(FighterCover {
        team: 1,
        owner: 1,
        at: p(80.0, 80.0),
        radius: 40.0,
        remaining: 60.0,
    });
    support(&mut m, 0, "strafe", p(80.0, 80.0));
    support(&mut m, 0, "dive", p(80.0, 80.0));
    // Equal declared arrival times isolate ordered single-use interception.
    for s in &mut m.world.game.strikes {
        s.runtime.as_mut().unwrap().delay = 0.05;
    }
    let events = strikes(&mut m, 1);
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SupportEvent::ShotDown { fighters: true, .. }))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SupportEvent::Bomb(_)))
            .count(),
        1
    );
    assert!(m.support.covers.iter().all(|cover| cover.remaining <= 0.0));
}
// MOV-95: reference/ww2-rts/shared/sim.js:4600-4602,5269.
#[test]
fn mov_95_recon_strip_reveals_inside_and_expires() {
    let mut m = scene();
    let inside = add(&mut m, "rifle", 1, p(100.0, 85.0));
    let outside = add(&mut m, "rifle", 1, p(80.0, 105.0));
    support(&mut m, 0, "recon", p(80.0, 80.0));
    strikes(&mut m, 61);
    crate::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    assert!(m.world.game.players[0].sees(inside));
    assert!(!m.world.game.players[0].sees(outside));
    strikes(&mut m, 301);
    crate::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    assert!(!m.world.game.players[0].sees(inside));
}
// MOV-97: reference/ww2-rts/shared/sim.js:5292-5298.
#[test]
fn mov_97_strafe_hits_both_sides_inside_strip_once() {
    let mut m = scene();
    let friend = add(&mut m, "rifle", 0, p(80.0, 80.0));
    let foe = add(&mut m, "rifle", 1, p(85.0, 80.0));
    let outside = add(&mut m, "rifle", 1, p(80.0, 90.0));
    let hp = [friend, foe, outside].map(|id| m.world.game.units[&id].hp);
    support(&mut m, 0, "strafe", p(80.0, 80.0));
    strikes(&mut m, 101);
    assert!(m.world.game.units[&friend].hp < hp[0]);
    assert!(m.world.game.units[&foe].hp < hp[1]);
    assert_eq!(m.world.game.units[&outside].hp, hp[2]);
    let once = m.world.game.units[&foe].hp;
    strikes(&mut m, 10);
    assert_eq!(m.world.game.units[&foe].hp, once);
}
// MOV-99: reference/ww2-rts/shared/sim.js:5279-5285.
#[test]
fn mov_99_bombing_delivers_six_ordered_bombs_across_run() {
    let mut m = scene();
    support(&mut m, 0, "bombing", p(80.0, 80.0));
    let bombs = strikes(&mut m, 160)
        .into_iter()
        .filter_map(|event| {
            if let SupportEvent::Bomb(at) = event {
                Some(at)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(bombs.len(), 6);
    assert!(bombs.windows(2).all(|pair| pair[0].x < pair[1].x));
    assert!(bombs.iter().all(|at| (at.z - 80.0).abs() <= 1.5));
    assert!((bombs[0].x - 60.0).abs() < 1e-9);
    assert!((bombs[5].x - 100.0).abs() < 1e-9);
}
// MOV-100: reference/ww2-rts/shared/sim.js:5251-5255.
#[test]
fn mov_100_dive_bomb_hits_the_spot_and_damages_tank() {
    let mut m = scene();
    let id = add(&mut m, "tank", 1, p(80.0, 80.0));
    let hp = m.world.game.units[&id].hp;
    support(&mut m, 0, "dive", p(80.0, 80.0));
    let events = strikes(&mut m, 101);
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e,SupportEvent::Bomb(at) if *at==p(80.0,80.0)))
            .count(),
        1
    );
    assert!(m.world.game.units[&id].hp < hp);
    assert!(m.world.game.strikes.is_empty());
}
// MOV-101: reference/ww2-rts/shared/sim.js:5256-5263.
#[test]
fn mov_101_paratroopers_reserve_and_publish_three_native_squads() {
    let mut m = scene();
    add(&mut m, "rifle", 0, p(80.0, 80.0));
    let first = m.next_unit_id;
    support(&mut m, 0, "para", p(80.0, 80.0));
    let events = strikes(&mut m, 121);
    let born = m
        .world
        .game
        .units
        .range(first..)
        .map(|(_, u)| u)
        .collect::<Vec<_>>();
    assert_eq!(born.len(), 3);
    assert_eq!(
        born.iter()
            .map(|u| m.rules.kinds[u.kind].name.as_str())
            .collect::<Vec<_>>(),
        ["rifle", "rifle", "mg"]
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SupportEvent::Chutes(_)))
            .count(),
        3
    );
    for u in born {
        assert_eq!(u.owner, 0);
        assert!(distance(u.at(), p(80.0, 80.0)) <= 8.0);
        assert_eq!(m.world.game.flags_at(u.x, u.z) & MOVE, 0);
    }
    let mut pending = scene();
    add(&mut pending, "rifle", 0, p(80.0, 80.0));
    for _ in 0..3 {
        support(&mut pending, 0, "para", p(80.0, 80.0));
    }
    pending.start.players[0].support.insert("para".into(), 0.0);
    let money = pending.start.economy.players[0].mp;
    assert_eq!(
        pending
            .admit_support(0, "para", p(80.0, 80.0), Some(0.0))
            .unwrap(),
        Some(SupportDenial::Population)
    );
    assert_eq!(pending.start.economy.players[0].mp, money);
    assert_eq!(pending.world.game.strikes.len(), 3);
}
fn naval_scene() -> Match {
    let rows = (0..80)
        .map(|_| format!("{}{}{}", ".".repeat(20), "W".repeat(40), ".".repeat(20)))
        .collect::<Vec<_>>();
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({"name":"Native naval MOV","w":80,"h":80,"rows":rows,
        "naval":true,"spawns":[{"x":8,"y":40},{"x":72,"y":40}],"points":[{"x":10,"y":40}]}))
        .unwrap(),
    )
    .unwrap();
    let mut m = scene();
    m.world = World::from_map(&map).unwrap();
    m.world.game.players = (0..2)
        .map(|slot| Player {
            slot,
            team: slot as i32,
            visible: vec![],
        })
        .collect();
    m.grid.rebuild(&m.world.game.units);
    m.initialize_recipient_routing().unwrap();
    m
}
// MOV-103: reference/ww2-rts/shared/sim.js:2665-2678.
#[test]
fn mov_103_naval_land_and_shoal_masks_keep_deep_ships_off_coast() {
    let mut m = naval_scene();
    let flags = &m.world.game.flags;
    assert_ne!(flags[40 * 80 + 10] & crate::game::LAND, 0);
    assert_eq!(flags[40 * 80 + 21] & crate::game::LAND, 0);
    assert_ne!(flags[40 * 80 + 21] & crate::game::SHOAL, 0);
    assert_eq!(
        flags[40 * 80 + 40] & (crate::game::LAND | crate::game::SHOAL),
        0
    );
    let destroyer = &m.rules.kinds[m.rules.kind("destroyer").unwrap()];
    assert_eq!(destroyer.block(), crate::game::LAND | crate::game::SHOAL);
    assert_eq!(
        m.rules.kinds[m.rules.kind("gunboat").unwrap()].block(),
        crate::game::LAND
    );
    let shoal = m.world.game.flags[40 * 80 + 21] & crate::game::SHOAL;
    m.world.set_cell(40 * 80 + 19, b'W').unwrap();
    assert_eq!(m.world.game.flags[40 * 80 + 21] & crate::game::SHOAL, shoal);
}
// MOV-104: reference/ww2-rts/shared/sim.js:263-280,1773-1808.
#[test]
fn mov_104_boat_scatter_finds_water_and_profiles_match_table() {
    let m = naval_scene();
    let mut placer = SpawnPlacer::new();
    for (name, speed, radius, hull) in [
        ("lcvp", 7.0, 2.5, 0.0),
        ("gunboat", 10.0, 3.0, 11.0),
        ("destroyer", 6.0, 6.0, 52.0),
    ] {
        let k = &m.rules.kinds[m.rules.kind(name).unwrap()];
        assert_eq!((k.speed, k.radius, k.hull), (speed, radius, hull));
        let at = placer
            .scatter_position(
                crate::match_runtime::terrain_view(&m.world.game),
                k,
                p(16.0, 80.0),
                0,
            )
            .unwrap();
        assert_eq!(m.world.game.flags[at.cell] & k.block(), 0);
        assert_eq!(m.world.game.chars[at.cell], b'W');
    }
}
// MOV-105: reference/ww2-rts/shared/sim.js:5490; movement.js:72-88.
#[test]
fn mov_105_boat_walks_water_legs_without_local_traffic() {
    let mut m = naval_scene();
    let boat = add(&mut m, "gunboat", 0, p(80.0, 80.0));
    let blocker = add(&mut m, "gunboat", 0, p(81.0, 80.0));
    route(&mut m, boat, p(100.0, 80.0));
    let before = m.world.game.units[&boat].at();
    let moved = movement::move_unit(&mut m.world.game, &m.rules, &mut m.grid, boat, 1.0).unwrap();
    assert_eq!(moved.speed, 10.0);
    assert!((distance(before, m.world.game.units[&boat].at()) - 0.5).abs() < 1e-9);
    assert!(m.world.game.units[&boat].traffic.is_none());
    assert_eq!(m.world.game.units[&blocker].at(), p(81.0, 80.0));
}
// MOV-106: reference/ww2-rts/shared/debris-motion.js:1-58; sim.js:5125-5183.
#[test]
fn mov_106_wreck_body_settles_within_two_metres() {
    use crate::environment_debris::{DebrisBody, DebrisInput, DebrisKind, Environment};
    struct Ground;
    impl Environment for Ground {
        fn ground(&mut self, _x: f64, _z: f64) -> f64 {
            0.0
        }
    }
    let mut body = DebrisBody::new(DebrisInput {
        id: "wreck-1".into(),
        kind: DebrisKind::Wreck,
        x: 60.0,
        z: 60.0,
        mass: 14000.0,
        radius: 3.0,
        impulse: 4.0,
        ..Default::default()
    })
    .unwrap();
    assert_eq!((body.gravity, body.drag), (9.81, 3.2));
    for _ in 1..=60 {
        body.step(0.05, &mut Ground).unwrap();
    }
    assert!(body.settled);
    assert!(distance(p(body.x, body.z), p(60.0, 60.0)) <= 2.0 + 1e-9);
}
// MOV-107: reference/ww2-rts/shared/sim.js:5884-5885.
#[test]
fn mov_107_vehicle_rows_publish_rounded_motion_tail() {
    let mut m = scene();
    let id = add(&mut m, "tank", 0, p(60.0, 60.0));
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.move_speed = 3.141;
    u.vx = Some(1.234);
    u.vz = Some(-2.341);
    u.travel_dir = Some(0.741);
    let snapshot = crate::recipient_snapshot::SnapshotCache::new(&m)
        .unwrap()
        .recipient(0)
        .unwrap();
    let row = snapshot.fields["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[0] == id)
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(
        &row[15..],
        &[json!(3.1), json!(1.2), json!(-2.3), json!(0.7)]
    );
}
// MOV-108: reference/ww2-rts/client/squad-motion.js:1-90.
#[test]
fn mov_108_native_drawn_men_geometry_tracks_authoritative_center() {
    let at = p(60.0, 60.0);
    let still = crate::squad_men::drawn_men("rifle", 480.0, 480.0, at, 0.0, p(0.0, 0.0)).unwrap();
    let moving = crate::squad_men::drawn_men("rifle", 480.0, 480.0, at, 0.0, p(3.0, 0.0)).unwrap();
    assert_eq!(still.positions.len(), moving.positions.len());
    for (s, m) in still.positions.iter().zip(&moving.positions) {
        assert!((s.x - m.x - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(s.z, m.z);
    }
    let shifted =
        crate::squad_men::drawn_men("rifle", 480.0, 480.0, p(70.0, 65.0), 0.0, p(0.0, 0.0))
            .unwrap();
    for (old, new) in still.positions.iter().zip(&shifted.positions) {
        assert!((new.x - old.x - 10.0).abs() < 1e-9);
        assert!((new.z - old.z - 5.0).abs() < 1e-9);
    }
}
