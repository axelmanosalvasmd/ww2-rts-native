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
