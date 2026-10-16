//! Fire-at ingress uses the recorded queue and ordinary room delivery.
use super::*;
use crate::room_runtime::{FrameKind, RoomRuntime};
use serde_json::json;
use ww2_core::{map::AuthoritativeMap, recipient_snapshot::SelectedSeat};

fn scene(kind: &str) -> (RoomRuntime, u32, CommanderIngress) {
    let mut rows = vec![".".repeat(80); 80];
    if kind == "destroyer" {
        for row in rows.iter_mut().take(26).skip(16) {
            row.replace_range(8..19, "WWWWWWWWWWW");
        }
    }
    rows[20].replace_range(20..24, "BBBB");
    rows[21].replace_range(20..24, "BBBB");
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Room fire-at", "w":80,"h":80,"rows":rows,
            "spawns":[{"x":4,"y":4},{"x":75,"y":75}],"points":[{"x":40,"y":40}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut options = crate::lobby::native_tests::native_match().options.clone();
    options.tech = false;
    options.map_key = None;
    let mut state = Match::new(&map, options).unwrap();
    let id = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let index = state
        .rules
        .kinds
        .iter()
        .position(|k| k.name == kind)
        .unwrap();
    if kind == "destroyer" {
        let unit = ww2_core::unit_spawn::initial_unit(
            &state.rules,
            ww2_core::catalog::Catalog::compiled().unwrap(),
            index,
            id,
            0,
            ww2_core::game::Point { x: 31., z: 41. },
        )
        .unwrap();
        let control = ww2_core::match_commands::UnitControl::new_for_mode(
            &unit,
            &state.rules,
            state.options.mode,
        )
        .unwrap();
        state.commands.units.insert(id, control);
        state.world.game.units.insert(id, unit);
    }
    let u = state.world.game.units.get_mut(&id).unwrap();
    u.kind = index;
    let definition = ww2_core::catalog::Catalog::compiled()
        .unwrap()
        .unit(kind)
        .unwrap();
    u.hp = definition
        .get("models")
        .and_then(|v| v.number())
        .unwrap_or(1.)
        * definition
            .get("hpPer")
            .and_then(|v| v.number())
            .unwrap_or(1.);
    u.x = 31.;
    u.z = 41.;
    u.path.clear();
    u.still = Some(30.);
    u.hold_pos = true;
    state.grid.update(id, u);
    (
        RoomRuntime::new(state, 0).unwrap(),
        id,
        CommanderIngress::default(),
    )
}

fn step(
    room: &mut RoomRuntime,
    ingress: &mut CommanderIngress,
) -> Vec<(usize, &'static str, Denial)> {
    let report = room.advance_tick(&[false, false]).unwrap();
    let feedback = ingress.complete(&report.commands);
    let seats = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frames = room.collect_frame(&seats, FrameKind::Scheduled).unwrap();
    assert_eq!(frames.len(), 2);
    for frame in frames {
        assert_eq!(frame.message["t"], "s");
    }
    assert!(room.fault().is_none());
    feedback
}
fn issue(room: &mut RoomRuntime, ingress: &mut CommanderIngress, raw: Value) -> Option<Denial> {
    match ingress.enqueue(room.state_mut(), 0, &raw) {
        Err(denial) => Some(denial),
        Ok(()) => {
            let feedback = step(room, ingress);
            assert!(
                feedback
                    .iter()
                    .all(|(slot, name, _)| *slot == 0 && *name == "fireat")
            );
            feedback.first().map(|(_, _, denial)| *denial)
        }
    }
}

#[test]
fn fireat_ingress_preserves_denials_and_direct_shell_eligibility() {
    let (mut room, id, mut ingress) = scene("rifle");
    for (x, z, denial) in [
        (160., 0., Denial::Blocked),
        (41., 41., Denial::Needs),
        (143., 141., Denial::NotVisible),
    ] {
        assert_eq!(
            issue(
                &mut room,
                &mut ingress,
                json!({"t":"fireat","ids":[id],"x":x,"z":z})
            ),
            Some(denial)
        );
    }
    let (mut room, id, mut ingress) = scene("tank");
    assert_eq!(
        issue(
            &mut room,
            &mut ingress,
            json!({"t":"fireat","ids":[id],"x":31,"z":41})
        ),
        Some(Denial::Blocked)
    );
    assert_eq!(
        issue(
            &mut room,
            &mut ingress,
            json!({"t":"fireat","ids":[id],"x":43,"z":41})
        ),
        None
    );
    assert_eq!(room.state().commands.units[&id].fire_at, Some(20 * 80 + 21));
    for _ in 0..8 {
        assert!(step(&mut room, &mut ingress).is_empty());
    }
}

#[test]
fn fireat_ingress_salvo_unseen_ground_queue_limit_and_replacement() {
    let (mut room, id, mut ingress) = scene("mortar");
    assert_eq!(
        issue(
            &mut room,
            &mut ingress,
            json!({"t":"fireat","ids":[id],"x":71,"z":71})
        ),
        None
    );
    let original = room.state().commands.units[&id].fire_at;
    for _ in 0..8 {
        ingress
            .enqueue(
                room.state_mut(),
                0,
                &json!({"t":"fireat","ids":[id],"x":73,"z":71,"queue":true}),
            )
            .unwrap();
    }
    ingress
        .enqueue(
            room.state_mut(),
            0,
            &json!({"t":"fireat","ids":[id],"x":73,"z":71,"queue":true}),
        )
        .unwrap();
    assert_eq!(
        step(&mut room, &mut ingress),
        vec![(0, "fireat", Denial::QueueFull)]
    );
    assert_eq!(room.state().commands.units[&id].orders.len(), 8);
    assert_eq!(room.state().commands.units[&id].fire_at, original);
    assert_eq!(
        issue(
            &mut room,
            &mut ingress,
            json!({"t":"fireat","ids":[id],"x":75,"z":71})
        ),
        None
    );
    assert!(room.state().commands.units[&id].orders.is_empty());
    assert_ne!(room.state().commands.units[&id].fire_at, original);
    for _ in 0..8 {
        assert!(step(&mut room, &mut ingress).is_empty());
    }
}

#[test]
fn fireat_ingress_bomber_area_and_mixed_ground_queue_reach_both_seats() {
    let (mut room, gun, mut ingress) = scene("mortar");
    let bomber = room.state_mut().spawn_aircraft(0, "bomber").unwrap();
    assert_eq!(
        issue(
            &mut room,
            &mut ingress,
            json!({"t":"fireat","ids":[bomber],"x":71,"z":71,"queue":true})
        ),
        None
    );
    assert_eq!(
        room.state().aviation.planes[&bomber].mission.unwrap().kind,
        ww2_core::match_aviation::MissionKind::Area
    );
    assert!(room.state().commands.units[&bomber].orders.is_empty());
    assert_eq!(
        issue(
            &mut room,
            &mut ingress,
            json!({"t":"fireat","ids":[gun],"x":69,"z":71})
        ),
        None
    );
    assert_eq!(
        issue(
            &mut room,
            &mut ingress,
            json!({"t":"fireat","ids":[bomber,gun],"x":73,"z":71,"queue":true})
        ),
        None
    );
    assert_eq!(room.state().commands.units[&gun].orders.len(), 1);
    assert!(room.state().commands.units[&bomber].orders.is_empty());
    let mission = room.state().aviation.planes[&bomber].mission.unwrap();
    assert_eq!(mission.kind, ww2_core::match_aviation::MissionKind::Area);
    assert_eq!(mission.at.x, 73.);
    assert_eq!(
        issue(
            &mut room,
            &mut ingress,
            json!({"t":"fireat","ids":[bomber,gun],"x":77,"z":71})
        ),
        None
    );
    assert!(room.state().commands.units[&gun].orders.is_empty());
    assert_eq!(
        room.state().commands.units[&gun].fire_at,
        Some(35 * 80 + 38)
    );
    assert_eq!(
        room.state().aviation.planes[&bomber].mission.unwrap().at.x,
        77.
    );
    for _ in 0..8 {
        assert!(step(&mut room, &mut ingress).is_empty());
    }
}

#[test]
fn fireat_ingress_destroyer_shells_open_ground_through_ordinary_delivery() {
    let (mut room, ship, mut ingress) = scene("destroyer");
    assert!(room.state().rules.kinds[room.state().world.game.units[&ship].kind].naval);
    assert_eq!(
        issue(
            &mut room,
            &mut ingress,
            json!({"t":"fireat","ids":[ship],"x":101,"z":81})
        ),
        None
    );
    assert_eq!(
        room.state().commands.units[&ship].fire_at,
        Some(40 * 80 + 50)
    );
    for _ in 0..8 {
        assert!(step(&mut room, &mut ingress).is_empty());
    }
    // Keep the order on unfortified ground inside the ship's vision so the
    // ordinary recipient stream can witness a real salvo, not just admission.
    assert_eq!(
        issue(
            &mut room,
            &mut ingress,
            json!({"t":"fireat","ids":[ship],"x":31,"z":81})
        ),
        None
    );
    let seats = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let mut salvo = false;
    let mut projectile = false;
    for _ in 0..160 {
        let report = room.advance_tick(&[false, false]).unwrap();
        assert!(ingress.complete(&report.commands).is_empty());
        projectile |= room
            .state()
            .firing
            .flights
            .values()
            .any(|flight| flight.shooter == ship);
        let frames = room.collect_frame(&seats, FrameKind::Scheduled).unwrap();
        assert_eq!(frames.len(), 2);
        for frame in frames {
            assert_eq!(frame.message["t"], "s");
            if frame.slot == 0 {
                salvo |= frame.message["shots"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|shot| shot["k"] == "salvo" && shot["f"] == ship);
            }
        }
        assert!(room.fault().is_none());
        assert_eq!(
            room.state().commands.units[&ship].fire_at,
            Some(40 * 80 + 15)
        );
    }
    assert!(salvo, "owner must receive the destroyer's real salvo event");
    assert!(projectile, "destroyer must launch real shell flights");
}
