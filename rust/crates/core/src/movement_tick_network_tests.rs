use super::runtime_tests::{arena, place, troop};
use crate::{
    game::Point,
    match_tick::TickPhase,
    recipient_snapshot::{DeliveryState, SelectedSeat},
};

#[test]
fn log21_network_is_initialized_then_refreshes_only_on_twentieth_ticks_for_active_teams() {
    let mut m = arena(true);
    assert_eq!(
        m.supply_net
            .teams
            .iter()
            .map(|n| n.team)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert!(m.supply_net.teams.iter().all(|n| n.steps.contains(&0)));
    for u in m.world.game.units.values_mut() {
        u.hold_pos = true;
    }
    for c in m.commands.units.values_mut() {
        c.auto_cast = false;
        c.auto_retreat = false;
        c.hold_fire = true;
    }
    let initial = m.supply_net.clone();
    // A full river disconnects the terrain after creation. The cached network
    // must retain the initial connectivity until its own beat, not every tick.
    for z in 0..m.world.game.h {
        m.world.set_cell(z * m.world.game.w + 40, b'W').unwrap();
    }
    let mut delivery = DeliveryState::new(&m).unwrap();
    let seats = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    for tick in 1..=40 {
        if tick == 21 {
            m.start.economy.players[1].out = true;
        }
        let report = m.advance_tick().unwrap();
        assert_eq!(
            report.phases.contains(&TickPhase::SupplyNetwork),
            tick % 20 == 0
        );
        if tick < 20 {
            assert_eq!(m.supply_net, initial);
        }
        if tick == 20 {
            assert_ne!(m.supply_net, initial);
        }
        if (20..40).contains(&tick) {
            assert_eq!(m.supply_net.teams.len(), 2);
        }
        if tick == 40 {
            assert_eq!(
                m.supply_net
                    .teams
                    .iter()
                    .map(|n| n.team)
                    .collect::<Vec<_>>(),
                vec![0]
            );
        }
        if tick % 20 == 0 {
            let index = |phase| report.phases.iter().position(|p| *p == phase).unwrap();
            assert!(index(TickPhase::PointSupply) < index(TickPhase::SupplyNetwork));
            assert!(index(TickPhase::SupplyNetwork) < index(TickPhase::Reinforcement));
            assert!(index(TickPhase::Reinforcement) < index(TickPhase::Death));
        }
        let frame = delivery
            .capture_selected_with_tick_events(&mut m, &seats, &report.events)
            .unwrap()
            .unwrap();
        for slot in 0..2 {
            frame.recipient(slot).unwrap();
        }
        assert!(m.tick_state.fault().is_none());
    }
}

#[test]
fn log21_zero_network_beat_freezes_existing_cache_without_disabling_creation() {
    let mut m = arena(true);
    let id = troop(&m);
    place(&mut m, id, Point { x: 80., z: 80. });
    let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let original = m.supply_net.clone();
    // An explicit test-hook beat is transient. It adds no checkpoint state.
    m.start.economy.players[1].out = true;
    m.world.game.tick = 20;
    assert!(!m.refresh_supply_network_on_beat(&ids, 0).unwrap());
    assert_eq!(m.supply_net, original);
    assert!(m.refresh_supply_network_on_beat(&ids, 20).unwrap());
    assert_eq!(m.supply_net.teams.len(), 1);
    assert_ne!(m.supply_net, original);
}
