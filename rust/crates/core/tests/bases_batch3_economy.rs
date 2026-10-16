//! Bounty and supply scheduling rules from pinned shared/sim.js:746,5655,5695-5697.
use ww2_core::{
    economy::Mode,
    economy_state::{EconomyState, ModeContext, PlayerEconomy},
    objectives::point_supply_due,
};

fn wallets() -> EconomyState {
    EconomyState {
        players: [0, 0, 1, 2]
            .into_iter()
            .map(|team| PlayerEconomy {
                team,
                mp: 123.5,
                ..Default::default()
            })
            .collect(),
    }
}

#[test]
fn base09_bounty_uses_conquest_cost_and_excludes_allies_out_and_horde_ai() {
    for kind in [
        Mode::Default,
        Mode::Conquest,
        Mode::Classic,
        Mode::World,
        Mode::Assault,
        Mode::Annihilation,
        Mode::Horde,
        Mode::Tutorial,
    ] {
        let mut state = wallets();
        let mode = ModeContext {
            kind,
            income_excluded_slot: Some(3),
            ..Default::default()
        };
        let before = state.clone();
        // Conquest costs, even when construction-mode purchases use a cheaper MP price.
        for (victim_kind, cost) in [
            ("rifle", 100.),
            ("tank", 300.),
            ("hq", 200.),
            ("barracks", 150.),
        ] {
            assert_eq!(
                state
                    .kill_bounty(mode, Some(2), victim_kind, Some(0))
                    .unwrap(),
                Some((0, cost * 0.2)),
                "{kind:?} {victim_kind}"
            );
            for killer in [None, Some(2), Some(3)] {
                assert_eq!(
                    state
                        .kill_bounty(mode, Some(2), victim_kind, killer)
                        .unwrap(),
                    None
                );
            }
            assert_eq!(
                state
                    .kill_bounty(mode, Some(1), victim_kind, Some(0))
                    .unwrap(),
                None
            );
        }
        assert_eq!(
            state, before,
            "a prepared bounty must not pay before retirement"
        );
        // An away player may still earn a bounty from a projectile in flight.
        state.players[0].away = true;
        assert_eq!(
            state.kill_bounty(mode, Some(2), "rifle", Some(0)).unwrap(),
            Some((0, 20.))
        );
        state.players[0].out = true;
        assert_eq!(
            state.kill_bounty(mode, Some(2), "rifle", Some(0)).unwrap(),
            None
        );
        assert_eq!(
            state.kill_bounty(mode, None, "rifle", Some(1)).unwrap(),
            Some((1, 20.))
        );
        assert!(state.kill_bounty(mode, Some(99), "rifle", Some(0)).is_err());
        assert!(state.kill_bounty(mode, Some(2), "rifle", Some(99)).is_err());
    }
}

#[test]
fn base05_supply_runs_on_the_forty_tick_beat_only_in_enabled_point_modes() {
    for mode in [
        Mode::Default,
        Mode::Conquest,
        Mode::Classic,
        Mode::Assault,
        Mode::Annihilation,
    ] {
        let due: Vec<_> = (0..122)
            .filter(|tick| point_supply_due(mode, true, *tick).unwrap())
            .collect();
        assert_eq!(due, [1, 41, 81, 121]);
        for tick in 0..122 {
            assert!(!point_supply_due(mode, false, tick).unwrap());
        }
    }
    for mode in [Mode::Horde, Mode::Tutorial, Mode::World] {
        for tick in 0..122 {
            assert!(!point_supply_due(mode, true, tick).unwrap());
        }
    }
    assert!(point_supply_due(Mode::Conquest, true, -1).is_err());
}
