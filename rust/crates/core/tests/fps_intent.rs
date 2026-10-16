//! Native behavior checks for the operative core, without room or client substitutes.
use serde_json::{Value, json};
use ww2_core::{
    companion_ray,
    companions::{CombatRequest, Input, WEAPONS},
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{MOVE, Point, SIGHT},
    match_commands::{Command, UnitControl},
    match_firing::LaunchEvent,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};

fn empty_scene() -> Match {
    let map = ww2_core::map::AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Operative intent", "w":64,"h":64,
            "rows":vec![".".repeat(64);64],
            "spawns":[{"x":10,"y":32},{"x":54,"y":32}],
            "points":[{"id":"inert","x":32,"y":32}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "Commander".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "Enemy".into(),
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
    .unwrap();
    // An authored empty scene keeps canonical metadata consistent for real ticks.
    let ids: Vec<_> = m.world.game.units.keys().copied().collect();
    for id in ids {
        m.world.game.units.remove(&id);
        m.commands.units.remove(&id);
        m.firing.remove(id);
        m.combat.remove(id);
        m.abilities.remove(id);
        m.recovery.remove(id);
        m.recruit_metadata.remove(&id);
        m.production.remove_building(id);
        m.logistics.remove(id);
    }
    m.building_cells.clear();
    m.building_rallies.clear();
    m.grid.rebuild(&m.world.game.units);
    m
}

fn scene() -> (Match, u32) {
    let mut m = empty_scene();
    let id = m.join_companion("operative", 0, false).unwrap().unwrap();
    place(&mut m, id, 40.0, 40.0);
    (m, id)
}

fn place(m: &mut Match, id: u32, x: f64, z: f64) {
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = x;
    u.z = z;
    m.grid.update(id, u);
}

fn input(seq: i64, extra: Value) -> Value {
    let mut v = json!({"seq":seq,"forward":0,"strafe":0,"yaw":0,"pitch":0,
        "jump":0,"reload":0,"nade":0,"squad":0});
    v.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    v
}

fn turn(m: &mut Match, id: u32, extra: Value) {
    let seq = m.companions.get("operative").unwrap().last_seq + 1;
    assert!(m.companion_input("operative", &input(seq, extra), false));
    m.world.game.tick += 1;
    match m.step_companion(id, false).unwrap().combat {
        Some(CombatRequest::Fire { unit, input }) => m.fire_companion(unit, &input, false).unwrap(),
        Some(CombatRequest::Grenade { unit, input }) => {
            m.throw_companion(unit, &input, false).unwrap();
        }
        None => {}
    }
}

fn actor(m: &mut Match, kind: &str, owner: usize, x: f64, z: f64) -> u32 {
    let id = m.next_unit_id;
    m.next_unit_id += 1;
    let unit = ww2_core::unit_spawn::initial_unit(
        &m.rules,
        ww2_core::catalog::Catalog::compiled().unwrap(),
        m.rules.kind(kind).unwrap(),
        id,
        owner,
        Point { x, z },
    )
    .unwrap();
    m.commands
        .insert(id, UnitControl::new(&unit, &m.rules).unwrap());
    m.firing.initialize(id);
    m.combat.initialize(id);
    m.abilities.initialize(id);
    m.recovery.initialize(id);
    m.world.game.units.insert(id, unit);
    m.grid.rebuild(&m.world.game.units);
    id
}

#[test]
fn fps06_free_join_has_one_soldier_and_reuses_identity() {
    let mut m = empty_scene();
    let mp = m.start.economy.players[0].mp;
    let id = m.join_companion("operative", 0, false).unwrap().unwrap();
    assert_eq!(m.start.economy.players[0].mp, mp, "first soldier is free");
    let again = m.join_companion("operative", 0, false).unwrap();
    assert_eq!(again, Some(id));
    assert_eq!(m.world.game.units.len(), 1);
    assert_eq!(m.start.economy.players[0].mp, mp);
    let u = &m.world.game.units[&id];
    let c = &m.commands.units[&id];
    let b = &m.companions.soldiers[&id];
    assert_eq!(u.hp, 20.0);
    assert_eq!(m.rules.kinds[u.kind].name, "rifle");
    assert!(u.hold_pos && c.hold_fire && c.operative);
    assert!(!c.auto_retreat && !c.auto_cast);
    assert_eq!((b.mags, b.nades), ([8, 32], 3));
    assert_eq!(m.join_companion("missing", 2, false).unwrap(), None);
    assert_eq!(m.join_companion("finished", 0, true).unwrap(), None);
    m.start.economy.players[0].out = true;
    assert_eq!(m.join_companion("out", 0, false).unwrap(), None);
    assert_eq!(m.companions.records.len(), 1);
}

#[test]
fn fps08_commander_orders_cannot_redirect_the_operative() {
    let (mut m, id) = scene();
    for value in [
        json!({"t":"move","orders":[[id,70,70]]}),
        json!({"t":"amove","orders":[[id,70,70]]}),
        json!({"t":"stop","ids":[id]}),
        json!({"t":"retreat","ids":[id]}),
        json!({"t":"autocast","ids":[id],"on":true}),
    ] {
        let mut before = m.world.game.units[&id].clone();
        let control = m.commands.units[&id].clone();
        m.execute_command(0, &Command::from_json(&value).unwrap(), false)
            .unwrap();
        let mut after = m.world.game.units[&id].clone();
        // Initial pace uses a NaN sentinel. Check its bits before comparing the rest.
        assert_eq!(after.pace.to_bits(), before.pace.to_bits());
        after.pace = 0.0;
        before.pace = 0.0;
        assert_eq!(after, before, "{value}");
        assert_eq!(m.commands.units[&id], control, "{value}");
    }
    m.execute_command(
        0,
        &Command::Destroy {
            ids: vec![id as i64],
            id: None,
        },
        false,
    )
    .unwrap();
    assert!(
        m.world.game.units[&id].hp <= 0.0,
        "destroy retains the prototype exception"
    );
}

#[test]
fn fps09_invalid_frames_do_not_mutate_last_accepted_input() {
    let (mut m, _) = scene();
    assert!(m.companion_input(
        "operative",
        &input(7, json!({"weapon":1,"fire":true})),
        false
    ));
    let before = m.companions.clone();
    for v in [
        input(7, json!({})),
        input(6, json!({})),
        input(8, json!({"forward":1.01})),
        input(8, json!({"strafe":-1.01})),
        input(8, json!({"yaw":10001})),
        input(8, json!({"pitch":1.451})),
        input(8, json!({"yaw":null})),
        input(8, json!({"forward":"1"})),
        input(8, json!({"seq":1.5})),
        input(8, json!({"seq":9_007_199_254_740_992_i64})),
    ] {
        assert!(!m.companion_input("operative", &v, false), "{v}");
        assert_eq!(m.companions, before);
    }
    assert!(m.companion_input(
        "operative",
        &input(
            8,
            json!({
                "weapon":99,"forward":-1,"strafe":1,"yaw":-10000,"pitch":-1.45,
                "fire":1,"ads":"true","crouch":[],"sprint":true
            })
        ),
        false
    ));
    let i = m
        .companions
        .get("operative")
        .unwrap()
        .input
        .as_ref()
        .unwrap();
    assert_eq!(i.weapon, 1);
    assert!(!i.fire && !i.ads && !i.crouch && i.sprint);
    let before = m.companions.clone();
    assert!(!m.companion_input("operative", &input(9, json!({})), true));
    assert_eq!(before, m.companions);
    m.start.economy.players[0].out = true;
    assert!(!m.companion_input("operative", &input(9, json!({})), false));
    m.start.economy.players[0].out = false;
    m.companions.get_mut("operative").unwrap().disconnect();
    assert!(!m.companion_input("operative", &input(9, json!({})), false));
}

#[test]
fn fps11_press_counts_baseline_skip_packets_and_do_not_repeat() {
    let (mut m, id) = scene();
    turn(
        &mut m,
        id,
        json!({"jump":42,"reload":42,"nade":42,"squad":42}),
    );
    assert_eq!(m.companions.soldiers[&id].jy, 0.0);
    assert_eq!(m.companions.soldiers[&id].nades, 3);
    turn(
        &mut m,
        id,
        json!({"jump":45,"nade":45,"reload":45,"squad":45}),
    );
    assert!(m.companions.soldiers[&id].jy > 0.0);
    assert_eq!(m.companions.soldiers[&id].nades, 2);
    for _ in 0..25 {
        turn(
            &mut m,
            id,
            json!({"jump":45,"nade":45,"reload":45,"squad":45}),
        );
    }
    assert_eq!(m.companions.soldiers[&id].jy, 0.0);
    assert_eq!(m.companions.soldiers[&id].nades, 2);
    m.companions.get_mut("operative").unwrap().reconnect(true);
    turn(&mut m, id, json!({"jump":100,"nade":100}));
    assert_eq!(m.companions.soldiers[&id].jy, 0.0);
    assert_eq!(m.companions.soldiers[&id].nades, 2);
    let seq = m.companions.get("operative").unwrap().last_seq + 1;
    assert!(m.companion_input(
        "operative",
        &input(
            seq,
            json!({
                "jump":101,"reload":101,"nade":101,"squad":101
            })
        ),
        false
    ));
    assert_eq!(m.companions.get("operative").unwrap().want, [true; 4]);
}

#[test]
fn fps12_stale_and_offline_input_decelerate_without_firing() {
    let (mut boundary, id) = scene();
    assert!(boundary.companion_input("operative", &input(0, json!({"fire":true})), false));
    boundary.world.game.tick = 5;
    assert!(
        matches!(
            boundary.step_companion(id, false).unwrap().combat,
            Some(CombatRequest::Fire { .. })
        ),
        "input is live through tick five"
    );
    boundary.world.game.tick = 6;
    assert_eq!(boundary.step_companion(id, false).unwrap().combat, None);
    for offline in [false, true] {
        let (mut m, id) = scene();
        for _ in 0..4 {
            turn(&mut m, id, json!({"forward":1}));
        }
        assert!(m.companion_input(
            "operative",
            &input(50, json!({"forward":1,"fire":true})),
            false
        ));
        if offline {
            m.companions.get_mut("operative").unwrap().disconnect();
        } else {
            m.world.game.tick += 6;
        }
        for _ in 0..6 {
            m.world.game.tick += 1;
            assert_eq!(m.step_companion(id, false).unwrap().combat, None);
        }
        assert_eq!(m.world.game.units[&id].vx, Some(0.0));
        assert_eq!(m.companions.soldiers[&id].mags, [8, 32]);
        let at = m.world.game.units[&id].at();
        m.step_companion(id, false).unwrap();
        assert_eq!(m.world.game.units[&id].at(), at);
    }
}

#[test]
fn fps13_movement_respects_stance_diagonals_acceleration_and_jump() {
    let (mut m, id) = scene();
    turn(&mut m, id, json!({"forward":1}));
    let ground_velocity_change = m.world.game.units[&id].vx.unwrap();
    assert_eq!(ground_velocity_change, 1.5);
    for (extra, speed) in [
        (json!({"forward":1}), 4.5),
        (json!({"forward":1,"sprint":true}), 7.0),
        (json!({"forward":1,"crouch":true}), 2.3),
        (json!({"forward":1,"ads":true}), 2.8),
        (json!({"forward":-1,"sprint":true}), 4.5),
        (json!({"forward":1,"strafe":1}), 4.5),
        (json!({"forward":1,"ads":true,"sprint":true}), 2.8),
    ] {
        for _ in 0..12 {
            turn(&mut m, id, extra.clone());
        }
        assert!(
            (m.world.game.units[&id].move_speed - speed).abs() < 1e-10,
            "{extra}"
        );
    }
    turn(&mut m, id, json!({"crouch":true}));
    turn(&mut m, id, json!({"jump":1,"crouch":true}));
    let b = &m.companions.soldiers[&id];
    assert!(!b.crouch);
    assert_eq!(b.vy, 4.6 - 15.0 * 0.05);
    assert_eq!(b.jy, b.vy * 0.05);
    let vx = m.world.game.units[&id].vx.unwrap();
    assert_eq!(vx, 0.0);
    assert!(b.jy > 0.0);
    turn(&mut m, id, json!({"jump":1,"crouch":true,"forward":1}));
    assert!(!m.companions.soldiers[&id].crouch);
    // Forward at yaw zero steers toward +x, with less control than on the ground.
    let air_velocity_change = m.world.game.units[&id].vx.unwrap() - vx;
    assert!(
        air_velocity_change > 0.0,
        "air control must accelerate forward"
    );
    assert!(air_velocity_change < ground_velocity_change);
    assert!((air_velocity_change - 0.3).abs() < 1e-12);
    assert_eq!(m.world.game.units[&id].vz, Some(0.0));
    for _ in 0..25 {
        turn(&mut m, id, json!({"jump":1}));
    }
    assert_eq!(m.companions.soldiers[&id].jy, 0.0);
    assert_eq!(m.companions.soldiers[&id].vy, 0.0);
}

#[test]
fn fps14_collision_slides_and_stops_at_water_cliffs_and_map_margin() {
    for barrier in ["wall", "water", "cliff"] {
        let (mut m, id) = scene();
        place(&mut m, id, 41.5, 40.0);
        if barrier == "cliff" {
            m.world.game.height = Some(vec![0; 64 * 64]);
        }
        for z in 0..64 {
            let c = z * 64 + 21;
            if barrier == "cliff" {
                m.world.game.height.as_mut().unwrap()[c] = 3;
            } else {
                m.world.game.flags[c] = if barrier == "wall" {
                    MOVE | SIGHT
                } else {
                    MOVE
                };
            }
        }
        for _ in 0..16 {
            turn(&mut m, id, json!({"forward":1,"strafe":1}));
        }
        let u = &m.world.game.units[&id];
        assert!(u.x < 42.0, "{barrier}");
        assert!(u.z > 41.0, "slides along {barrier}");
    }
    let (mut m, id) = scene();
    place(&mut m, id, 0.41, 40.0);
    for _ in 0..8 {
        turn(&mut m, id, json!({"forward":-1}));
    }
    assert!(m.world.game.units[&id].x >= 0.4);
}

#[test]
fn fps16_fire_gates_spend_rounds_once_and_bound_held_trigger() {
    let (mut m, id) = scene();
    for _ in 0..20 {
        turn(&mut m, id, json!({"fire":true}));
    }
    assert_eq!(m.companions.get("operative").unwrap().shots, 3);
    assert_eq!(m.companions.soldiers[&id].mags[0], 5);
    for _ in 0..10 {
        turn(&mut m, id, json!({"fire":true,"forward":1,"sprint":true}));
    }
    assert_eq!(m.companions.get("operative").unwrap().shots, 3);
    turn(&mut m, id, json!({"fire":true,"reload":1}));
    assert!(m.companions.soldiers[&id].reload > 0.0);
    assert_eq!(m.companions.get("operative").unwrap().shots, 3);
    for _ in 0..5 {
        turn(&mut m, id, json!({"fire":true,"reload":1}));
    }
    assert_eq!(m.companions.get("operative").unwrap().shots, 3);
}

#[test]
fn fps17_fixed_draw_spread_changes_with_ads_speed_air_and_crouch() {
    let (mut m, id) = scene();
    let order = vec![id];
    for (weapon, w) in WEAPONS.into_iter().enumerate() {
        m.companions.soldiers.get_mut(&id).unwrap().weapon = weapon;
        for (ads, moving, jump, crouch, multiplier) in [
            (false, 0.0, 0.0, false, 1.0),
            (false, 4.5, 0.0, false, 2.0),
            (false, 100.0, 0.0, false, 2.5),
            (false, 0.0, 1.0, false, 3.0),
            (false, 0.0, 0.0, true, 0.6),
            (true, 0.0, 0.0, false, 1.0),
        ] {
            let b = m.companions.soldiers.get_mut(&id).unwrap();
            b.jy = jump;
            b.crouch = crouch;
            let i = Input {
                forward: 0.0,
                strafe: 0.0,
                yaw: 0.0,
                pitch: 0.0,
                fire: true,
                ads,
                crouch,
                sprint: false,
                weapon,
            };
            let ray = companion_ray::trace(
                &m.world.game,
                &m.rules,
                &m.companions.soldiers,
                &order,
                id,
                &i,
                moving,
                [0.81, 0.0],
            )
            .unwrap();
            let expected = (if ads { w.ads } else { w.hip }) * multiplier * 0.9;
            let angle = ww2_core::math::atan2(ray.end.z - 40.0, ray.end.x - 40.0);
            assert!((angle - expected).abs() < 1e-12);
            assert!(ray.enemy_hit.is_none());
        }
    }
}

#[test]
fn fps22_reload_swap_and_empty_trigger_preserve_magazines() {
    let (mut m, id) = scene();
    turn(&mut m, id, json!({}));
    turn(&mut m, id, json!({"reload":1}));
    assert_eq!(m.companions.soldiers[&id].reload, 0.0);
    turn(&mut m, id, json!({"fire":true,"reload":1}));
    turn(&mut m, id, json!({"reload":2}));
    assert_eq!(m.companions.soldiers[&id].reload, 2.4);
    turn(&mut m, id, json!({"weapon":1,"reload":2}));
    assert_eq!(m.companions.soldiers[&id].reload, 0.0);
    assert_eq!(m.companions.soldiers[&id].mags[0], 7);
    assert_eq!(m.firing.get(id).unwrap().cooldown, 0.45);
    for _ in 0..80 {
        turn(&mut m, id, json!({"weapon":1,"fire":true}));
    }
    assert!(m.companions.soldiers[&id].reload > 0.0);
    assert_eq!(m.companions.soldiers[&id].mags[1], 0);
    for _ in 0..45 {
        turn(&mut m, id, json!({"weapon":1}));
    }
    assert_eq!(m.companions.soldiers[&id].mags, [7, 32]);
}

fn aim() -> Input {
    Input {
        forward: 0.0,
        strafe: 0.0,
        yaw: 0.0,
        pitch: 0.0,
        fire: true,
        ads: true,
        crouch: false,
        sprint: false,
        weapon: 0,
    }
}

fn trace(m: &Match, id: u32, i: &Input) -> companion_ray::RayResult {
    let order: Vec<_> = m.world.game.units.keys().copied().collect();
    companion_ray::trace(
        &m.world.game,
        &m.rules,
        &m.companions.soldiers,
        &order,
        id,
        i,
        0.0,
        [0.0, 0.0],
    )
    .unwrap()
}

#[test]
fn fps18_ray_uses_aim_first_body_ground_walls_and_map_edge() {
    let (mut m, id) = scene();
    let target = actor(&mut m, "rifle", 1, 55.0, 40.0);
    m.world.game.players[0].visible = vec![target];
    assert_eq!(trace(&m, id, &aim()).enemy_hit.unwrap().unit, target);
    let mut off = aim();
    off.yaw = 0.3;
    assert!(trace(&m, id, &off).enemy_hit.is_none());
    let nearer = actor(&mut m, "rifle", 1, 48.0, 40.0);
    m.world.game.players[0].visible.push(nearer);
    assert_eq!(trace(&m, id, &aim()).enemy_hit.unwrap().unit, nearer);
    for z in 0..64 {
        m.world.game.flags[z * 64 + 22] = MOVE | SIGHT;
    }
    let stopped = trace(&m, id, &aim());
    assert!(stopped.enemy_hit.is_none());
    assert!(stopped.end.x < 48.0);
    m.world.game.flags.fill(0);
    let mut down = aim();
    down.pitch = -0.5;
    let ground = trace(&m, id, &down);
    assert!(ground.enemy_hit.is_none());
    assert!(ground.end.y < 0.0);
    assert!(ground.end.x < 44.0);
    place(&mut m, id, 125.0, 40.0);
    let edge = trace(&m, id, &aim());
    assert!(edge.enemy_hit.is_none());
    assert!(edge.end.x >= 128.0 && edge.end.x < 128.21);
    m.world.game.players[0].visible.clear();
    m.companions.soldiers.get_mut(&id).unwrap().crouch = true;
    assert_eq!(trace(&m, id, &aim()).end.y, 1.1);
    m.companions.soldiers.get_mut(&id).unwrap().jy = 0.7;
    assert_eq!(trace(&m, id, &aim()).end.y, 1.1 + 0.7);
}

#[test]
fn fps19_target_filters_and_drawn_man_bodies_leave_gaps() {
    let (mut m, id) = scene();
    let target = actor(&mut m, "rifle", 1, 55.0, 40.0);
    assert!(trace(&m, id, &aim()).enemy_hit.is_none(), "unseen body");
    m.world.game.players[0].visible = vec![target];
    let hit = trace(&m, id, &aim()).enemy_hit.unwrap();
    assert_eq!((hit.unit, hit.man), (target, Some(12)));
    for excluded in ["dead", "riding", "air"] {
        let original = m.world.game.units[&target].clone();
        let t = m.world.game.units.get_mut(&target).unwrap();
        match excluded {
            "dead" => t.hp = 0.0,
            "riding" => t.riding = true,
            _ => t.air = Some(ww2_core::game::PlaneState::Base),
        }
        assert!(trace(&m, id, &aim()).enemy_hit.is_none(), "{excluded}");
        m.world.game.units.insert(target, original);
    }
    m.world.game.units.get_mut(&target).unwrap().hp = 1.0;
    assert!(
        trace(&m, id, &aim()).enemy_hit.is_none(),
        "aim at the squad anchor misses its last offset man"
    );
    m.world.game.units.get_mut(&target).unwrap().garrison = Some(0);
    assert_eq!(
        trace(&m, id, &aim()).enemy_hit.unwrap().man,
        None,
        "garrison has one body"
    );
    m.world.game.units.get_mut(&target).unwrap().garrison = None;
    m.world.game.flags.fill(ww2_core::game::TRENCH);
    assert_eq!(
        trace(&m, id, &aim()).enemy_hit.unwrap().man,
        None,
        "trench has one body"
    );
    m.world.game.flags.fill(0);
    m.world.game.players[0].visible.clear();
    let enemy = m.join_companion("enemy", 1, false).unwrap().unwrap();
    place(&mut m, enemy, 55.0, 40.0);
    m.world.game.players[0].visible.push(enemy);
    assert_eq!(trace(&m, id, &aim()).enemy_hit.unwrap().unit, enemy);
    m.companions.soldiers.get_mut(&enemy).unwrap().crouch = true;
    assert!(
        trace(&m, id, &aim()).enemy_hit.is_none(),
        "standing ray clears crouched operative"
    );
    m.companions.soldiers.get_mut(&enemy).unwrap().jy = 0.6;
    assert_eq!(
        trace(&m, id, &aim()).enemy_hit.unwrap().unit,
        enemy,
        "jump raises target body"
    );
    m.world.game.players[0].visible.clear();
    for (kind, pitch) in [("tank", 0.04), ("bunker", 0.15)] {
        let heavy = actor(&mut m, kind, 1, 55.0, 40.0);
        m.world.game.players[0].visible = vec![heavy];
        let mut i = aim();
        i.pitch = pitch;
        assert_eq!(
            trace(&m, id, &i).enemy_hit.unwrap().unit,
            heavy,
            "{kind} body height"
        );
        i.pitch = 0.5;
        assert!(
            trace(&m, id, &i).enemy_hit.is_none(),
            "ray above {kind} body"
        );
    }
}

#[test]
fn fps20_friendly_blocks_and_enemy_damage_updates_hit_and_kill_counters() {
    let (mut m, id) = scene();
    let target = m.join_companion("enemy", 1, false).unwrap().unwrap();
    place(&mut m, target, 55.0, 40.0);
    let friendly = m.join_companion("friendly", 0, false).unwrap().unwrap();
    place(&mut m, friendly, 48.0, 40.0);
    m.world.game.players[0].visible = vec![target, friendly];
    m.fire_companion(id, &aim(), false).unwrap();
    assert_eq!(m.world.game.units[&friendly].hp, 20.0);
    assert_eq!(m.world.game.units[&target].hp, 20.0);
    assert_eq!(m.companions.get("operative").unwrap().hits, 0);
    place(&mut m, friendly, 48.0, 60.0);
    m.firing.get_mut(id).unwrap().cooldown = 0.0;
    m.fire_companion(id, &aim(), false).unwrap();
    assert_eq!(m.world.game.units[&target].hp, 12.0);
    assert!(m.world.game.units[&target].supp > 0.0);
    let metadata = m.combat.get(target).unwrap();
    assert_eq!(metadata.hit_by, Some(id));
    assert_eq!(metadata.hit_at, m.world.game.tick as f64);
    assert_eq!(
        m.world.game.units[&target].hit_from,
        Some(Point { x: 40.0, z: 40.0 })
    );
    m.world.game.units.get_mut(&target).unwrap().hp = 4.0;
    m.firing.get_mut(id).unwrap().cooldown = 0.0;
    m.fire_companion(id, &aim(), false).unwrap();
    let op = m.companions.get("operative").unwrap();
    assert_eq!((op.shots, op.hits, op.kills), (3, 2, 1));
    let tank = actor(&mut m, "tank", 1, 55.0, 40.0);
    m.world.game.players[0].visible = vec![tank];
    for (weapon, damage) in [(0, 0.5), (1, 0.2)] {
        m.companions.soldiers.get_mut(&id).unwrap().weapon = weapon;
        m.firing.get_mut(id).unwrap().cooldown = 0.0;
        let before = m.world.game.units[&tank].hp;
        m.fire_companion(id, &aim(), false).unwrap();
        assert_eq!(
            m.world.game.units[&tank].hp,
            before - damage,
            "vehicle damage for weapon {weapon}"
        );
    }
}

#[test]
fn fps21_shots_publish_weapon_shooter_endpoint_and_hit_man() {
    let (mut m, id) = scene();
    let target = actor(&mut m, "rifle", 1, 55.0, 40.0);
    m.world.game.players[0].visible = vec![target];
    for (weapon, effect) in [(0, "sniper"), (1, "commando")] {
        m.companions.soldiers.get_mut(&id).unwrap().weapon = weapon;
        m.firing.get_mut(id).unwrap().cooldown = 0.0;
        m.firing.events.clear();
        m.fire_companion(id, &aim(), false).unwrap();
        let Some(LaunchEvent::SmallArms(event)) = m.firing.events.last() else {
            panic!("missing shot")
        };
        assert_eq!((event.effect, event.shooter, event.owner), (effect, id, 0));
        let hit = event.hit.unwrap();
        assert_eq!((hit.target, hit.owner, hit.man), (target, 1, Some(12)));
        assert!(event.at.x > 50.0 && event.at.y > 1.0 && (event.at.z - 40.0).abs() < 0.45);
    }
}

#[test]
fn fps24_squad_selection_cadence_retreat_and_commander_release() {
    let (mut m, id) = scene();
    let far = actor(&mut m, "rifle", 0, 46.0, 40.0);
    let nearest = actor(&mut m, "rifle", 0, 43.0, 40.0);
    let _enemy = actor(&mut m, "rifle", 1, 40.5, 40.0);
    let riding = actor(&mut m, "rifle", 0, 41.0, 40.0);
    m.world.game.units.get_mut(&riding).unwrap().riding = true;
    m.pick_companion_squad(id).unwrap();
    assert_eq!(m.companions.get("operative").unwrap().squad_id, nearest);
    assert_eq!(m.commands.units[&far].follow, 0);
    m.world.game.tick = 4;
    m.lead_companion_squad(id, false).unwrap();
    assert_eq!(m.world.game.units[&nearest].amove, None);
    m.world.game.tick = 5;
    m.world.game.units.get_mut(&nearest).unwrap().retreating = true;
    m.lead_companion_squad(id, false).unwrap();
    assert_eq!(m.world.game.units[&nearest].amove, None);
    m.world.game.units.get_mut(&nearest).unwrap().retreating = false;
    m.lead_companion_squad(id, false).unwrap();
    assert_eq!(
        m.world.game.units[&nearest].amove,
        Some(Point { x: 36.0, z: 40.0 })
    );
    let follow = m.commands.units[&nearest].follow;
    assert_ne!(follow, 0);
    let generation = m.world.game.units[&nearest].path_gen;
    m.world.game.tick = 10;
    m.lead_companion_squad(id, false).unwrap();
    assert_eq!(
        m.world.game.units[&nearest].path_gen, generation,
        "unchanged target emits no order"
    );
    place(&mut m, id, 42.9, 40.0);
    m.lead_companion_squad(id, false).unwrap();
    assert_eq!(
        m.world.game.units[&nearest].path_gen, generation,
        "movement below 3 m retains target"
    );
    place(&mut m, id, 43.1, 40.0);
    m.lead_companion_squad(id, false).unwrap();
    assert_eq!(
        m.world.game.units[&nearest].amove,
        Some(Point { x: 39.1, z: 40.0 })
    );
    m.execute_command(
        0,
        &Command::Stop {
            ids: vec![nearest as i64],
        },
        false,
    )
    .unwrap();
    assert_eq!(m.commands.units[&nearest].follow, 0);
    m.lead_companion_squad(id, false).unwrap();
    assert_eq!(m.companions.get("operative").unwrap().squad_id, 0);
    m.pick_companion_squad(id).unwrap();
    m.pick_companion_squad(id).unwrap();
    assert_eq!(m.companions.get("operative").unwrap().squad_id, 0);
}

#[test]
fn fps25_tick_death_removes_soldier_clears_input_and_schedules_respawn() {
    let (mut m, id) = scene();
    assert!(m.companion_input("operative", &input(0, json!({"forward":1})), false));
    m.world.game.units.get_mut(&id).unwrap().hp = 0.0;
    m.advance_tick().unwrap();
    assert!(!m.world.game.units.contains_key(&id));
    let op = m.companions.get("operative").unwrap();
    assert_eq!(op.unit_id, 0);
    assert_eq!(op.input, None);
    assert_eq!(op.respawn_at, m.world.game.tick + 160);
}

#[test]
fn fps26_respawn_prefers_a_squad_then_charges_mp_and_waits_when_ineligible() {
    let (mut m, id) = scene();
    let squad = actor(&mut m, "rifle", 0, 43.0, 40.0);
    m.pick_companion_squad(id).unwrap();
    let hp = m.world.game.units[&squad].hp;
    m.world.game.units.get_mut(&id).unwrap().hp = 0.0;
    m.advance_tick().unwrap();
    let respawn_at = m.companions.get("operative").unwrap().respawn_at;
    m.start.economy.players[0].mp = 0.0;
    m.world.game.tick = respawn_at - 1;
    assert!(m.respawn_companions(false).unwrap().is_empty());
    m.world.game.tick = respawn_at;
    for gate in ["offline", "out", "winner"] {
        m.companions.get_mut("operative").unwrap().online = gate != "offline";
        m.start.economy.players[0].out = gate == "out";
        assert!(m.respawn_companions(gate == "winner").unwrap().is_empty());
    }
    m.companions.get_mut("operative").unwrap().online = true;
    m.start.economy.players[0].out = false;
    let spawned = m.respawn_companions(false).unwrap();
    assert_eq!(spawned.len(), 1);
    assert_eq!(m.world.game.units[&squad].hp, hp - 20.0);
    assert_eq!(m.start.economy.players[0].mp, 0.0);
    assert!((m.world.game.units[&spawned[0]].x - 43.0).abs() < 2.0);
    let body = spawned[0];
    m.world.game.units.get_mut(&body).unwrap().hp = 0.0;
    m.companion_died(body).unwrap();
    m.world.game.units.remove(&body);
    m.grid.rebuild(&m.world.game.units);
    m.world.game.tick += 160;
    for ineligible_squad in ["one_man", "garrison", "riding"] {
        let u = m.world.game.units.get_mut(&squad).unwrap();
        u.hp = if ineligible_squad == "one_man" {
            20.0
        } else {
            80.0
        };
        u.garrison = (ineligible_squad == "garrison").then_some(0);
        u.riding = ineligible_squad == "riding";
        assert!(m.respawn_companions(false).unwrap().is_empty());
    }
    m.start.economy.players[0].mp = 9.0;
    assert!(m.respawn_companions(false).unwrap().is_empty());
    m.start.economy.players[0].mp = 10.0;
    assert_eq!(m.respawn_companions(false).unwrap().len(), 1);
    assert_eq!(m.start.economy.players[0].mp, 0.0);
}

#[test]
fn fps27_real_tick_skips_autonomy_and_squad_reinforcement_for_operative() {
    let (mut m, id) = scene();
    let spawn = m.start.players[0].spawn;
    place(&mut m, id, spawn.x, spawn.z);
    let ordinary = actor(&mut m, "rifle", 0, spawn.x + 2.0, spawn.z);
    m.world.game.units.get_mut(&ordinary).unwrap().hp = 10.0;
    m.commands.units.get_mut(&ordinary).unwrap().auto_retreat = false;
    m.recovery.get_mut(ordinary).unwrap().reinforcement = 10.0;
    m.recovery.get_mut(id).unwrap().reinforcement = 10.0;
    m.start.economy.players[0].mp = 1000.0;
    m.world.game.units.get_mut(&id).unwrap().hp = 10.0;
    m.world.game.units.get_mut(&id).unwrap().hold_pos = false;
    m.world.game.units.get_mut(&id).unwrap().path = vec![Point {
        x: spawn.x + 15.0,
        z: spawn.z,
    }];
    m.commands.units.get_mut(&id).unwrap().auto_retreat = true;
    m.commands.units.get_mut(&id).unwrap().auto_cast = true;
    let at = m.world.game.units[&id].at();
    for _ in 0..25 {
        m.advance_tick().unwrap();
    }
    let u = &m.world.game.units[&id];
    assert_eq!(u.hp, 10.0);
    assert_eq!(u.at(), at);
    assert!(!u.retreating);
    assert_eq!(m.companions.get("operative").unwrap().shots, 0);
    assert_eq!(m.companions.soldiers[&id].nades, 3);
    let hq = actor(&mut m, "hq", 0, spawn.x + 5.0, spawn.z);
    m.world.game.units.get_mut(&hq).unwrap().built = Some(1.0);
    m.recovery.get_mut(ordinary).unwrap().reinforcement = 10.0;
    m.recovery.get_mut(id).unwrap().reinforcement = 10.0;
    let frame = m.prepare_recovery(&[id, ordinary, hq]).unwrap();
    m.reinforce(&frame, 0.05).unwrap();
    assert_eq!(m.world.game.units[&id].hp, 10.0);
    assert!(
        m.world.game.units[&ordinary].hp > 10.0,
        "ordinary squad can reinforce in the same scene"
    );
}

#[test]
fn fps23_counted_grenade_uses_clamped_range_flight_blast_and_cooldown() {
    for (pitch, distance) in [(-1.45, 3.0), (0.0, 13.0), (1.45, 24.0)] {
        let (mut m, id) = scene();
        turn(&mut m, id, json!({"nade":0}));
        turn(&mut m, id, json!({"nade":1,"pitch":pitch}));
        assert_eq!(m.companions.soldiers[&id].nades, 2);
        assert_eq!(m.companions.soldiers[&id].nade_time, 1.0);
        let ticket = m.firing.grenades.last().unwrap();
        assert_eq!(
            ticket.at,
            Point {
                x: 40.0 + distance,
                z: 40.0
            }
        );
        let flight = &m.firing.flights[&ticket.flight];
        assert!(flight.explosive);
        assert_eq!(flight.blast_radius, Some(4.5));
        let blast = flight.blast_source.unwrap();
        assert_eq!(
            (
                blast.infantry,
                blast.vehicle,
                blast.terrain,
                blast.suppression
            ),
            (40.0, 15.0, Some(30.0), 50.0)
        );
        assert_eq!(flight.solution.gravity, 25.0);
        assert_eq!((flight.max_contacts, flight.near), (1, 0.0));
        assert_eq!(flight.effect, Some("boom"));
        assert!(
            m.firing
                .events
                .iter()
                .any(|e| matches!(e,LaunchEvent::Throw{shooter,..} if *shooter==id))
        );
        turn(&mut m, id, json!({"nade":2,"pitch":pitch}));
        assert_eq!(
            m.companions.soldiers[&id].nades, 2,
            "cooldown refuses another press"
        );
        let flights = m.firing.flights.len();
        let body = m.companions.soldiers.get_mut(&id).unwrap();
        body.nades = 0;
        body.nade_time = 0.0;
        turn(&mut m, id, json!({"nade":3,"pitch":pitch}));
        assert_eq!(
            m.firing.flights.len(),
            flights,
            "empty loadout cannot throw"
        );
    }
    let (mut m, id) = scene();
    turn(&mut m, id, json!({}));
    turn(&mut m, id, json!({"nade":1,"sprint":true,"forward":1}));
    assert_eq!(
        m.companions.soldiers[&id].nades, 3,
        "sprinting blocks throw"
    );
    // The integrated runner must settle the actual flight, not a simulated blast.
    let (mut m, id) = scene();
    let enemy = m
        .join_companion("grenade-victim", 1, false)
        .unwrap()
        .unwrap();
    place(&mut m, enemy, 53.0, 40.0);
    assert!(m.companion_input("operative", &input(0, json!({})), false));
    m.advance_tick().unwrap();
    assert!(m.companion_input("operative", &input(1, json!({"nade":1})), false));
    m.advance_tick().unwrap();
    for _ in 0..40 {
        m.advance_tick().unwrap();
    }
    assert!(m.world.game.units.get(&enemy).is_none_or(|u| u.hp < 20.0));
    assert!(m.firing.flights.is_empty());
    assert_eq!(m.companions.soldiers[&id].nades, 2);
}
