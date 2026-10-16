//! Ordinary-command counterfactuals for the canonical Retreat work owner.
use super::*;

fn unit_row(w: &Witness, id: u32) -> Value {
    w.snapshot()["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == id)
        .unwrap()
        .clone()
}

fn cancel(w: &mut Witness, ids: &[u32]) {
    let wallets = w.state.start.economy.players.clone();
    let story = w.state.story.clone();
    let tick = w.state.world.game.tick;
    w.command(0, json!({"t":"retreat","ids":ids,"queue":true}), None);
    assert_eq!(
        w.state.start.economy.players, wallets,
        "Retreat never refunds paid work"
    );
    assert_eq!(
        w.state.story, story,
        "cancellation is not completion or new expenditure"
    );
    assert_eq!(w.state.world.game.tick, tick);
    for &id in ids {
        let u = &w.state.world.game.units[&id];
        if u.owner != 0 {
            continue;
        }
        let c = &w.state.commands.units[&id];
        assert!(u.retreating && !u.path.is_empty());
        assert!(!u.build && !u.dig && !u.entrench);
        assert_eq!(c.build_target, None);
        assert!(!w.state.field_works.is_active(id));
        assert_eq!(w.state.field_works.membership(id), None);
        assert!(c.orders.is_empty() && c.attack_id == 0);
    }
}

#[test]
#[ignore = "requires Root exact slot grant and authentic retreat-build Source receipt"]
fn genuine_retreat_cancels_paid_builders_without_refund_or_site_removal() {
    let mut w = Witness::new("retreat-build");
    let site = w.state.next_unit_id;
    let charge = crate::catalog::Catalog::compiled()
        .unwrap()
        .unit("motorpool")
        .unwrap()
        .get("cost")
        .unwrap()
        .number()
        .unwrap();
    let mp = w.state.start.economy.players[0].mp;
    let spent = w.state.story[0].mp_spent;
    w.command(
        0,
        json!({"t":"build","ids":[1,2],"kind":"motorpool","x":29,"z":9}),
        None,
    );
    assert_eq!(w.state.start.economy.players[0].mp, mp - charge);
    assert_eq!(w.state.story[0].mp_spent, spent + charge);
    for id in [1, 2] {
        assert!(w.state.world.game.units[&id].build);
        assert_eq!(w.state.commands.units[&id].build_target, Some(site));
    }
    w.until(2400, "genuine-partial-site", |m| {
        m.world.game.units[&site].built.is_some_and(|v| v > 0.0)
    });
    assert!(w.state.world.game.units[&site].built.unwrap() < 1.0);
    w.unchanged_command(0, json!({"t":"retreat","ids":[4]}), Denial::Needs);
    let other = unit_row(&w, 2);
    let site_before = unit_row(&w, site);
    cancel(&mut w, &[1, 1, 4]);
    assert_eq!(
        unit_row(&w, 2),
        other,
        "unselected worker continues its real job"
    );
    assert_eq!(
        unit_row(&w, site),
        site_before,
        "paid site survives cancellation unchanged"
    );
    let progress = w.state.world.game.units[&site].built.unwrap();
    w.until(2400, "uncancelled-worker-progress", |m| {
        m.world.game.units[&site].built.unwrap() > progress
    });
    assert!(w.state.world.game.units[&site].built.unwrap() < 1.0);
    let other_progress = w.state.world.game.units[&site].built.unwrap();
    cancel(&mut w, &[2]);
    for _ in 0..20 {
        let report = w.step();
        assert!(report.retired.is_empty());
        assert_eq!(
            w.state.world.game.units[&site].built,
            Some(other_progress),
            "no cancelled builder remains in next tick's crew"
        );
    }
    w.complete_checks(
        json!({"site":site,"paidCharge":number(charge),"unselectedProgress":true,
        "siteRetained":true,"noRefundOrStoryEvent":true,"noWorkerProgressTicks":20}),
    );
}

#[test]
#[ignore = "requires Root exact slot grant and authentic retreat-project Source receipt"]
fn genuine_retreat_releases_paid_project_work_without_refund_or_cached_membership() {
    let mut w = Witness::new("retreat-project");
    w.command(
        0,
        json!({"t":"entrench","ids":[1,2],"pattern":"line","fort":"trench",
        "x":29,"z":9,"x2":41,"z2":9}),
        None,
    );
    let project = w.state.field_works.membership(1).unwrap();
    assert_eq!(w.state.field_works.membership(2), Some(project));
    w.until(2400, "both-real-paid-segments", |m| {
        [1, 2].iter().all(|id| {
            m.field_works
                .jobs()
                .get(id)
                .is_some_and(|job| job.project == Some(project))
        })
    });
    assert_eq!(w.state.field_works.project(project).unwrap().active, 2);
    w.unchanged_command(0, json!({"t":"retreat","ids":[4]}), Denial::Needs);
    let other = w.state.field_works.jobs()[&2].clone();
    cancel(&mut w, &[1, 1, 4]);
    assert_eq!(
        w.state.field_works.jobs()[&2],
        other,
        "other paid segment remains owned"
    );
    assert_eq!(w.state.field_works.membership(2), Some(project));
    let cached = w.state.field_works.project(project).unwrap();
    assert_eq!(
        (cached.active, cached.crew),
        (2, 2),
        "valid cached counts must not be eagerly rewritten"
    );
    w.state
        .field_works
        .validate_retreat_reservation(&w.state.world, 2)
        .unwrap();
    // Source recounts the cached active number on the next ordinary early beat.
    w.step();
    assert_eq!(w.state.field_works.project(project).unwrap().active, 1);
    cancel(&mut w, &[2]);
    let ticks = 20 - w.state.world.game.tick.rem_euclid(20);
    for _ in 0..ticks {
        w.step();
    }
    assert!(
        w.state.field_works.project(project).is_none(),
        "real no-crew sweep retires the listed project"
    );
    assert!(w.state.field_works.jobs().is_empty());
    for id in [1, 2] {
        assert_eq!(w.state.field_works.membership(id), None);
    }
    w.complete_checks(json!({"project":project,"paidActors":[1,2],"unselectedPaidJobPreserved":true,
        "cachedActiveAfterCancel":2,"cachedCrewAfterCancel":2,"nextTickActive":1,"sweepTicks":ticks,"projectSwept":true,"noRefundOrStoryEvent":true}));
}

// These fingerprints read real canonical state. They never restore actors or
// supply expected Source state to Match. Fault cleanup restores metadata only.
fn guard_state(w: &Witness) -> Value {
    let m = &w.state;
    json!({
        "world":format!("{:?}",m.world),"grid":format!("{:?}",m.grid),
        "work":format!("{:?}",m.field_works),"controls":format!("{:?}",m.commands.units),
        "queue":format!("{:?}",m.commands.queue),
        "paths":format!("{:?}",m.commands.paths),"requests":format!("{:?}",m.commands.requests),
        "recipients":format!("{:?}",m.recipients),"fog":format!("{:?}",m.routing_fog),
        "wallets":format!("{:?}",m.start.economy.players),"story":format!("{:?}",m.story),
        "fallen":format!("{:?}",m.fallen),"production":format!("{:?}",m.production),
        "combat":format!("{:?}",m.combat),"firing":format!("{:?}",m.firing),
        "abilities":format!("{:?}",m.abilities),"recovery":format!("{:?}",m.recovery),
        "nextId":m.next_unit_id,"inputReceipt":w.replay.receipt()
    })
}

fn selected_error(w: &mut Witness, ids: &[u32], label: &str, expected: &str) {
    // Selections include the genuine own MG, another real paid worker, reversed
    // order and repeated/foreign entries. Failed preflight must publish nothing.
    let raw = json!({"t":"retreat","ids":ids,"queue":true});
    let command = Command::from_json(&raw).unwrap();
    let before = guard_state(w);
    let result = w
        .state
        .execute_command(0, &command, w.state.finish.is_some());
    let after = guard_state(w);
    let atomic = before == after;
    let mut evidence = json!({"stage":"reservationGuard","label":label,"command":raw,
        "result":format!("{result:?}"),"expectedProviderError":expected,"atomic":atomic,"nativeTypedFaultOnly":true});
    if !atomic {
        evidence["before"] = before.clone();
        evidence["after"] = after.clone();
    }
    w.record(evidence);
    assert_eq!(
        result.as_ref().err().map(String::as_str),
        Some(expected),
        "the named canonical preflight must reject: {label}"
    );
    assert_eq!(
        before, after,
        "failed selection must publish nothing: {label}"
    );
}

#[test]
#[ignore = "requires Root exact slot grant and authentic retreat-reservations Source receipt"]
fn genuine_retreat_reservation_preflight_is_atomic_and_filters_foreign_corruption() {
    use crate::field_works::reservation_faults::{ALL, Fault, corrupt};
    let mut w = Witness::new("retreat-reservations");
    w.move_to(0, 3, Point { x: 43.0, z: 29.0 });
    for owner in [0, 1] {
        let (ids, x, z) = if owner == 0 {
            (vec![1, 2], 29, 9)
        } else {
            (vec![4, 5], 93, 53)
        };
        w.command(
            owner,
            json!({"t":"entrench","ids":ids,"pattern":"line","fort":"trench",
            "x":x,"z":z,"x2":x+12,"z2":z}),
            None,
        );
        w.until(2400, "guard-case-real-paid-segments", |m| {
            ids.iter().all(|id| {
                m.field_works
                    .jobs()
                    .get(id)
                    .is_some_and(|job| job.project.is_some())
            })
        });
    }
    for &fault in ALL {
        let original = w.state.field_works.clone();
        w.state
            .field_works
            .validate_retreat_reservation(&w.state.world, 2)
            .unwrap();
        corrupt(&mut w.state.field_works, 2, fault);
        assert_eq!(
            w.state
                .field_works
                .validate_retreat_reservation(&w.state.world, 2)
                .unwrap_err(),
            fault.validator_error()
        );
        for ids in [&[3, 2][..], &[1, 2][..], &[2, 1][..], &[1, 1, 4, 3, 2][..]] {
            // Moving the shared record leaves worker 1's membership dangling.
            // The first selected worker determines which canonical check fails.
            let expected = match (fault, ids[0]) {
                (Fault::ZeroProjectIdentity, 1) => {
                    "ground retreat reservation: invalid field-work membership identity".into()
                }
                _ => fault.dispatch_error(),
            };
            selected_error(&mut w, ids, &format!("{fault:?}"), &expected);
        }
        w.state.field_works = original;
    }
    let hq = w
        .state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && w.state.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .id;
    let original = w.state.commands.units[&2].build_target;
    w.state.commands.units.get_mut(&2).unwrap().build_target = Some(hq);
    for ids in [&[3, 2][..], &[1, 2][..], &[2, 1][..], &[1, 1, 4, 3, 2][..]] {
        selected_error(
            &mut w,
            ids,
            "selected malformed build projection",
            "unrepresented or inconsistent construction activity",
        );
    }
    w.state.commands.units.get_mut(&2).unwrap().build_target = original;

    for control in 0..2 {
        let original_work = w.state.field_works.clone();
        let original_build = w.state.commands.units[&4].build_target;
        if control == 0 {
            corrupt(&mut w.state.field_works, 4, Fault::PaidProjectMismatch);
            assert!(
                w.state
                    .field_works
                    .validate_retreat_reservation(&w.state.world, 4)
                    .is_err()
            );
        } else {
            w.state.commands.units.get_mut(&4).unwrap().build_target = Some(hq);
            assert!(
                crate::match_construction::validate_build_activity(
                    &w.state.world.game.units[&4],
                    &w.state.commands.units[&4]
                )
                .is_err()
            );
        }
        let before = guard_state(&w);
        w.unchanged_command(0, json!({"t":"retreat","ids":[4]}), Denial::Needs);
        assert_eq!(
            before,
            guard_state(&w),
            "foreign-only malformed metadata is not consumed"
        );
        let foreign = unit_row(&w, 4);
        let job = w.state.field_works.jobs()[&4].clone();
        w.command(0, json!({"t":"retreat","ids":[3,4],"queue":true}), None);
        assert!(
            w.state.world.game.units[&3].retreating
                && !w.state.world.game.units[&3].path.is_empty()
        );
        assert_eq!(
            w.state.rules.kinds[w.state.world.game.units[&3].kind].name,
            "mg"
        );
        assert_eq!(foreign, unit_row(&w, 4));
        assert_eq!(job, w.state.field_works.jobs()[&4]);
        w.record(
            json!({"stage":"reservationGuard","label":"foreign metadata filtered",
            "control":control,"needs":true,"mixedAccepted":3,"foreignPreserved":true}),
        );
        w.state.field_works = original_work;
        w.state.commands.units.get_mut(&4).unwrap().build_target = original_build;
    }
    w.record(json!({"stage":"reservationGuardSummary","selectedErrors":(ALL.len()+1)*4,"selectionOrders":[[3,2],[1,2],[2,1],[1,1,4,3,2]],
        "foreignOnlyNeeds":2,"mixedAccepted":2,"nativeTypedFaultsComparedToSource":false}));
    // Only the ordinary valid-input branch has a Source counterpart. The
    // typed native errors are retained above, not asserted as Source equality.
    w.complete_checks(
        json!({"foreignOnlyControls":2,"mixedAcceptedControls":2,"selectedActor":3,
        "foreignActor":4,"realPaidActors":[1,2,4,5],"nativeTypedFaultsComparedToSource":false}),
    );
}
