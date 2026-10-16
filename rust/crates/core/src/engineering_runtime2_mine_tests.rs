use super::*;
use crate::{match_commands::MoveOrder, match_tick::TickEvent};

fn quiet(m: &mut Match) {
    m.options.tech = false;
    for control in m.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_retreat = false;
    }
}

#[test]
fn eng64_moved_enemy_detonates_once_with_collateral_falloff_and_layer_credit() {
    let mut m = scene();
    let at = Point { x: 53., z: 51. };
    let cell = m.world.game.cell_of(at.x, at.z).unwrap();
    let friendly = publish(&mut m, "tank", 0, at);
    let enemy = publish(&mut m, "tank", 1, at);
    let center_foot = publish(&mut m, "rifle", 0, at);
    let edge_tank = publish(&mut m, "tank", 0, Point { x: 56., z: 51. });
    let middle_foot = publish(&mut m, "rifle", 1, Point { x: 53., z: 52.5 });
    quiet(&mut m);
    m.world.set_cell(cell, b'N').unwrap();
    m.world.mines.insert(cell, 0);
    m.world.set_cell(cell + 1, b'N').unwrap();
    m.world.mines.insert(cell + 1, 1);
    let before = Pose {
        x: 52.9,
        z: 51.,
        rot: 0.,
    };
    let mut effects = Vec::new();
    m.apply_moved_ground_contact_emitting(
        enemy,
        m.world.game.units[&enemy].pose(),
        None,
        &mut |e| effects.push(e),
    )
    .unwrap();
    assert_eq!(
        m.world.game.chars[cell], b'N',
        "standing on a mine does not trigger it"
    );
    m.apply_moved_ground_contact_emitting(friendly, before, None, &mut |e| effects.push(e))
        .unwrap();
    assert_eq!(
        m.world.game.chars[cell], b'N',
        "a friendly vehicle can cross"
    );
    assert!(effects.is_empty());
    let ids = [friendly, enemy, center_foot, edge_tank, middle_foot];
    let health = ids.map(|id| m.world.game.units[&id].hp);
    m.apply_moved_ground_contact_emitting(enemy, before, None, &mut |e| effects.push(e))
        .unwrap();
    assert_eq!(m.world.game.chars[cell], b'+');
    assert_eq!(m.world.mine_layer[cell], 0);
    assert!(!m.world.mines.contains_key(&cell));
    assert_eq!(
        m.world.game.chars[cell + 1],
        b'N',
        "mine blasts do not chain"
    );
    assert_eq!(m.world.mines[&(cell + 1)], 1);
    for ((id, hp), damage) in ids
        .into_iter()
        .zip(health)
        .zip([220., 220., 45., 110., 33.75])
    {
        close(hp - m.world.game.units[&id].hp, damage);
        assert_eq!(m.combat.get(id).unwrap().last_hit, Some(0));
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Hurt(h) if h.unit == id && h.from == Some(0)))
        );
    }
    close(m.world.game.units[&center_foot].supp, 60.);
    close(m.world.game.units[&middle_foot].supp, 60.);
    let health = ids.map(|id| m.world.game.units[&id].hp);
    effects.clear();
    m.apply_moved_ground_contact_emitting(friendly, before, None, &mut |e| effects.push(e))
        .unwrap();
    assert_eq!(ids.map(|id| m.world.game.units[&id].hp), health);
    assert!(
        effects.is_empty(),
        "a later mover cannot detonate the spent mine"
    );

    // Authored map mines have no layer. Either side can trigger them.
    for owner in 0..2 {
        let mut m = scene();
        let id = publish(&mut m, "rifle", owner, at);
        quiet(&mut m);
        m.world.set_cell(cell, b'N').unwrap();
        assert_eq!(m.world.mines[&cell], -1);
        let hp = m.world.game.units[&id].hp;
        let mut events = Vec::new();
        m.apply_moved_ground_contact_emitting(id, before, None, &mut |e| events.push(e))
            .unwrap();
        assert_eq!(m.world.game.chars[cell], b'+');
        close(hp - m.world.game.units[&id].hp, 45.);
        assert_eq!(m.combat.get(id).unwrap().last_hit, None);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Effect::Hurt(h) if h.unit == id && h.from == Some(-1)))
        );
    }

    for killed in [false, true] {
        let mut m = scene();
        let id = publish(&mut m, "tank", 1, Point { x: 51., z: 51. });
        quiet(&mut m);
        if killed {
            m.world.game.units.get_mut(&id).unwrap().hp = 1.;
        }
        m.world.set_cell(cell, b'N').unwrap();
        m.world.mines.insert(cell, 0);
        assert_eq!(
            m.execute_command(
                1,
                &Command::Move {
                    orders: vec![MoveOrder {
                        unit: i64::from(id),
                        at: Point { x: 57., z: 51. }
                    }],
                    control_ids: None,
                    attack_move: false,
                    queue: false,
                    together: false,
                    face: None,
                },
                false
            )
            .unwrap(),
            None
        );
        let mut hits = Vec::new();
        for _ in 0..200 {
            let report = m.advance_tick().unwrap();
            hits.extend(report.events.into_iter().filter_map(|e| match e {
                TickEvent::Environment(Effect::Hurt(h)) if h.unit == id => Some(h),
                _ => None,
            }));
            if m.world.game.chars[cell] != b'N' {
                break;
            }
        }
        assert_eq!(
            m.world.game.chars[cell], b'+',
            "real movement reaches and detonates the mine"
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].from, Some(0));
        assert_eq!(hits[0].killed, killed);
        assert_eq!(m.world.game.units.contains_key(&id), !killed);
    }
}
