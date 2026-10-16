//! Native command intent checks for the third combat runtime stream.
use super::*;

#[test]
fn cmb144_retreat_is_immediate_for_mixed_air_ground_and_working_units() {
    use crate::game::PlaneState;
    let mut m = scene(&[("engineer", 0, 40., 40.), ("rifle", 1, 64., 64.)]);
    let plane = m.spawn_aircraft(0, "fighter").unwrap();
    m.world.game.units.get_mut(&plane).unwrap().air = Some(PlaneState::Station);
    let site = m.next_unit_id;
    assert_eq!(
        command(
            &mut m,
            json!({"t":"build","ids":[1],"kind":"barracks","x":48,"z":42})
        ),
        None
    );
    assert!(m.world.game.units[&1].build);
    assert_eq!(m.commands.units[&1].build_target, Some(site));
    let paid = m.start.economy.players[0].mp;
    let site_before = m.world.game.units[&site].clone();
    assert_eq!(
        command(
            &mut m,
            json!({"t":"move","orders":[[1,70,70]],"queue":true})
        ),
        None
    );
    m.commands
        .units
        .get_mut(&plane)
        .unwrap()
        .orders
        .push_back(Command::Stop {
            ids: vec![i64::from(plane)],
        });
    let foreign = m.world.game.units[&2].clone();
    let foreign_control = m.commands.units[&2].clone();
    assert_eq!(
        command(
            &mut m,
            json!({"t":"retreat","ids":[1,plane,2,999999],"queue":true})
        ),
        None
    );
    assert!(m.world.game.units[&1].retreating);
    assert!(!m.world.game.units[&1].build);
    assert_eq!(m.commands.units[&1].build_target, None);
    assert!(m.world.game.units[&1].world_goal.is_some());
    assert!(!m.world.game.units[&1].path.is_empty());
    assert!(m.commands.units[&1].orders.is_empty());
    assert_eq!(m.world.game.units[&plane].air, Some(PlaneState::Home));
    assert!(m.commands.units[&plane].orders.is_empty());
    assert_eq!(
        m.start.economy.players[0].mp, paid,
        "retreat does not refund construction"
    );
    assert_eq!(
        format!("{:?}", m.world.game.units[&site]),
        format!("{site_before:?}"),
        "retreat leaves the unfinished site"
    );
    assert_eq!(
        format!("{:?}", m.world.game.units[&2]),
        format!("{foreign:?}")
    );
    assert_eq!(m.commands.units[&2], foreign_control);
    assert_eq!(
        command(&mut m, json!({"t":"retreat","ids":[2,999999]})),
        Some(Denial::Needs)
    );
    m.options.mode = Mode::Classic;
    let before = format!("{:?}", (&m.world.game.units, &m.commands.units));
    assert_eq!(
        command(&mut m, json!({"t":"retreat","ids":[plane]})),
        Some(Denial::Unsupported),
        "unavailable aircraft modes retain ordinary command denial"
    );
    assert_eq!(
        format!("{:?}", (&m.world.game.units, &m.commands.units)),
        before
    );
}

#[test]
fn cmb137_compound_clicks_preserve_outer_queue_issuer_limit_and_first_denial() {
    let mut m = scene(&[("rifle", 0, 40., 40.), ("rifle", 1, 64., 64.)]);
    move_to(&mut m, 1, 50., 40.);
    let foreign = m.world.game.units[&2].clone();
    for outer in [true, false] {
        assert_eq!(
            command(
                &mut m,
                json!({"t":"orders","queue":outer,"commands":[
                    {"t":"move","queue":!outer,"orders":[[1,60,40],[2,70,70]]},
                    {"t":"amove","queue":!outer,"orders":[[1,70,40]]}
                ]})
            ),
            None
        );
        assert_eq!(
            format!("{:?}", m.world.game.units[&2]),
            format!("{foreign:?}")
        );
        let c = &m.commands.units[&1];
        if outer {
            assert_eq!(c.orders.len(), 2);
            assert!(matches!(
                c.orders[0],
                Command::Move {
                    attack_move: false,
                    queue: false,
                    ..
                }
            ));
            assert!(matches!(
                c.orders[1],
                Command::Move {
                    attack_move: true,
                    queue: false,
                    ..
                }
            ));
        } else {
            assert!(c.orders.is_empty());
            assert_eq!(m.world.game.units[&1].amove, Some(p(70., 40.)));
        }
    }
    assert_eq!(
        command(
            &mut m,
            json!({"t":"orders","commands":[
                {"t":"attack","ids":[1],"target":999999},
                {"t":"move","orders":[[2,20,20]]}
            ]})
        ),
        Some(Denial::Unseen)
    );
    assert_eq!(
        command(
            &mut m,
            json!({"t":"orders","commands":[
                {"t":"move","orders":[[2,20,20]]},
                {"t":"amove","orders":[[1,65,40]]}
            ]})
        ),
        None,
        "one accepted part makes the click succeed"
    );
    assert_eq!(
        command(
            &mut m,
            json!({"t":"orders","commands":[
                {"t":"move","orders":[[1,"bad",40]]},
                {"t":"amove","orders":[[1,66,40]]}
            ]})
        ),
        None,
        "malformed parts do not block later parts"
    );
    let before = m.world.game.units[&1].clone();
    let foreign_move = json!({"t":"move","orders":[[2,20,20]]});
    let denial = command(&mut m, foreign_move.clone());
    assert!(denial.is_some());
    let mut parts = vec![foreign_move; 8];
    parts.push(json!({"t":"move","orders":[[1,10,10]]}));
    assert_eq!(
        command(&mut m, json!({"t":"orders","commands":parts})),
        denial
    );
    assert_eq!(
        format!("{:?}", m.world.game.units[&1]),
        format!("{before:?}"),
        "ninth part is not dispatched"
    );
    assert_eq!(
        command(
            &mut m,
            json!({"t":"orders","commands":[
                null, {"t":"stop","ids":[1]}, {"t":"orders","commands":[{"t":"move","orders":[[1,10,10]]}]}
            ]})
        ),
        Some(Denial::Blocked)
    );
    assert_eq!(
        format!("{:?}", m.world.game.units[&1]),
        format!("{before:?}"),
        "unlisted and nested commands are ignored"
    );
}
