use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::{Command, Denial},
    match_runtime::{MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};
use serde_json::json;

fn scene(kind: &str) -> (Match, UnitId) {
    let mut rows = vec![".".repeat(80); 80];
    rows[20].replace_range(20..24, "BBBB");
    rows[21].replace_range(20..24, "BBBB");
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&json!({"name":"Fire-at intent", "w":80,"h":80,"rows":rows,"spawns":[{"x":4,"y":4},{"x":75,"y":75}],"points":[{"x":40,"y":40}]})).unwrap()).unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 1804,
            mode: Mode::Conquest,
            players: (0..2)
                .map(|team| PlayerConfig {
                    name: format!("P{team}"),
                    team,
                    faction: Faction::Usa,
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Endless,
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
    let id = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    // Retain canonical IDs and all component owners while selecting a weapon profile.
    let index = m.rules.kinds.iter().position(|k| k.name == kind).unwrap();
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.kind = index;
    let definition = Catalog::compiled().unwrap().unit(kind).unwrap();
    u.hp = n(definition, "models", 1.) * n(definition, "hpPer", 1.);
    u.x = 31.;
    u.z = 41.;
    u.path.clear();
    u.still = Some(30.);
    u.hold_pos = true;
    m.grid.update(id, u);
    (m, id)
}
fn dispatch(m: &mut Match, raw: serde_json::Value) -> Option<Denial> {
    m.execute_command(0, &Command::from_json(&raw).unwrap(), false)
        .unwrap()
}
fn delivered(
    m: &mut Match,
    delivery: &mut crate::recipient_snapshot::DeliveryState,
    report: &crate::match_tick::TickReport,
) {
    let selected = (0..m.world.game.players.len())
        .map(|slot| crate::recipient_snapshot::SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frame = delivery
        .capture_selected_with_tick_events(m, &selected, &report.events)
        .unwrap()
        .unwrap();
    for seat in selected {
        assert_eq!(frame.recipient(seat.slot).unwrap().into_json()["t"], "s");
    }
}
fn snapshots(m: &Match) {
    let cache = crate::recipient_snapshot::SnapshotCache::new(m).unwrap();
    for slot in 0..m.world.game.players.len() {
        cache.recipient(slot).unwrap();
    }
}
#[test]
fn fire_at_admission_refusals_and_compound_queue_projection() {
    let (mut m, id) = scene("rifle");
    assert_eq!(
        dispatch(&mut m, json!({"t":"fireat","ids":[id],"x":-1,"z":0})),
        Some(Denial::Blocked)
    );
    assert_eq!(
        dispatch(&mut m, json!({"t":"fireat","ids":[id],"x":41,"z":41})),
        Some(Denial::Needs)
    );
    assert_eq!(
        dispatch(&mut m, json!({"t":"fireat","ids":[id],"x":143,"z":141})),
        Some(Denial::NotVisible)
    );
    let (mut m, id) = scene("tank");
    assert_eq!(
        dispatch(
            &mut m,
            json!({"t":"orders","queue":true,"commands":[{"t":"fireat","ids":[id],"x":42.2,"z":40.2}]})
        ),
        None
    );
    let cache = crate::recipient_snapshot::SnapshotCache::new(&m).unwrap();
    assert!(
        cache.recipient(0).unwrap().fields["orders"]
            .as_array()
            .unwrap()
            .contains(&json!([id, 1, 5, 43., 41.]))
    );
    assert!(
        !cache.recipient(1).unwrap().fields["orders"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r[0] == id)
    );
    for _ in 0..7 {
        assert_eq!(
            dispatch(
                &mut m,
                json!({"t":"fireat","ids":[id],"x":43,"z":41,"queue":true})
            ),
            None
        );
    }
    assert_eq!(
        dispatch(
            &mut m,
            json!({"t":"fireat","ids":[id],"x":43,"z":41,"queue":true})
        ),
        Some(Denial::QueueFull)
    );
    snapshots(&m);
    m.advance_tick().unwrap();
    snapshots(&m);
    assert_eq!(m.commands.units[&id].orders.len(), 7);
    assert_eq!(m.commands.units[&id].fire_at, Some(20 * 80 + 21));
}
#[test]
fn fire_at_direct_shells_same_structure_and_finishes_when_cell_is_gone() {
    let (mut m, id) = scene("tank");
    let cell = 20 * 80 + 23;
    assert!(m.can_shell_cell(id, cell).unwrap());
    assert_eq!(
        dispatch(&mut m, json!({"t":"fireat","ids":[id],"x":47,"z":41})),
        None
    );
    let before: f64 = m.world.game.cell_hp.iter().map(|v| f64::from(*v)).sum();
    let mut delivery = crate::recipient_snapshot::DeliveryState::new(&m).unwrap();
    let mut launched = false;
    for _ in 0..3600 {
        let report = m.advance_tick().unwrap();
        launched |= report.events.iter().any(|e| matches!(e,crate::match_tick::TickEvent::Firing(LaunchEvent::Flight{shooter,..}) if *shooter == id));
        delivered(&mut m, &mut delivery, &report);
        if m.world.game.cell_hp[cell] <= 0. && m.commands.units[&id].fire_at.is_none() {
            break;
        }
    }
    assert!(launched);
    assert!(
        m.world
            .game
            .cell_hp
            .iter()
            .map(|v| f64::from(*v))
            .sum::<f64>()
            < before
    );
    assert!(
        m.world.game.cell_hp[cell] <= 0.,
        "shells must destroy the interior target"
    );
    assert_eq!(m.commands.units[&id].fire_at, None);
}
#[test]
fn fire_at_salvo_persists_on_unseen_open_ground_until_replaced() {
    let (mut m, id) = scene("mortar");
    assert_eq!(
        dispatch(&mut m, json!({"t":"fireat","ids":[id],"x":71,"z":71})),
        None
    );
    let cell = m.commands.units[&id].fire_at;
    let mut delivery = crate::recipient_snapshot::DeliveryState::new(&m).unwrap();
    let mut launches = 0;
    for _ in 0..240 {
        let report = m.advance_tick().unwrap();
        launches += report.events.iter().filter(|e|matches!(e,crate::match_tick::TickEvent::Firing(LaunchEvent::Salvo{shooter,..}) if *shooter == id)).count();
        delivered(&mut m, &mut delivery, &report);
        assert_eq!(m.commands.units[&id].fire_at, cell);
    }
    assert!(launches >= 2);
    assert_eq!(dispatch(&mut m, json!({"t":"stop","ids":[id]})), None);
    m.advance_tick().unwrap();
    snapshots(&m);
    assert_eq!(m.commands.units[&id].fire_at, None);
    assert_eq!(
        dispatch(&mut m, json!({"t":"fireat","ids":[id],"x":71,"z":71})),
        None
    );
    assert_eq!(
        dispatch(
            &mut m,
            json!({"t":"fireat","ids":[id],"x":73,"z":71,"queue":true})
        ),
        None
    );
    m.start.economy.players[0].vp = m.start.win_vp;
    for _ in 0..20 {
        let report = m.advance_tick().unwrap();
        delivered(&mut m, &mut delivery, &report);
        if m.finish.is_some() {
            break;
        }
    }
    assert!(m.finish.is_some());
    assert_eq!(m.commands.units[&id].fire_at, cell);
    assert_eq!(m.commands.units[&id].orders.len(), 1);
    assert_eq!(m.tick_state.fault(), None);
}

#[test]
fn fire_at_ammo_debits_actual_rounds_and_shortage_does_not_stack() {
    for kind in ["tank", "rocket"] {
        for (ammo, exhausted, expected_rate) in [
            (100., false, 1.),
            (55., false, 1.),
            (2.9, false, 2.),
            (100., true, 2.),
            (2.9, true, 2.),
            (0.9, false, 2.),
        ] {
            let (mut m, id) = scene(kind);
            let def = crate::logistics::Definition::from_catalog(kind).unwrap();
            m.logistics
                .initialize_unit(
                    crate::logistics::Context {
                        enabled_override: Some(true),
                        scenario: false,
                        mode: crate::logistics::Mode::Conquest,
                    },
                    crate::logistics::UnitFacts {
                        id,
                        kind,
                        owner: Some(0),
                        owner_exists: true,
                        guard_home: false,
                        riding: false,
                    },
                    Some(&def),
                )
                .unwrap();
            let r = m.logistics.get_mut(id).unwrap();
            r.ammo = ammo;
            r.ammo_max = 200.;
            r.exhausted = exhausted;
            let at = if kind == "tank" {
                Point { x: 43., z: 41. }
            } else {
                Point { x: 71., z: 71. }
            };
            assert_eq!(
                dispatch(&mut m, json!({"t":"fireat","ids":[id],"x":at.x,"z":at.z})),
                None
            );
            m.commands.units.get_mut(&id).unwrap().hold_fire = true;
            m.apply_unit_firing(id, false, 0.05).unwrap();
            let requested = if kind == "tank" { 1. } else { 8. };
            let rounds = math::min(requested, ammo.floor());
            assert_eq!(m.logistics.get(id).unwrap().ammo, ammo - rounds);
            assert_eq!(m.firing.flights.len(), rounds as usize);
            let interval = n(
                m.weapon(&m.world.game.units[&id]).unwrap().unwrap(),
                "interval",
                0.,
            );
            let expected_rate = if kind == "rocket" && ammo == 55. {
                2.
            } else {
                expected_rate
            };
            assert_eq!(
                m.firing.get(id).unwrap().cooldown,
                if rounds > 0. {
                    interval * expected_rate
                } else {
                    0.
                }
            );
            if rounds > 0. {
                m.firing.get_mut(id).unwrap().cooldown = 0.;
            }
            m.logistics.get_mut(id).unwrap().ammo = 0.;
            let before = m.firing.next_flight;
            m.apply_unit_firing(id, false, 0.05).unwrap();
            assert_eq!(m.firing.next_flight, before);
        }
    }
}

#[test]
fn fire_at_refuses_pending_aimed_ability_without_changing_it() {
    let (mut m, id) = scene("churchill");
    m.commands.units.get_mut(&id).unwrap().nade = Some(Point { x: 63., z: 41. });
    let before = format!("{:?}", (&m.world.game.units, &m.commands.units));
    assert_eq!(
        dispatch(&mut m, json!({"t":"fireat","ids":[id],"x":41,"z":41})),
        Some(Denial::Blocked)
    );
    assert_eq!(
        format!("{:?}", (&m.world.game.units, &m.commands.units)),
        before
    );
}

#[test]
fn fire_at_closes_to_range_and_does_not_treat_another_structure_as_its_wall() {
    let (mut m, id) = scene("tank");
    let cell = 20 * 80 + 23;
    let wall = 20 * 80 + 20;
    let original = m.world.sections[&wall].structure_id.clone();
    m.world.sections.get_mut(&wall).unwrap().structure_id = "separate-structure".into();
    assert!(!m.can_shell_cell(id, cell).unwrap());
    m.world.sections.get_mut(&wall).unwrap().structure_id = original;
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = 5.;
    u.z = 41.;
    u.hold_pos = false;
    u.path.clear();
    m.grid.update(id, u);
    assert_eq!(
        dispatch(&mut m, json!({"t":"fireat","ids":[id],"x":47,"z":41})),
        None
    );
    let start = m.world.game.units[&id].x;
    let mut delivery = crate::recipient_snapshot::DeliveryState::new(&m).unwrap();
    let mut fired = false;
    for _ in 0..240 {
        let report = m.advance_tick().unwrap();
        fired |= report.events.iter().any(|e|matches!(e,crate::match_tick::TickEvent::Firing(LaunchEvent::Flight{shooter,..}) if *shooter==id));
        delivered(&mut m, &mut delivery, &report);
        if fired {
            break;
        }
    }
    assert!(m.world.game.units[&id].x > start + 1.);
    assert!(fired);
}

#[test]
fn fire_at_terrain_contact_scales_weapon_damage_by_quarter_and_energy() {
    let (mut m, id) = scene("tank");
    let cell = 20 * 80 + 20;
    let at = m.fire_at_center(cell).unwrap();
    let shooter = m.world.game.units[&id].clone();
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let before = m.world.game.cell_hp[cell];
    let (mut damage, retarget) = m.firing_damage_view(&order);
    let (taken, _) = damage
        .local_contact(
            crate::match_combat::FlightSource {
                shooter: id,
                kind: shooter.kind,
                owner: 0,
                airborne: false,
            },
            crate::match_combat::ContactTarget::Cell(cell),
            crate::combat::Impact {
                owner: 0,
                shooter: id,
                weapon: crate::combat::WeaponDamage::from_catalog("tank").unwrap(),
                damage_scale: Some(1.),
                energy: 0.5,
                armor_piercing: false,
                ap_naval_only: false,
                ap_multiplier: 1.,
                launch_origin: shooter.at(),
                at,
                direction: Point { x: 1., z: 0. },
            },
            "fire-at-contact",
            None,
            retarget,
        )
        .unwrap();
    assert_eq!(taken, 90. * 0.25 * 0.5);
    assert_eq!(f64::from(before - m.world.game.cell_hp[cell]), taken);
}

#[test]
fn fire_at_live_flight_is_visible_without_disclosing_its_hidden_shooter() {
    let (mut m, id) = scene("mortar");
    m.launch_weapon_salvo(id, Point { x: 71., z: 71. }, None)
        .unwrap();
    m.firing.take_events();
    m.world.game.tick = 20;
    let observer = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 1 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let u = m.world.game.units.get_mut(&observer).unwrap();
    u.x = 71.;
    u.z = 71.;
    u.path.clear();
    m.grid.update(observer, u);
    m.world.game.players[1].visible.clear();
    let snapshot = crate::recipient_snapshot::SnapshotCache::new(&m)
        .unwrap()
        .recipient(1)
        .unwrap();
    let rows = snapshot.fields["flights"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert!(row.get("f").is_none());
    assert!(row.get("fo").is_none());
    assert_eq!(row["kind"], "mortar");
    assert_eq!(row["time"], 1.);
    assert!(row["until"].as_f64().unwrap() <= 1.15);
    assert!(row["until"].as_f64().unwrap() >= 1.);
    m.world.game.units.get_mut(&observer).unwrap().x = 150.;
    m.world.game.units.get_mut(&observer).unwrap().z = 150.;
    let u = &m.world.game.units[&observer];
    m.grid.update(observer, u);
    assert_eq!(
        crate::recipient_snapshot::SnapshotCache::new(&m)
            .unwrap()
            .recipient(1)
            .unwrap()
            .fields["flights"],
        json!([])
    );
}

#[test]
fn fire_at_preserves_aircraft_area_dispatch_and_mixed_selection_refusal() {
    let (mut m, tank) = scene("tank");
    let bomber = m.spawn_aircraft(0, "bomber").unwrap();
    let before = serde_json::to_value((&m.commands, &m.aviation, &m.world.game)).unwrap();
    assert_eq!(
        dispatch(
            &mut m,
            json!({"t":"fireat","ids":[tank,bomber],"x":43.,"z":41.})
        ),
        Some(Denial::Unsupported)
    );
    assert_eq!(
        serde_json::to_value((&m.commands, &m.aviation, &m.world.game)).unwrap(),
        before
    );
    assert_eq!(
        dispatch(&mut m, json!({"t":"fireat","ids":[bomber],"x":43.,"z":41.})),
        None
    );
    let mission = m.aviation.planes[&bomber].mission.unwrap();
    assert_eq!(mission.kind, crate::match_aviation::MissionKind::Area);
    assert_eq!(m.commands.units[&tank].fire_at, None);
    assert_eq!(
        dispatch(&mut m, json!({"t":"fireat","ids":[tank],"x":43.,"z":41.})),
        None
    );
    assert_eq!(m.commands.units[&tank].fire_at, Some(20 * 80 + 21));
    assert_eq!(m.aviation.planes[&bomber].mission, Some(mission));
}

#[test]
fn fire_at_mixed_aircraft_and_ground_sheller_keeps_main_refusal_atomic() {
    for queue in [false, true] {
        let (mut m, id) = scene("howitzer");
        let bomber = m.spawn_aircraft(0, "bomber").unwrap();
        let before = format!("{:?}", (&m.world, &m.commands.units, &m.aviation));
        let random = m.start.random.snapshot();
        assert_eq!(
            dispatch(
                &mut m,
                json!({"t":"fireat","ids":[bomber,id],"x":41,"z":41,"queue":queue})
            ),
            Some(Denial::Unsupported)
        );
        assert_eq!(
            format!("{:?}", (&m.world, &m.commands.units, &m.aviation)),
            before
        );
        assert_eq!(m.start.random.snapshot(), random);
    }
}

#[test]
fn fire_at_dead_aircraft_keeps_main_needs_refusal_without_ground_validation() {
    let (mut m, _) = scene("howitzer");
    let bomber = m.spawn_aircraft(0, "bomber").unwrap();
    m.world.game.units.get_mut(&bomber).unwrap().hp = 0.0;
    let before = format!("{:?}", (&m.world, &m.commands.units, &m.aviation));
    assert_eq!(
        dispatch(&mut m, json!({"t":"fireat","ids":[bomber],"x":41,"z":41})),
        Some(Denial::Needs)
    );
    assert_eq!(
        format!("{:?}", (&m.world, &m.commands.units, &m.aviation)),
        before
    );
}
