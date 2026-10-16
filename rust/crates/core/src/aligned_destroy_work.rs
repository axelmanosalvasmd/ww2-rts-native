//! Destroy dispatch retains work until the actual ordinary retirement owner.
use super::*;

#[test]
#[ignore = "requires Root exact slot grant and authentic scuttle-work Source receipt"]
fn genuine_scuttle_defers_paid_work_and_site_teardown_until_retirement() {
    let mut w = Witness::new("scuttle-work");
    let site = w.state.next_unit_id;
    let charge = crate::catalog::Catalog::compiled()
        .unwrap()
        .unit("motorpool")
        .unwrap()
        .get("cost")
        .unwrap()
        .number()
        .unwrap();
    let initial_mp = w.state.start.economy.players[0].mp;
    let initial_spent = w.state.story[0].mp_spent;
    w.command(
        0,
        json!({"t":"build","ids":[1,2],"kind":"motorpool","x":29,"z":9}),
        None,
    );
    assert_eq!(
        w.state.start.economy.players[0].mp.to_bits(),
        (initial_mp - charge).to_bits()
    );
    assert_eq!(
        w.state.story[0].mp_spent.to_bits(),
        (initial_spent + charge).to_bits()
    );
    assert_eq!(
        w.state.rules.kinds[w.state.world.game.units[&site].kind].name,
        "motorpool"
    );
    assert_eq!(w.state.world.game.units[&site].built, Some(0.0));
    w.until(2400, "real-partial-scuttle-site", |m| {
        m.world.game.units[&site].built.is_some_and(|v| v > 0.0)
    });
    assert!(w.state.world.game.units[&site].built.unwrap() < 1.0);
    let before_project_spent = w.state.story[0].mp_spent;
    w.command(
        0,
        json!({"t":"entrench","ids":[2],"pattern":"line","fort":"trench",
        "x":41,"z":9,"x2":53,"z2":9}),
        None,
    );
    let project = w.state.field_works.membership(2).unwrap();
    w.until(2400, "real-paid-scuttle-segment", |m| {
        m.field_works
            .jobs()
            .get(&2)
            .is_some_and(|job| job.project == Some(project))
    });
    let project_charge = w.state.story[0].mp_spent - before_project_spent;
    assert!(
        project_charge.is_finite() && project_charge > 0.0,
        "the genuine paid project must record real expenditure"
    );
    assert!(w.state.world.game.units[&site].built.unwrap() < 1.0);
    assert!(w.state.world.game.units[&1].build);
    assert_eq!(w.state.commands.units[&1].build_target, Some(site));
    assert!(w.state.production.building(site).is_some());
    assert!(
        w.state.building_rallies.contains_key(&site),
        "producer removal must have a real preexisting owner"
    );
    for id in [1, 2, site] {
        assert!(
            w.state.abilities.get(id).is_some() && w.state.recovery.get(id).is_some(),
            "retirement owner-removal assertions need genuine live sidecars"
        );
    }
    let producer = w.state.production.clone();
    let rallies = w.state.building_rallies.clone();
    let ids = w.state.world.game.units.keys().copied().collect::<Vec<_>>();
    let site_at = w.state.world.game.units[&site].at();
    let controls = w.state.commands.units.clone();
    let work = w.state.field_works.clone();
    let wallets = w.state.start.economy.players.clone();
    let story = w.state.story.clone();
    let foreign = w.snapshot()["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["id"] == 4)
        .unwrap()
        .clone();
    w.command(0, json!({"t":"destroy","ids":[1,1,2,4],"id":site}), None);
    assert_eq!(
        w.state.commands.units, controls,
        "Destroy dispatch must not eagerly tear down work controls"
    );
    assert_eq!(w.state.field_works, work);
    assert_eq!(
        w.state.production, producer,
        "dispatch retains the real producer until retirement"
    );
    assert_eq!(w.state.building_rallies, rallies);
    assert_eq!(w.state.start.economy.players, wallets);
    assert_eq!(w.state.story, story);
    for id in [1, 2, site] {
        assert_eq!(w.state.world.game.units[&id].hp, 0.0);
        assert_eq!(w.state.combat.get(id).unwrap().last_hit, Some(0));
    }
    assert_eq!(
        w.snapshot()["units"]
            .as_array()
            .unwrap()
            .iter()
            .find(|u| u["id"] == 4)
            .unwrap(),
        &foreign
    );
    w.unchanged_command(
        0,
        json!({"t":"destroy","ids":[1,2],"id":site}),
        Denial::Unseen,
    );
    let report = w.step();
    assert_eq!(
        report.retired.iter().map(|r| r.unit.id).collect::<Vec<_>>(),
        vec![1, 2, site]
    );
    assert!(report.world_removed.is_empty());
    assert_eq!(
        w.state.world.game.units.keys().copied().collect::<Vec<_>>(),
        ids.into_iter()
            .filter(|id| ![1, 2, site].contains(id))
            .collect::<Vec<_>>()
    );
    for loss in &report.retired {
        assert_eq!(loss.last_hit, Some(0));
        assert_eq!(loss.enemy_killer, None);
        assert!(!loss.unit.build && !loss.unit.dig && !loss.unit.entrench);
        assert!(!w.state.field_works.is_active(loss.unit.id));
        assert_eq!(w.state.field_works.membership(loss.unit.id), None);
        // Ordinary dead controls may remain for cached/projectile consumers.
        // The canonical cancellation must still clear their construction scalar.
        if let Some(control) = w.state.commands.units.get(&loss.unit.id) {
            assert_eq!(control.build_target, None);
        }
        assert!(w.state.abilities.get(loss.unit.id).is_none());
        assert!(w.state.recovery.get(loss.unit.id).is_none());
    }
    assert!(w.state.production.building(site).is_none());
    assert!(!w.state.building_rallies.contains_key(&site));
    assert_eq!(w.state.fallen.structure, Some(site_at));
    assert_eq!(w.state.fallen.hq, Some(site_at));
    for (owner, prior) in story.iter().enumerate() {
        let now = w.state.story[owner];
        for key in crate::match_story::StoryKey::ALL {
            let delta = if owner == 0 && key == crate::match_story::StoryKey::Losses {
                3.0
            } else {
                0.0
            };
            assert_eq!(
                now.value(key).to_bits(),
                (prior.value(key) + delta).to_bits(),
                "only the three actual friendly losses change story field {}",
                key.as_str()
            );
        }
        let p = &w.state.start.economy.players[owner];
        assert_eq!(
            p.mp,
            wallets[owner].mp + p.inc * w.state.rules.movement.tick
        );
    }
    let ticks = 20 - w.state.world.game.tick.rem_euclid(20);
    for _ in 0..ticks {
        w.step();
    }
    assert!(w.state.field_works.project(project).is_none());
    w.complete_checks(json!({"site":site,"paidProject":project,"deferredTeardown":true,
        "paidSiteCharge":number(charge),"paidProjectCharge":number(project_charge),"storyFieldsChecked":7,
        "retiredActors":[1,2,site],"ownLossDelta":3,"enemyKillDelta":0,"siteProducerRemoved":true,
        "projectSwept":true,"sweepTicks":ticks,"noRefundOrBounty":true}));
}
