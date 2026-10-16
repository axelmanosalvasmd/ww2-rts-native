//! Native intent checks for the owned combat range. No recorded Source state.
use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{Player, Point},
    map::AuthoritativeMap,
    match_abilities::AutocastTarget,
    match_commands::{Command, Denial, MatchCommands, UnitControl},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    unit_spawn::initial_unit,
    vision,
    weather::WeatherChoice,
    world::{Recipient, World},
};
use serde_json::{Value, json};

fn p(x: f64, z: f64) -> Point {
    Point { x, z }
}

fn scene(rows: &[(&str, usize, f64, f64)]) -> Match {
    scene_with_teams(rows, &[0, 1])
}

fn scene_with_teams(rows: &[(&str, usize, f64, f64)], teams: &[i32]) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"combat intent", "w":96, "h":96,
            "rows":vec![".".repeat(96);96],
            "spawns":[{"x":3,"y":3},{"x":92,"y":92}],
            "points":[{"id":"p","x":80,"y":80}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: teams
                .iter()
                .enumerate()
                .map(|(slot, &team)| PlayerConfig {
                    name: format!("Player {slot}"),
                    team,
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
        },
    )
    .unwrap();
    // Component setup uses authored physical terrain and canonical unit owners.
    m.world = World::from_map(&map).unwrap();
    m.world.game.players = teams
        .iter()
        .enumerate()
        .map(|(slot, &team)| Player {
            slot,
            team,
            visible: vec![],
        })
        .collect();
    m.building_cells.clear();
    m.commands = MatchCommands::new(4096);
    m.firing = Default::default();
    m.combat = Default::default();
    m.abilities = Default::default();
    m.recovery = Default::default();
    m.logistics = Default::default();
    m.world.game.tick = 200;
    for (i, &(name, owner, x, z)) in rows.iter().enumerate() {
        let id = i as u32 + 1;
        let mut u = initial_unit(
            &m.rules,
            Catalog::compiled().unwrap(),
            m.rules.kind(name).unwrap(),
            id,
            owner,
            p(x, z),
        )
        .unwrap();
        u.still = Some(10.0);
        let mut c = UnitControl::new(&u, &m.rules).unwrap();
        c.auto_cast = false;
        c.auto_retreat = false;
        m.commands.insert(id, c);
        m.firing.initialize(id);
        m.combat.initialize(id);
        m.abilities.initialize(id);
        m.recovery.initialize(id);
        if u.air.is_some() {
            m.aviation.planes.insert(
                id,
                crate::match_aviation::AircraftRuntime {
                    fuel: 100.,
                    ammo: 100.,
                    timer: 0.,
                    orbit: 0.,
                    mission: None,
                    attack_pick: 0,
                    downed: false,
                },
            );
        }
        m.world.game.units.insert(id, u);
    }
    m.next_unit_id = rows.len() as u32 + 1;
    for owner in 0..teams.len() {
        m.world.register_recipient(Recipient::Player(owner));
        m.world.game.players[owner].visible = m
            .world
            .game
            .units
            .values()
            .filter(|u| u.owner != owner)
            .map(|u| u.id)
            .collect();
    }
    m.grid.rebuild(&m.world.game.units);
    m.initialize_recipient_routing().unwrap();
    let ids = ids(&m);
    m.begin_firing_tick(&ids).unwrap();
    m
}
fn ids(m: &Match) -> Vec<u32> {
    m.world.game.units.keys().copied().collect()
}
fn command(m: &mut Match, v: Value) -> Option<Denial> {
    let c = Command::from_json(&v).expect("valid native command input");
    m.execute_command(0, &c, false).unwrap()
}
fn move_to(m: &mut Match, id: u32, x: f64, z: f64) {
    assert_eq!(command(m, json!({"t":"move","orders":[[id,x,z]]})), None);
}
fn terrain(m: &mut Match, x: f64, z: f64, ch: u8) -> usize {
    let c = m.world.game.cell_of(x, z).unwrap();
    m.world.set_cell(c, ch).unwrap();
    c
}
fn rebuild(m: &mut Match) {
    m.grid.rebuild(&m.world.game.units);
}
fn acquire(m: &mut Match, id: u32) -> u32 {
    // Suppress launch to isolate the actual automatic target-selection decision.
    m.firing.get_mut(id).unwrap().cooldown = 100.0;
    m.apply_unit_firing(id, false, 0.05).unwrap();
    m.firing.get(id).unwrap().target_id
}

#[test]
fn cmb82_medics_choose_worst_heal_calm_and_recent_wounds_and_seek() {
    let mut m = scene(&[
        ("medic", 0, 40., 40.),
        ("rifle", 0, 46., 40.),
        ("rifle", 0, 44., 40.),
    ]);
    m.world.game.units.get_mut(&2).unwrap().hp = 40.;
    m.world.game.units.get_mut(&3).unwrap().hp = 70.;
    m.heal_wounded(&ids(&m), 1.).unwrap();
    assert_eq!(m.world.game.units[&2].hp, 45.);
    assert_eq!(m.world.game.units[&3].hp, 70.);
    m.world.game.units.get_mut(&2).unwrap().hp = 40.;
    m.heal_wounded(&ids(&m), 1.).unwrap();
    assert_eq!(m.world.game.units[&2].hp, 42.5);
    m.world.game.units.get_mut(&2).unwrap().retreating = true;
    m.world.game.units.get_mut(&3).unwrap().riding = true;
    m.heal_wounded(&ids(&m), 1.).unwrap();
    assert_eq!(m.world.game.units[&2].hp, 42.5);
    assert_eq!(m.world.game.units[&3].hp, 70.);
    let u = m.world.game.units.get_mut(&2).unwrap();
    u.retreating = false;
    u.x = 65.;
    rebuild(&mut m);
    m.begin_path_tick(4096, 4).unwrap();
    let routes = m.heal_wounded(&ids(&m), 0.5).unwrap();
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].unit, 1);
    assert_eq!(routes[0].goal, p(65., 40.));
    assert!(!m.world.game.units[&1].path.is_empty());
}

#[test]
fn cmb82_medic_pass_is_every_ten_ticks_and_provisions_cap_healing() {
    use crate::logistics::Reserves;
    let mut m = scene(&[("medic", 0, 40., 40.), ("rifle", 0, 46., 40.)]);
    m.world.game.units.get_mut(&2).unwrap().hp = 40.;
    m.logistics.insert(
        1,
        Reserves {
            ammo: 0.,
            ammo_max: 0.,
            ammo_price: 0.,
            ammo_credit: None,
            provisions: 0.1,
            fuel: None,
            emergency: None,
            warning: None,
            forced: false,
            stranded: false,
            exhausted: false,
            supply_rate: None,
            cut_for: None,
        },
    );
    m.world.game.tick = 201;
    let frame = m.prepare_recovery(&ids(&m)).unwrap();
    m.heal_and_sweep(&frame, 0.05).unwrap();
    assert_eq!(m.world.game.units[&2].hp, 40.);
    m.world.game.tick = 210;
    let frame = m.prepare_recovery(&ids(&m)).unwrap();
    m.heal_and_sweep(&frame, 0.05).unwrap();
    assert_eq!(m.world.game.units[&2].hp, 40.5);
    assert_eq!(m.logistics.get(1).unwrap().provisions, 0.);
}

#[test]
fn cmb85_hold_position_keeps_exact_destination_and_blocks_idle_spacing() {
    let mut m = scene(&[("rifle", 0, 40., 40.), ("rifle", 0, 41., 40.)]);
    assert_eq!(
        command(
            &mut m,
            json!({"t":"stance","key":"holdPos","ids":[1,2],"on":true})
        ),
        None
    );
    terrain(&mut m, 61., 41., b'O');
    move_to(&mut m, 1, 60., 40.);
    assert_eq!(m.commands.units[&1].route_end, Some(p(60., 40.)));
    m.world.game.units.get_mut(&1).unwrap().path.clear();
    m.world.game.units.get_mut(&1).unwrap().world_goal = None;
    m.react_unit(1).unwrap();
    m.react_unit(2).unwrap();
    assert!(m.world.game.units[&2].path.is_empty());
}

#[test]
fn cmb86_target_threat_hysteresis_visibility_and_explicit_lock() {
    let mut m = scene(&[
        ("rifle", 0, 40., 40.),
        ("rifle", 1, 50., 40.),
        ("rifle", 1, 51., 40.),
    ]);
    assert_eq!(acquire(&mut m, 1), 2);
    m.world.game.units.get_mut(&3).unwrap().x = 48.5;
    rebuild(&mut m);
    m.firing.get_mut(1).unwrap().retarget = 0.;
    assert_eq!(
        acquire(&mut m, 1),
        2,
        "8.5 m challenger cannot displace 10 m current target"
    );
    m.world.game.units.get_mut(&3).unwrap().x = 46.5;
    rebuild(&mut m);
    m.firing.get_mut(1).unwrap().retarget = 0.;
    assert_eq!(
        acquire(&mut m, 1),
        3,
        "6.5 m challenger crosses the hysteresis margin"
    );
    // Restore the farther challenger so incoming fire must change the decision.
    m.world.game.units.get_mut(&3).unwrap().x = 51.;
    rebuild(&mut m);
    m.firing.get_mut(1).unwrap().target_id = 2;
    m.combat.get_mut(1).unwrap().hit_by = Some(3);
    m.combat.get_mut(1).unwrap().hit_at = 200.;
    m.firing.get_mut(1).unwrap().retarget = 0.;
    assert_eq!(
        acquire(&mut m, 1),
        3,
        "recent incoming fire is worth answering"
    );
    m.world.game.players[0].visible = vec![2];
    assert_eq!(acquire(&mut m, 1), 2, "hidden enemy cannot retain target");
    assert_eq!(
        command(&mut m, json!({"t":"attack","ids":[1],"target":2})),
        None
    );
    m.apply_ground_attack(1).unwrap();
    m.world.game.players[0].visible = vec![2, 3];
    m.firing.get_mut(1).unwrap().retarget = 0.;
    assert_eq!(
        acquire(&mut m, 1),
        2,
        "explicit attack wins over automatic threat"
    );
}

#[test]
fn cmb87_retarget_waits_then_replaces_dead_target() {
    let mut m = scene(&[
        ("rifle", 0, 40., 40.),
        ("rifle", 1, 50., 40.),
        ("rifle", 1, 51., 40.),
    ]);
    m.firing.get_mut(1).unwrap().retarget = 0.25;
    assert_eq!(
        acquire(&mut m, 1),
        0,
        "idle actor waits for scheduled search"
    );
    m.advance_firing_timers(1, 0.25).unwrap();
    assert_eq!(acquire(&mut m, 1), 2);
    m.world.game.units.get_mut(&2).unwrap().hp = 0.;
    assert!(m.firing.get(1).unwrap().retarget > 0.);
    m.firing.get_mut(1).unwrap().cooldown = 0.;
    let before = m.world.game.units[&3].hp;
    m.apply_unit_firing(1, false, 0.05).unwrap();
    assert_eq!(
        m.firing.get(1).unwrap().target_id,
        3,
        "dead target invalidates before next search"
    );
    assert!(m.firing.take_events().iter().any(|e|matches!(e,crate::match_firing::LaunchEvent::Flight {shooter:1,time,..} if *time==10.)),"replacement receives a volley on acquisition tick");
    for _ in 0..6 {
        m.capture_firing_prior().unwrap();
        m.world.game.tick += 1;
        m.step_firing_flights().unwrap();
    }
    assert!(
        m.world.game.units[&3].hp < before,
        "replacement is damaged by actual physical flight"
    );
}

#[test]
fn cmb88_claims_spread_fire_track_switches_and_rebuild_from_living_models() {
    use std::collections::BTreeMap;
    let mut m = scene(&[
        ("rifle", 0, 40., 40.),
        ("rifle", 0, 40., 40.),
        ("rifle", 0, 40., 40.),
        ("rifle", 0, 40., 40.),
        ("rifle", 0, 40., 40.),
        ("rifle", 0, 40., 40.),
        ("rifle", 1, 60., 40.),
        ("rifle", 1, 60., 41.),
    ]);
    for id in [7, 8] {
        m.world.game.units.get_mut(&id).unwrap().hp = 10.;
    }
    let mut expected = BTreeMap::new();
    // Prototype rifle volley: damage 3, infantry accuracy 0.7, five living models.
    let volley = 3. * 0.7 * 5.;
    for id in 1..=6 {
        let target = acquire(&mut m, id);
        assert!([7, 8].contains(&target));
        *expected.entry((target, 0)).or_insert(0.) += volley;
        assert_eq!(
            m.firing.target_claims().collect::<BTreeMap<_, _>>(),
            expected
        );
    }
    assert_eq!(expected.len(), 2, "the group spreads its fire");
    let old = m.firing.get(1).unwrap().target_id;
    let next = if old == 7 { 8 } else { 7 };
    m.world.game.players[0].visible = vec![next];
    assert_eq!(acquire(&mut m, 1), next);
    *expected.get_mut(&(old, 0)).unwrap() -= volley;
    *expected.get_mut(&(next, 0)).unwrap() += volley;
    assert_eq!(
        m.firing.target_claims().collect::<BTreeMap<_, _>>(),
        expected
    );
    assert_eq!(acquire(&mut m, 1), next);
    assert_eq!(
        m.firing.target_claims().collect::<BTreeMap<_, _>>(),
        expected
    );

    m.world.game.units.get_mut(&1).unwrap().hp = 40.;
    m.world.game.units.get_mut(&2).unwrap().hp = 0.;
    expected.clear();
    for id in [1, 3, 4, 5, 6] {
        let models = if id == 1 { 2. } else { 5. };
        *expected
            .entry((m.firing.get(id).unwrap().target_id, 0))
            .or_insert(0.) += 3. * 0.7 * models;
    }
    m.begin_firing_tick(&ids(&m)).unwrap();
    assert_eq!(
        m.firing.target_claims().collect::<BTreeMap<_, _>>(),
        expected
    );
}

#[test]
fn cmb89_salvo_crowds_ignore_hidden_and_riding_neighbors() {
    let mut m = scene(&[
        ("mortar", 0, 40., 40.),
        ("rifle", 1, 65., 40.),
        ("rifle", 1, 40., 66.),
        ("rifle", 1, 66., 41.),
        ("rifle", 1, 67., 40.),
    ]);
    assert_eq!(acquire(&mut m, 1), 2);
    terrain(&mut m, 40., 66., b'O');
    m.firing.get_mut(1).unwrap().retarget = 0.;
    assert_eq!(acquire(&mut m, 1), 2, "visible crowd outweighs shelter");
    m.world.game.players[0].visible = vec![2, 3];
    m.firing.get_mut(1).unwrap().target_id = 0;
    m.firing.get_mut(1).unwrap().retarget = 0.;
    assert_eq!(
        acquire(&mut m, 1),
        3,
        "hidden crowd cannot outweigh shelter"
    );
    m.world.game.players[0].visible = vec![2, 3, 4, 5];
    m.world.game.units.get_mut(&4).unwrap().riding = true;
    m.world.game.units.get_mut(&5).unwrap().riding = true;
    m.firing.get_mut(1).unwrap().target_id = 0;
    m.firing.get_mut(1).unwrap().retarget = 0.;
    assert_eq!(acquire(&mut m, 1), 3, "riding crowd does not contribute");
}

#[test]
fn cmb90_attack_chases_stops_locks_drops_unseen_and_pauses_for_satchel() {
    let mut m = scene(&[("rifle", 0, 40., 40.), ("rifle", 1, 100., 40.)]);
    assert_eq!(
        command(&mut m, json!({"t":"attack","ids":[1],"target":2})),
        None
    );
    m.begin_path_tick(4096, 4).unwrap();
    m.apply_ground_attack(1).unwrap();
    assert!(!m.world.game.units[&1].path.is_empty());
    m.commands.units.get_mut(&1).unwrap().nade = Some(p(100., 40.));
    let path = m.world.game.units[&1].path.clone();
    m.apply_ground_attack(1).unwrap();
    assert_eq!(m.world.game.units[&1].path, path);
    m.commands.units.get_mut(&1).unwrap().nade = None;
    m.world.game.units.get_mut(&2).unwrap().x = 60.;
    rebuild(&mut m);
    m.apply_ground_attack(1).unwrap();
    assert!(m.world.game.units[&1].path.is_empty());
    assert_eq!(m.firing.get(1).unwrap().target_id, 2);
    m.world.game.players[0].visible.clear();
    m.apply_ground_attack(1).unwrap();
    assert_eq!(m.commands.units[&1].attack_id, 0);
}

#[test]
fn cmb92_reaction_seeks_cover_but_crews_orders_and_hold_position_stay() {
    for (kind, hold, ordered, should_move) in [
        ("rifle", false, false, true),
        ("mg", false, false, false),
        ("rifle", true, false, false),
        ("rifle", false, true, false),
    ] {
        let mut m = scene(&[(kind, 0, 41., 41.), ("rifle", 1, 57., 41.)]);
        terrain(&mut m, 43., 41., b'O');
        m.world.game.units.get_mut(&1).unwrap().hit_from = Some(p(57., 41.));
        m.world.game.units.get_mut(&1).unwrap().hold_pos = hold;
        m.combat.get_mut(1).unwrap().hit_at = 200.;
        m.combat.get_mut(1).unwrap().hit_by = Some(2);
        if ordered {
            m.commands.units.get_mut(&1).unwrap().attack_id = 2;
        }
        m.react_unit(1).unwrap();
        assert_eq!(
            !m.world.game.units[&1].path.is_empty(),
            should_move,
            "{kind} hold={hold} ordered={ordered}"
        );
        if should_move {
            assert_eq!(m.commands.units[&1].react, 2.);
        }
    }
}

#[test]
fn cmb93_idle_spacing_moves_newer_squad_and_respects_hold_position() {
    for hold in [false, true] {
        let mut m = scene(&[("rifle", 0, 41., 41.), ("rifle", 0, 42., 41.)]);
        m.world.game.units.get_mut(&2).unwrap().hold_pos = hold;
        m.react_unit(1).unwrap();
        m.react_unit(2).unwrap();
        assert!(m.world.game.units[&1].path.is_empty());
        assert_eq!(!m.world.game.units[&2].path.is_empty(), !hold);
    }
}

#[test]
fn cmb97_take_cover_refusals_and_queue_cancellation() {
    let mut m = scene(&[("rifle", 0, 41., 41.), ("tank", 0, 45., 41.)]);
    assert_eq!(
        command(&mut m, json!({"t":"cover","ids":[2]})),
        Some(Denial::Needs)
    );
    assert_eq!(
        command(&mut m, json!({"t":"cover","ids":[1]})),
        Some(Denial::NoCover)
    );
    m.world.game.units.get_mut(&1).unwrap().retreating = true;
    assert_eq!(
        command(&mut m, json!({"t":"cover","ids":[1]})),
        Some(Denial::Retreating)
    );
    m.world.game.units.get_mut(&1).unwrap().retreating = false;
    terrain(&mut m, 43., 41., b'O');
    move_to(&mut m, 1, 70., 41.);
    assert_eq!(
        command(
            &mut m,
            json!({"t":"move","orders":[[1,80.,41.]],"queue":true})
        ),
        None
    );
    assert_eq!(command(&mut m, json!({"t":"cover","ids":[1]})), None);
    assert!(m.commands.units[&1].orders.is_empty());
    assert_eq!(m.world.game.units[&1].world_goal, None);
    assert!(!m.world.game.units[&1].path.is_empty());
}

#[test]
fn cmb98_hull_turns_to_visible_anti_tank_threat_before_ordered_face() {
    let mut m = scene(&[("tank", 0, 41., 41.), ("at", 1, 41., 61.)]);
    m.world.game.units.get_mut(&1).unwrap().rot = 0.;
    m.firing.get_mut(1).unwrap().target_id = 2;
    m.commands.units.get_mut(&1).unwrap().face = Some(crate::match_commands::Facing {
        angle: -1.,
        at: p(41., 41.),
    });
    acquire(&mut m, 1);
    assert!(m.world.game.units[&1].rot > 0.);
    assert!(m.world.game.units[&1].rot < std::f64::consts::FRAC_PI_2);
}

#[test]
fn cmb99_short_engaged_move_behind_reverses_long_move_does_not() {
    for (distance, reverse) in [(10., true), (20., false)] {
        let mut m = scene(&[("tank", 0, 60., 40.), ("rifle", 1, 70., 40.)]);
        m.firing.get_mut(1).unwrap().target_id = 2;
        m.world.game.units.get_mut(&1).unwrap().rot = 0.;
        move_to(&mut m, 1, 60. - distance, 40.);
        assert_eq!(m.world.game.units[&1].reverse.is_some(), reverse);
    }
}

#[test]
fn cmb100_facing_validation_shared_spots_arrival_and_replacement() {
    for bad in [json!("bad"), json!(null), json!({})] {
        assert_eq!(
            Command::from_json(&json!({"t":"move","orders":[[1,60,40]],"face":bad})),
            Err(Denial::Blocked)
        );
    }
    let mut m = scene(&[("rifle", 0, 40., 40.), ("rifle", 0, 40., 46.)]);
    assert_eq!(
        command(
            &mut m,
            json!({"t":"move","orders":[[1,70,50],[2,70,50]],"face":0})
        ),
        None
    );
    let a = m.commands.units[&1].face.unwrap();
    let b = m.commands.units[&2].face.unwrap();
    assert_ne!(a.at, b.at);
    assert_eq!(a.angle, 0.);
    assert_eq!(b.angle, 0.);
    for id in [1, 2] {
        let face = m.commands.units[&id].face.unwrap();
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.x = face.at.x;
        u.z = face.at.z;
        u.path.clear();
        u.world_goal = None;
        u.rot = 1.;
        m.apply_unit_firing(id, false, 0.05).unwrap();
        assert_eq!(m.world.game.units[&id].rot, 0.);
    }
    assert_eq!(command(&mut m, json!({"t":"stop","ids":[1,2]})), None);
    assert!(m.commands.units[&1].face.is_none());
    assert!(m.commands.units[&2].face.is_none());
}

#[test]
fn cmb103_together_caps_group_pace_and_queued_leg_drops_it() {
    let mut m = scene(&[("rifle", 0, 40., 40.), ("mg", 0, 40., 48.)]);
    assert_eq!(
        command(
            &mut m,
            json!({"t":"move","orders":[[1,80,40],[2,80,48]],"together":true})
        ),
        None
    );
    let pace = m.world.game.units[&1].pace;
    assert_eq!(m.world.game.units[&2].pace, pace);
    let slowest = m.rules.kinds[m.rules.kind("rifle").unwrap()]
        .speed
        .min(m.rules.kinds[m.rules.kind("mg").unwrap()].speed);
    assert_eq!(pace, slowest);
    assert_eq!(
        command(
            &mut m,
            json!({"t":"move","orders":[[1,90,40]],"queue":true,"together":true})
        ),
        None
    );
    let u = m.world.game.units.get_mut(&1).unwrap();
    u.path.clear();
    u.world_goal = None;
    m.start_queued_orders(1, false).unwrap();
    assert!(m.world.game.units[&1].pace == 0.);
}

#[test]
fn cmb104_109_113_aimed_families_launch_once_and_manual_halts_auto_keeps_route() {
    for kind in ["rifle", "ranger", "commando", "churchill", "medium"] {
        for automatic in [false, true] {
            let mut m = scene(&[(kind, 0, 40., 40.)]);
            let destination = p(70., 40.);
            m.world.game.units.get_mut(&1).unwrap().path = vec![destination];
            let cmd = Command::Ability {
                ids: vec![1],
                at: Some(p(44., 40.)),
                queue: false,
                automatic,
            };
            assert_eq!(m.execute_command(0, &cmd, false).unwrap(), None, "{kind}");
            m.step_aimed_ability(1).unwrap();
            assert!(m.commands.units[&1].nade.is_none());
            assert!(m.abilities.get(1).unwrap().cooldown > 0.);
            assert_eq!(!m.world.game.units[&1].path.is_empty(), automatic, "{kind}");
            let events = m.firing.take_events();
            assert!(!events.is_empty(), "{kind}");
            m.step_aimed_ability(1).unwrap();
            assert!(m.firing.take_events().is_empty());
        }
    }
}

fn finish_ability_flight(m: &mut Match) {
    for _ in 0..100 {
        m.capture_firing_prior().unwrap();
        m.world.game.tick += 1;
        m.step_firing_flights().unwrap();
        m.advance_firing_grenades(0.05).unwrap();
    }
    assert!(m.firing.flights.is_empty());
    assert!(m.firing.grenades.is_empty());
}

#[test]
fn cmb104_109_grenades_satchel_and_petard_impact_hurt_both_teams_and_structures() {
    for kind in ["rifle", "medium", "ranger", "commando", "churchill"] {
        let mut m = scene(&[
            (kind, 0, 41., 41.),
            ("rifle", 1, 47., 43.),
            ("rifle", 0, 47., 39.),
            ("tank", 1, 49., 41.),
            ("rifle", 1, 65., 41.),
        ]);
        let before: Vec<_> = (2..=5).map(|id| m.world.game.units[&id].hp).collect();
        assert_eq!(
            command(&mut m, json!({"t":"ability","ids":[1],"x":47,"z":41})),
            None
        );
        m.step_aimed_ability(1).unwrap();
        for id in 2..=5 {
            assert_eq!(m.world.game.units[&id].hp, before[id as usize - 2]);
        }
        finish_ability_flight(&mut m);
        for id in 2..=4 {
            assert!(
                m.world.game.units[&id].hp < before[id as usize - 2],
                "{kind} blast hurts unit {id}"
            );
        }
        assert_eq!(
            m.world.game.units[&2].hp, m.world.game.units[&3].hp,
            "{kind} applies the same blast to friend and foe"
        );
        assert_eq!(
            m.world.game.units[&5].hp, before[3],
            "outside the blast radius"
        );
        let after: Vec<_> = (2..=5).map(|id| m.world.game.units[&id].hp).collect();
        finish_ability_flight(&mut m);
        assert_eq!(
            after,
            (2..=5)
                .map(|id| m.world.game.units[&id].hp)
                .collect::<Vec<_>>(),
            "linked ticket cannot explode twice"
        );

        let mut structure = scene(&[(kind, 0, 41., 41.), ("bunker", 1, 47., 43.)]);
        // Beside the trajectory: only the blast can damage this section.
        let cell = terrain(&mut structure, 47., 43., b'B');
        structure.world.game.units.get_mut(&2).unwrap().hp = 1000.;
        structure
            .world
            .register_building_structure(2, &[cell], 1000., 1000.)
            .unwrap();
        rebuild(&mut structure);
        assert_eq!(
            command(
                &mut structure,
                json!({"t":"ability","ids":[1],"x":47,"z":41})
            ),
            None
        );
        structure.step_aimed_ability(1).unwrap();
        finish_ability_flight(&mut structure);
        assert!(
            structure.world.game.units[&2].hp < 1000.,
            "{kind} damages the registered structure"
        );
        assert!(
            structure
                .world
                .game
                .sections
                .get(&(cell as u32))
                .is_none_or(|section| section.hp < 1000.),
            "{kind} damages its structural section"
        );
    }
}

#[test]
fn cmb106_ap_loads_once_and_sets_family_cooldown() {
    for (kind, cd) in [("at", 45.), ("tankdestroyer", 45.), ("tiger", 40.)] {
        let mut m = scene(&[(kind, 0, 40., 40.)]);
        assert_eq!(command(&mut m, json!({"t":"ability","ids":[1]})), None);
        assert!(m.abilities.get(1).unwrap().ap);
        assert_eq!(m.abilities.get(1).unwrap().cooldown, cd);
        assert_eq!(
            command(&mut m, json!({"t":"ability","ids":[1]})),
            Some(Denial::Cooldown)
        );
    }
}

#[test]
fn cmb107_smoke_uses_unit_id_radius_duration_and_cooldown() {
    for kind in ["tank", "armoredcar"] {
        let mut m = scene(&[(kind, 0, 40., 40.)]);
        let next = m.next_unit_id;
        assert_eq!(command(&mut m, json!({"t":"ability","ids":[1]})), None);
        assert_eq!(m.next_unit_id, next + 1);
        let s = &m.world.game.smokes[0];
        assert_eq!(s.id, next);
        assert_eq!(s.r, 9.);
        assert_eq!(s.remaining, Some(14.));
        assert_eq!((s.x, s.z), (40., 40.));
        assert_eq!(m.abilities.get(1).unwrap().cooldown, 45.);
    }
}

#[test]
fn cmb108_barrage_uses_family_shell_count_and_reload() {
    for (kind, count, cooldown) in [("rocket", 8, 20.), ("mortar", 4, 40.), ("howitzer", 4, 50.)] {
        let mut m = scene(&[(kind, 0, 40., 40.)]);
        assert_eq!(
            command(&mut m, json!({"t":"ability","ids":[1],"x":50,"z":40})),
            None
        );
        m.step_aimed_ability(1).unwrap();
        assert_eq!(m.abilities.get(1).unwrap().cooldown, cooldown);
        assert!(m.firing.get(1).unwrap().cooldown > 0.);
        assert!(m.firing.take_events().iter().any(|event| matches!(
            event,
            crate::match_firing::LaunchEvent::Salvo { count: actual, .. } if *actual == count
        )));
        assert!(m.commands.units[&1].nade.is_none());
    }
}

#[test]
fn cmb110_ura_clears_suppression_sprints_and_cools_down() {
    let mut m = scene(&[("conscript", 0, 40., 40.)]);
    m.world.game.units.get_mut(&1).unwrap().supp = 80.;
    assert_eq!(command(&mut m, json!({"t":"ability","ids":[1]})), None);
    assert_eq!(m.world.game.units[&1].supp, 0.);
    assert_eq!(m.world.game.units[&1].sprint, 6.);
    assert_eq!(m.abilities.get(1).unwrap().cooldown, 35.);
}

#[test]
fn cmb111_mixed_selection_cooldown_retreat_missing_spot_and_deferred_queue() {
    let mut m = scene(&[("rifle", 0, 40., 40.), ("rifle", 0, 40., 46.)]);
    assert_eq!(
        command(&mut m, json!({"t":"ability","ids":[1]})),
        Some(Denial::Blocked)
    );
    m.abilities.get_mut(1).unwrap().cooldown = 10.;
    assert_eq!(
        command(&mut m, json!({"t":"ability","ids":[1],"x":45,"z":40})),
        Some(Denial::Cooldown)
    );
    assert_eq!(
        command(&mut m, json!({"t":"ability","ids":[1,2],"x":45,"z":40})),
        None
    );
    assert!(m.commands.units[&1].nade.is_none());
    assert!(m.commands.units[&2].nade.is_some());
    m.world.game.units.get_mut(&1).unwrap().retreating = true;
    m.abilities.get_mut(1).unwrap().cooldown = 0.;
    assert_eq!(
        command(&mut m, json!({"t":"ability","ids":[1],"x":45,"z":40})),
        Some(Denial::Retreating)
    );
    m.world.game.units.get_mut(&1).unwrap().retreating = false;
    m.abilities.get_mut(1).unwrap().cooldown = 10.;
    assert_eq!(
        command(
            &mut m,
            json!({"t":"ability","ids":[1],"x":45,"z":40,"queue":true})
        ),
        None
    );
    assert_eq!(m.commands.units[&1].orders.len(), 1);
}

#[test]
fn cmb114_autocast_boolean_ownership_ability_and_free_default() {
    let mut m = scene(&[
        ("rifle", 0, 40., 40.),
        ("medic", 0, 40., 46.),
        ("rifle", 1, 50., 40.),
    ]);
    assert_eq!(
        Command::from_json(&json!({"t":"autocast","ids":[1],"on":1})),
        Err(Denial::Blocked)
    );
    assert_eq!(
        command(&mut m, json!({"t":"autocast","ids":[3],"on":true})),
        Some(Denial::Needs)
    );
    assert_eq!(
        command(&mut m, json!({"t":"autocast","ids":[2],"on":true})),
        Some(Denial::Needs)
    );
    assert_eq!(
        command(&mut m, json!({"t":"autocast","ids":[1],"on":true})),
        None
    );
    assert!(m.commands.units[&1].auto_cast);
    let u = &m.world.game.units[&1];
    assert!(
        UnitControl::new_for_mode(u, &m.rules, Mode::Conquest)
            .unwrap()
            .auto_cast
    );
    assert!(
        !UnitControl::new_for_mode(u, &m.rules, Mode::World)
            .unwrap()
            .auto_cast
    );
}

#[test]
fn cmb115_autocast_staggers_and_preserves_moving_ura_route() {
    let mut m = scene(&[("conscript", 0, 40., 40.)]);
    m.commands.units.get_mut(&1).unwrap().auto_cast = true;
    m.world.game.units.get_mut(&1).unwrap().path = vec![p(80., 40.)];
    m.world.game.units.get_mut(&1).unwrap().supp = 80.;
    for tick in 200..209 {
        m.world.game.tick = tick;
        m.apply_autocast(1).unwrap();
        assert_eq!(m.world.game.units[&1].sprint, 0.);
    }
    m.world.game.tick = 209;
    m.apply_autocast(1).unwrap();
    assert_eq!(m.world.game.units[&1].sprint, 6.);
    assert_eq!(m.world.game.units[&1].path, vec![p(80., 40.)]);
    m.world.game.tick = 219;
    m.apply_autocast(1).unwrap();
    assert_eq!(m.abilities.get(1).unwrap().cooldown, 35.);
}

#[test]
fn cmb116_grenade_requires_visible_sheltered_enemy_and_clear_blast() {
    let mut m = scene(&[
        ("rifle", 0, 40., 40.),
        ("rifle", 1, 54., 40.),
        ("rifle", 0, 80., 80.),
    ]);
    assert_eq!(m.autocast_target(1).unwrap(), None);
    terrain(&mut m, 54., 40., b'O');
    assert_eq!(
        m.autocast_target(1).unwrap(),
        Some(AutocastTarget::At(p(54., 40.)))
    );
    m.world.game.players[0].visible.clear();
    assert_eq!(m.autocast_target(1).unwrap(), None);
    m.world.game.players[0].visible = vec![2];
    let u = m.world.game.units.get_mut(&3).unwrap();
    u.x = 55.;
    u.z = 40.;
    rebuild(&mut m);
    assert_eq!(m.autocast_target(1).unwrap(), None);
}

#[test]
fn cmb117_mg_suppression_requires_stationary_gun_and_approaching_enemy() {
    let mut m = scene(&[("mg", 0, 40., 40.), ("rifle", 1, 60., 40.)]);
    let u = m.world.game.units.get_mut(&2).unwrap();
    u.path = vec![p(40., 40.)];
    u.rot = std::f64::consts::PI;
    assert_eq!(m.autocast_target(1).unwrap(), Some(AutocastTarget::Instant));
    m.world.game.units.get_mut(&2).unwrap().retreating = true;
    assert_eq!(m.autocast_target(1).unwrap(), None);
    m.world.game.units.get_mut(&2).unwrap().retreating = false;
    m.world.game.units.get_mut(&2).unwrap().rot = 0.;
    assert_eq!(m.autocast_target(1).unwrap(), None);
    m.world.game.units.get_mut(&2).unwrap().rot = std::f64::consts::PI;
    m.world.game.units.get_mut(&1).unwrap().path = vec![p(45., 40.)];
    assert_eq!(m.autocast_target(1).unwrap(), None);
}

#[test]
fn cmb121_barrage_requires_visible_crowd_or_dug_target_and_no_friend() {
    let mut m = scene(&[
        ("mortar", 0, 40., 40.),
        ("rifle", 1, 65., 40.),
        ("rifle", 1, 66., 40.),
        ("rifle", 1, 67., 40.),
        ("rifle", 0, 90., 90.),
    ]);
    assert!(m.autocast_target(1).unwrap().is_some());
    m.world.game.players[0].visible = vec![2];
    assert_eq!(m.autocast_target(1).unwrap(), None);
    terrain(&mut m, 65., 40., b'T');
    assert!(m.autocast_target(1).unwrap().is_some());
    let u = m.world.game.units.get_mut(&5).unwrap();
    u.x = 66.;
    u.z = 40.;
    rebuild(&mut m);
    assert_eq!(m.autocast_target(1).unwrap(), None);
}

#[test]
fn cmb122_satchel_requires_seen_ordered_structure_with_safe_reach() {
    let mut m = scene(&[("ranger", 0, 40., 40.), ("bunker", 1, 55., 40.)]);
    assert_eq!(m.autocast_target(1).unwrap(), None);
    m.commands.units.get_mut(&1).unwrap().attack_id = 2;
    assert!(m.autocast_target(1).unwrap().is_some());
    m.world.game.players[0].visible.clear();
    assert_eq!(m.autocast_target(1).unwrap(), None);
    m.world.game.players[0].visible = vec![2];
    m.world.game.units.get_mut(&2).unwrap().x = 65.;
    rebuild(&mut m);
    assert_eq!(m.autocast_target(1).unwrap(), None);
}

#[test]
fn cmb123_vision_is_team_shared_and_refreshes_through_match_ticks() {
    let mut m = scene_with_teams(
        &[("rifle", 0, 40., 40.), ("rifle", 1, 60., 40.)],
        &[0, 1, 0],
    );
    for control in m.commands.units.values_mut() {
        control.hold_fire = true;
    }
    for unit in m.world.game.units.values_mut() {
        unit.hold_pos = true;
    }
    for player in &mut m.world.game.players {
        player.visible.clear();
    }
    // The authored scene replaced startup buildings, so it needs empty production rows.
    m.production =
        crate::production::ProductionState::new(vec![
            crate::economy::TechState::new(1, 0, 0, 0)
                .unwrap();
            3
        ])
        .unwrap();
    m.world.game.tick = 0;
    m.initialize_tick_scene(200, Default::default()).unwrap();
    m.advance_tick().unwrap();
    assert_eq!(m.world.game.tick, 201);
    assert_eq!(m.world.game.vision_tick, Some(201));
    assert!(m.world.game.players[0].sees(2));
    assert_eq!(
        m.world.game.players[0].visible,
        m.world.game.players[2].visible
    );
    m.world.game.units.get_mut(&2).unwrap().x = 140.;
    for tick in 202..=204 {
        m.advance_tick().unwrap();
        assert_eq!(m.world.game.tick, tick);
        assert_eq!(m.world.game.vision_tick, Some(201));
        assert!(
            m.world.game.players[0].sees(2),
            "visibility remains cached between passes"
        );
        assert_eq!(
            m.world.game.players[0].visible,
            m.world.game.players[2].visible
        );
    }
    m.advance_tick().unwrap();
    assert_eq!(m.world.game.vision_tick, Some(205));
    assert!(!m.world.game.players[0].sees(2));
    assert_eq!(
        m.world.game.players[0].visible,
        m.world.game.players[2].visible
    );
}

#[test]
fn cmb128_los_endpoints_woods_smoke_length_and_hills() {
    let mut m = scene(&[]);
    assert!(vision::los(&m.world.game, &m.rules, (41., 41.), (61., 41.)));
    terrain(&mut m, 41., 41., b'B');
    terrain(&mut m, 61., 41., b'B');
    assert!(
        vision::los(&m.world.game, &m.rules, (41., 41.), (61., 41.)),
        "both endpoints skipped"
    );
    terrain(&mut m, 51., 41., b'B');
    assert!(!vision::los(
        &m.world.game,
        &m.rules,
        (41., 41.),
        (61., 41.)
    ));
    terrain(&mut m, 51., 41., b'.');
    for x in [45., 47., 49.] {
        terrain(&mut m, x, 41., b'O');
    }
    assert!(vision::los(&m.world.game, &m.rules, (41., 41.), (61., 41.)));
    terrain(&mut m, 53., 41., b'O');
    assert!(!vision::los(
        &m.world.game,
        &m.rules,
        (41., 41.),
        (61., 41.)
    ));
    for x in [45., 47., 49., 53.] {
        terrain(&mut m, x, 41., b'.');
    }
    m.world.game.smokes.push(crate::game::Smoke {
        id: 1,
        x: 51.,
        z: 41.,
        r: 9.,
        remaining: Some(14.),
    });
    assert!(!vision::los(
        &m.world.game,
        &m.rules,
        (41., 41.),
        (61., 41.)
    ));
    assert!(vision::los(&m.world.game, &m.rules, (45., 41.), (57., 41.)));
    m.world.game.smokes.clear();
    let c = m.world.game.cell_of(51., 41.).unwrap();
    m.world.set_level(c, 2).unwrap();
    assert!(!vision::los(
        &m.world.game,
        &m.rules,
        (41., 41.),
        (61., 41.)
    ));
}

#[test]
fn cmb129_team_spot_visibility_requires_range_and_clear_ray() {
    let mut m = scene(&[("rifle", 0, 41., 41.)]);
    assert!(vision::team_sees(
        &m.world.game,
        &m.rules,
        &m.grid,
        0,
        p(61., 41.)
    ));
    terrain(&mut m, 51., 41., b'B');
    assert!(!vision::team_sees(
        &m.world.game,
        &m.rules,
        &m.grid,
        0,
        p(61., 41.)
    ));
    assert!(!vision::team_sees(
        &m.world.game,
        &m.rules,
        &m.grid,
        0,
        p(141., 41.)
    ));
}

#[test]
fn cmb135_command_entry_rejects_winner_away_inactive_malformed_and_foreign_ids() {
    let mut m = scene(&[("rifle", 0, 40., 40.), ("rifle", 1, 60., 40.)]);
    let stop = Command::from_json(&json!({"t":"stop","ids":[1]})).unwrap();
    assert_eq!(
        m.execute_command(0, &stop, true).unwrap(),
        Some(Denial::Blocked)
    );
    assert_eq!(
        m.execute_command(9, &stop, false).unwrap(),
        Some(Denial::Blocked)
    );
    m.start.economy.players[0].away = true;
    assert_eq!(
        m.execute_command(0, &stop, false).unwrap(),
        Some(Denial::Blocked)
    );
    m.start.economy.players[0].away = false;
    m.start.economy.players[0].out = true;
    assert_eq!(
        m.execute_command(0, &stop, false).unwrap(),
        Some(Denial::Blocked)
    );
    for raw in [
        json!(null),
        json!([]),
        json!({"t":"move","orders":[[1,4]]}),
        json!({"t":"move","orders":[[1,"x",4]]}),
    ] {
        assert_eq!(Command::from_json(&raw), Err(Denial::Blocked));
    }
}

#[test]
fn cmb136_queue_limit_automatic_step_yields_rally_waits_and_stop_clears() {
    let mut m = scene(&[("rifle", 0, 40., 40.)]);
    move_to(&mut m, 1, 70., 40.);
    for _ in 0..8 {
        assert_eq!(
            command(
                &mut m,
                json!({"t":"move","orders":[[1,80,40]],"queue":true})
            ),
            None
        );
    }
    assert_eq!(
        command(
            &mut m,
            json!({"t":"move","orders":[[1,90,40]],"queue":true})
        ),
        Some(Denial::QueueFull)
    );
    m.world.game.units.get_mut(&1).unwrap().drift = true;
    m.world.game.units.get_mut(&1).unwrap().world_goal = None;
    m.commands.units.get_mut(&1).unwrap().rally_drift = true;
    m.start_queued_orders(1, false).unwrap();
    assert_eq!(m.commands.units[&1].orders.len(), 8);
    m.commands.units.get_mut(&1).unwrap().rally_drift = false;
    m.start_queued_orders(1, false).unwrap();
    assert_eq!(m.commands.units[&1].orders.len(), 7);
    assert_eq!(command(&mut m, json!({"t":"stop","ids":[1]})), None);
    assert!(m.commands.units[&1].orders.is_empty());
}

#[test]
fn cmb138_mass_move_reaches_every_selected_actor_and_clamps_coordinates() {
    let rows = (0..80)
        .map(|i| {
            (
                "rifle",
                0,
                20. + f64::from(i % 10) * 4.,
                20. + f64::from(i / 10) * 4.,
            )
        })
        .collect::<Vec<_>>();
    let mut m = scene(&rows);
    let orders = ids(&m)
        .iter()
        .map(|id| json!([id, 120, 100]))
        .collect::<Vec<_>>();
    assert_eq!(command(&mut m, json!({"t":"move","orders":orders})), None);
    assert_eq!(
        m.world
            .game
            .units
            .values()
            .filter(|u| !u.path.is_empty())
            .count(),
        80
    );
    assert_eq!(
        command(
            &mut m,
            json!({"t":"stance","key":"holdPos","ids":[1],"on":true})
        ),
        None
    );
    move_to(&mut m, 1, -5., 500.);
    let end = m.commands.units[&1].route_end.unwrap();
    assert!(end.x >= 0. && end.x <= 192. && end.z >= 0. && end.z <= 192.);
}

#[test]
fn cmb139_attack_denies_missing_allied_hidden_and_unarmed() {
    let mut m = scene(&[
        ("rifle", 0, 40., 40.),
        ("rifle", 1, 60., 40.),
        ("medic", 0, 40., 46.),
    ]);
    for target in [1, 99] {
        assert_eq!(
            command(&mut m, json!({"t":"attack","ids":[1],"target":target})),
            Some(Denial::Unseen)
        );
    }
    m.world.game.players[0].visible.clear();
    assert_eq!(
        command(&mut m, json!({"t":"attack","ids":[1],"target":2})),
        Some(Denial::Unseen)
    );
    m.world.game.players[0].visible = vec![2];
    assert_eq!(
        command(&mut m, json!({"t":"attack","ids":[3],"target":2})),
        Some(Denial::QueueFull)
    );
    assert_eq!(
        command(&mut m, json!({"t":"attack","ids":[1],"target":2})),
        None
    );
    assert_eq!(m.commands.units[&1].attack_id, 2);
    let plane = m.spawn_aircraft(1, "fighter").unwrap();
    m.world.game.units.get_mut(&plane).unwrap().air = Some(crate::game::PlaneState::Out);
    m.world.game.players[0].visible.push(plane);
    assert!(m.world.game.players[0].sees(plane));
    assert_eq!(
        command(&mut m, json!({"t":"attack","ids":[1],"target":plane})),
        Some(Denial::Needs)
    );
    assert_eq!(
        m.commands.units[&1].attack_id, 2,
        "rejected aircraft order preserves ground target"
    );
}

#[test]
fn cmb94_threat_includes_zero_range_weapon_then_recent_hit_then_move_direction() {
    use crate::command_destinations::threat_for;
    use std::collections::BTreeMap;
    let mut m = scene(&[
        ("rifle", 0, 41., 41.),
        ("rifle", 1, 61., 41.),
        ("medic", 1, 50., 41.),
    ]);
    m.world.game.units.get_mut(&1).unwrap().hit_from = Some(p(41., 65.));
    let hit = BTreeMap::from([(1, 200.)]);

    assert_eq!(
        threat_for(&destination_context(&m, &hit), 1, p(45., 41.)).unwrap(),
        Some(p(50., 41.)),
        "the prototype treats the medic weapon object as a threat candidate"
    );
    m.world.game.players[0].visible.clear();
    assert_eq!(
        threat_for(&destination_context(&m, &hit), 1, p(45., 41.)).unwrap(),
        Some(p(41., 65.))
    );
    m.world.game.tick = 301;
    assert_eq!(
        threat_for(&destination_context(&m, &hit), 1, p(45., 41.)).unwrap(),
        Some(p(65., 41.))
    );
    assert_eq!(
        threat_for(&destination_context(&m, &hit), 1, p(41., 41.)).unwrap(),
        None
    );
}

#[test]
fn cmb95_spot_search_prefers_shelter_and_excludes_held_or_unreachable_cells() {
    use crate::command_destinations::{Context, spot_near};
    use std::collections::BTreeMap;
    let mut m = scene(&[("rifle", 0, 41., 41.)]);
    terrain(&mut m, 43., 41., b'O');
    let hit = BTreeMap::new();
    let query = |m: &Match, held: &[Point]| {
        let context = Context {
            game: &m.world.game,
            rules: &m.rules,
            grid: &m.grid,
            points: &[],
            hit_at: &hit,
        };
        spot_near(
            &context,
            1,
            p(41., 41.),
            4.,
            held,
            Some(p(61., 41.)),
            3,
            None,
        )
        .unwrap()
    };
    assert_eq!(query(&m, &[]), Some(p(43., 41.)));
    assert_ne!(query(&m, &[p(43., 41.)]), Some(p(43., 41.)));
    terrain(&mut m, 43., 41., b'B');
    assert_ne!(query(&m, &[]), Some(p(43., 41.)));
}

#[test]
fn cmb96_shared_plain_move_spreads_and_hold_position_keeps_single_click() {
    let mut m = scene(&[
        ("rifle", 0, 41., 41.),
        ("rifle", 0, 41., 47.),
        ("rifle", 0, 41., 53.),
    ]);
    assert_eq!(
        command(
            &mut m,
            json!({"t":"move","orders":[[1,80,50],[2,80,50],[3,80,50]]})
        ),
        None
    );
    let spots = (1..=3)
        .map(|id| m.commands.units[&id].route_end.unwrap())
        .collect::<Vec<_>>();
    assert!(spots[0] != spots[1] && spots[0] != spots[2] && spots[1] != spots[2]);
    assert_eq!(
        command(
            &mut m,
            json!({"t":"stance","key":"holdPos","ids":[1],"on":true})
        ),
        None
    );
    move_to(&mut m, 1, 70., 41.);
    assert_eq!(m.commands.units[&1].route_end, Some(p(70., 41.)));
}

#[test]
fn cmb126_airborne_plane_seen_at_sixty_parked_plane_never_sees_or_is_seen() {
    use crate::game::PlaneState;
    let mut m = scene(&[("rifle", 0, 41., 41.)]);
    let plane = m.spawn_aircraft(1, "fighter").unwrap();
    assert_eq!(plane, 2);
    let u = m.world.game.units.get_mut(&plane).unwrap();
    u.x = 90.;
    u.z = 41.;
    rebuild(&mut m);
    for (air, visible) in [(PlaneState::Out, true), (PlaneState::Base, false)] {
        m.world.game.units.get_mut(&2).unwrap().air = Some(air);
        vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
        assert_eq!(m.world.game.players[0].sees(2), visible);
        if !visible {
            assert!(!m.world.game.players[1].sees(1));
        }
    }
    m.world.game.units.get_mut(&2).unwrap().air = Some(PlaneState::Out);
    m.world.game.units.get_mut(&2).unwrap().x = 102.;
    rebuild(&mut m);
    vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    assert!(!m.world.game.players[0].sees(2));
}

fn destination_context<'a>(
    m: &'a Match,
    hit: &'a std::collections::BTreeMap<u32, f64>,
) -> crate::command_destinations::Context<'a> {
    crate::command_destinations::Context {
        game: &m.world.game,
        rules: &m.rules,
        grid: &m.grid,
        points: &[],
        hit_at: hit,
    }
}

#[test]
fn cmb101_formation_shapes_rear_ranks_drag_and_spread() {
    use crate::formation::{Item, Shape, facing_spots, slot_size};
    assert_eq!(slot_size(true, 0.5), 3.);
    assert_eq!(slot_size(false, 2.5), 6.5);
    let items = (1..=12)
        .map(|id| Item {
            id,
            x: 0.,
            z: f64::from(id) * 4.,
            size: 3.,
            back: id == 12,
        })
        .collect::<Vec<_>>();
    for shape in Shape::ALL {
        let spots = facing_spots(&items, 80., 80., 0., 0., shape, 1.);
        assert_eq!(spots.len(), 12);
        let rear = spots.iter().find(|s| s.id == 12).unwrap();
        assert!(spots.iter().all(|s| rear.x <= s.x));
        for (i, a) in spots.iter().enumerate() {
            assert!(spots[i + 1..].iter().all(|b| a.x != b.x || a.z != b.z));
        }
    }
    let small = &items[..3];
    let tight = facing_spots(small, 80., 80., 0., 0., Shape::Line, 1.);
    let wide = facing_spots(small, 80., 80., 0., 1000., Shape::Line, 1.);
    let spread = facing_spots(small, 80., 80., 0., 0., Shape::Line, 2.);
    let width = |v: &[crate::formation::Spot]| v.last().unwrap().z - v.first().unwrap().z;
    assert!(width(&wide) > width(&tight));
    assert!(width(&spread) > width(&tight));
    assert_eq!(
        wide,
        facing_spots(small, 80., 80., 0., 2000., Shape::Line, 1.),
        "drag stretch capped"
    );
}

#[path = "combat_feature_tail_tests.rs"]
mod tail;

#[path = "combat_hold_fire_tests.rs"]
mod hold_fire_tests;
