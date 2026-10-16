//! CMB-148: command delegation to the canonical off-map support owner.
use super::*;
use crate::{game::StrikeKind, match_support::SupportEvent};

#[test]
fn cmb148_support_commands_delegate_all_catalog_families_to_existing_owner() {
    let catalog = Catalog::compiled().unwrap();
    let families = catalog.table("SUPPORT").unwrap().object().unwrap();
    assert_eq!(families.len(), 8);
    let admission_state = |m: &Match| {
        (
            m.start.economy.players.clone(),
            m.start
                .players
                .iter()
                .map(|player| player.support.clone())
                .collect::<Vec<_>>(),
            m.world.game.strikes.clone(),
            m.support.clone(),
            m.story.clone(),
            ids(m),
            m.next_unit_id,
        )
    };
    for (name, def) in families {
        let mut m = scene(&[("rifle", 0, 40., 40.), ("rifle", 1, 150., 150.)]);
        m.start.economy.players[0].mp = 1000.;
        m.start.economy.players[0].mun = 73.;
        m.start.economy.players[0].fuel = 41.;
        let number = |key: &str| def.get(key).unwrap().number().unwrap();
        let hidden = p(150., 150.);
        assert!(!vision::team_sees(
            &m.world.game,
            &m.rules,
            &m.grid,
            0,
            hidden
        ));
        assert!(m.world.game.strikes.is_empty());
        assert_eq!(m.start.players[0].support[name], 0.);

        // Only a unit drop requires sight. Other families may target hidden ground.
        let at = if name == "para" {
            let before = admission_state(&m);
            assert_eq!(
                command(
                    &mut m,
                    json!({"t":"support","kind":name,"x":hidden.x,"z":hidden.z})
                ),
                Some(Denial::Unseen)
            );
            assert_eq!(admission_state(&m), before, "{name}: unseen refusal");
            p(45., 40.)
        } else {
            hidden
        };
        if name == "para" {
            assert!(vision::team_sees(&m.world.game, &m.rules, &m.grid, 0, at));
        }
        let mut order = json!({"t":"support","kind":name,"x":at.x,"z":at.z});
        let direction = if name == "para" {
            let spawn = m.start.players[0].spawn;
            crate::math::atan2(at.z - spawn.z, at.x - spawn.x)
        } else {
            order["dir"] = json!(0.5);
            0.5
        };
        let mut paid_wallets = m.start.economy.players.clone();
        paid_wallets[0].mp -= number("cost");
        let mut called_story = m.story.clone();
        called_story[0].support_calls += 1.;
        called_story[0].mp_spent += number("cost");
        let mut cooldowns = m
            .start
            .players
            .iter()
            .map(|player| player.support.clone())
            .collect::<Vec<_>>();
        cooldowns[0].insert(name.clone(), number("cd"));

        assert_eq!(command(&mut m, order.clone()), None, "{name}");
        assert_eq!(m.start.economy.players, paid_wallets, "{name}: wallet");
        assert_eq!(m.story, called_story, "{name}: support tally");
        assert_eq!(
            m.start
                .players
                .iter()
                .map(|player| player.support.clone())
                .collect::<Vec<_>>(),
            cooldowns,
            "{name}: cooldown ownership"
        );
        assert_eq!(m.world.game.strikes.len(), 1, "{name}");
        let strike = &m.world.game.strikes[0];
        assert_eq!(strike.owner, 0, "{name}");
        assert_eq!(p(strike.x, strike.z), at, "{name}");
        assert_eq!(strike.dir, direction, "{name}");
        assert!(!strike.live, "{name}: wait for arrival");
        assert_eq!(
            strike.kind,
            if name == "recon" {
                StrikeKind::Recon
            } else {
                StrikeKind::Other
            }
        );
        let timing = strike.runtime.as_ref().unwrap();
        assert_eq!(timing.kind.name(), name, "{name}: support family");
        assert_eq!(timing.delay, number("delay"), "{name}");
        assert_eq!(
            timing.left,
            def.get("shells")
                .or_else(|| def.get("dur"))
                .and_then(|value| value.number())
                .unwrap_or(0.),
            "{name}: payload"
        );
        assert_eq!(timing.next, 0., "{name}");
        assert!(m.support.events.is_empty());

        let before = admission_state(&m);
        assert_eq!(command(&mut m, order), Some(Denial::Cooldown), "{name}");
        assert_eq!(admission_state(&m), before, "{name}: no second debit");

        // Advance the admitted strike, without injecting a payload or damage result.
        if name == "strafe" {
            let actors = ids(&m);
            let hp = m.world.game.units[&2].hp;
            let suppression = m.world.game.units[&2].supp;
            m.step_support_strikes(&actors, 0.05).unwrap();
            m.advance_support_timers(0.05).unwrap();
            assert_eq!(m.world.game.units[&2].hp, hp);
            assert!(m.support.events.is_empty());
            for _ in 0..120 {
                m.step_support_strikes(&actors, 0.05).unwrap();
                m.advance_support_timers(0.05).unwrap();
            }
            assert!(m.world.game.units[&2].hp < hp);
            assert!(m.world.game.units[&2].supp > suppression);
            assert!(m.world.game.strikes.is_empty());
            assert_eq!(
                m.support
                    .events
                    .iter()
                    .filter(|event| matches!(event, SupportEvent::Plane { kind, .. } if kind.name() == "strafe"))
                    .count(),
                1
            );
            assert!(m.start.players[0].support[name] > 0.);
            assert!(m.start.players[0].support[name] < number("cd"));
            assert_eq!(m.start.economy.players, paid_wallets);
            assert_eq!(m.story[0].support_calls, called_story[0].support_calls);
        }
    }
}
