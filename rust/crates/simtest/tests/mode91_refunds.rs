use crate::shared::world_scene;
use serde_json::{Value, json};
use ww2_core::{match_commands::Command, match_end::EndReason, match_runtime::Match};

fn scene() -> Match {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/world-phases.json")).unwrap();
    world_scene::make_match(&fixture, &[json!(0), json!(1)])
}

#[test]
fn mode91_waiting_refund_reverses_original_mp_charge_and_preserves_active_progress() {
    let mut m = scene();
    let producer = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .id;
    m.start.economy.players[0].mp = 5000.0;
    m.start.economy.players[0].fuel = 500.0;
    let wallet = m.start.economy.players[0].mp;
    let buy = Command::Buy {
        kind: "engineer".into(),
        from: Some(producer),
    };
    for _ in 0..3 {
        assert_eq!(m.execute_command(0, &buy, false).unwrap(), None);
    }
    let jobs = m.production.building(producer).unwrap().jobs().to_vec();
    let charge = jobs.iter().map(|j| j.charge.mp).sum::<f64>();
    assert_eq!(m.story[0].mp_spent, charge);
    assert_eq!(m.start.economy.players[0].mp, wallet - charge);
    m.step_training(producer, 0.5).unwrap();
    let progress = m.production.building(producer).unwrap().progress();
    assert!(progress > 0.0);
    let story = m.story.clone();
    assert!(m.cancel_training(0, producer, jobs[0].id).is_err());
    assert!(m.cancel_training(1, producer, jobs[1].id).is_err());
    assert_eq!(m.story, story);
    let fuel = m.start.economy.players[0].fuel;
    let refund = m.cancel_training(0, producer, jobs[1].id).unwrap();
    assert_eq!(refund, jobs[1].charge);
    assert_eq!(m.start.economy.players[0].mp, wallet - charge + refund.mp);
    assert_eq!(m.start.economy.players[0].fuel, fuel + refund.fuel);
    assert_eq!(m.story[0].mp_spent, charge - refund.mp);
    let mut expected_story = story;
    expected_story[0].mp_spent -= refund.mp;
    assert_eq!(m.story, expected_story);
    let queue = m.production.building(producer).unwrap();
    assert_eq!(queue.jobs(), [jobs[0].clone(), jobs[2].clone()]);
    assert_eq!(queue.progress(), progress);
    assert_eq!(m.story[0].built, 0.0);
    assert_eq!(m.story[0].losses, 0.0);
    assert!(m.cancel_training(0, producer, jobs[1].id).is_err());
    let before = format!("{:?}", (&m.production, &m.story, &m.start.economy));
    m.initialize_tick_scene_finish(0, EndReason::Nations, None)
        .unwrap();
    assert!(m.cancel_training(0, producer, jobs[2].id).is_err());
    assert_eq!(
        format!("{:?}", (&m.production, &m.story, &m.start.economy)),
        before
    );
}

#[test]
fn mode91_training_refund_refusals_preserve_queue_wallet_and_all_counters() {
    let mut m = scene();
    let producer = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .id;
    m.start.economy.players[0].mp = 5000.0;
    for _ in 0..2 {
        assert_eq!(
            m.execute_command(
                0,
                &Command::Buy {
                    kind: "engineer".into(),
                    from: Some(producer),
                },
                false,
            )
            .unwrap(),
            None
        );
    }
    let waiting = m.production.building(producer).unwrap().jobs()[1].id;
    for gate in ["away", "out", "decided", "story", "wallet"] {
        let story = m.story.clone();
        let wallet = m.start.economy.clone();
        match gate {
            "away" => m.start.economy.players[0].away = true,
            "out" => m.start.economy.players[0].out = true,
            "decided" => {
                m.initialize_tick_scene_finish(0, EndReason::Nations, None)
                    .unwrap();
            }
            "story" => m.story[0].mp_spent = f64::INFINITY,
            "wallet" => m.start.economy.players[0].mp = f64::INFINITY,
            _ => unreachable!(),
        }
        let before = format!("{:?}", (&m.production, &m.start.economy, &m.story));
        assert!(m.cancel_training(0, producer, waiting).is_err(), "{gate}");
        assert_eq!(
            format!("{:?}", (&m.production, &m.start.economy, &m.story)),
            before,
            "{gate}"
        );
        m.story = story;
        m.start.economy = wallet;
        m.finish = None;
    }
}

#[test]
fn mode91_training_refund_uses_the_stored_charge_and_counts_only_its_manpower() {
    let mut m = scene();
    let producer = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .id;
    // Seed an already-paid queue with an original fractional charge. Cancellation
    // must preserve its MP/fuel amounts rather than pricing the unit again.
    m.production
        .record_paid(
            producer,
            "engineer",
            ww2_core::economy::Price { mp: 0.0, fuel: 0.0 },
        )
        .unwrap();
    let original = ww2_core::economy::Price {
        mp: 123.5,
        fuel: 17.25,
    };
    let waiting = m
        .production
        .record_paid(producer, "engineer", original)
        .unwrap();
    m.start.economy.players[0].mp = 500.0 - original.mp;
    m.start.economy.players[0].fuel = 100.0 - original.fuel;
    m.record_spending(0, original.mp).unwrap();
    let refunded = m.cancel_training(0, producer, waiting).unwrap();
    assert_eq!(refunded, original);
    assert_eq!(m.start.economy.players[0].mp, 500.0);
    assert_eq!(m.start.economy.players[0].fuel, 100.0);
    assert!(m.story.iter().all(|s| *s == Default::default()));
}
