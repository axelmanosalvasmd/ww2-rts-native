//! AI-48: delivered field work enters the canonical queue and survives ordinary ticks.
use super::*;
use super::{runtime_match, screen};
use crate::{
    match_commands::Command, match_human_observation::HumanObservationMemory, orders::OrderKey,
    recipient_snapshot::SnapshotCache,
};

#[test]
fn intent_ai48_human_policy_fill_admission_completion_and_every_seat_snapshot() {
    let mut m = runtime_match();
    let point = m.start.points[0].at;
    let worker = *m
        .world
        .game
        .units
        .iter()
        .find(|(_, u)| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .0;
    // Place an existing squad in the capture square; ownership comes from ordinary ticks.
    let u = m.world.game.units.get_mut(&worker).unwrap();
    u.x = point.x;
    u.z = point.z;
    m.grid.rebuild(&m.world.game.units);
    for _ in 0..180 {
        m.advance_tick().unwrap();
    }
    assert_eq!(m.start.points[0].owner, Some(0));
    let cell = m.world.game.cell_of(point.x + 2.0, point.z).unwrap();
    assert_eq!(m.world.initial().chars[cell], b'.');
    m.world.set_cell(cell, b'F').unwrap();
    assert!(m.world.spoiled(cell));
    let fill_cost = crate::catalog::Catalog::compiled()
        .unwrap()
        .table("FORTS")
        .unwrap()
        .get("fill")
        .unwrap()
        .get("cost")
        .unwrap()
        .number()
        .unwrap();
    // Enough for repair and its reserve, below the next facility's 200 MP cost.
    m.start.economy.players[0].mp = fill_cost + 150.0;
    let mut memory = HumanObservationMemory::new(0);
    let view = m.human_commander_observation(&mut memory).unwrap();
    let field = view
        .packet
        .input()
        .runtime
        .as_ref()
        .unwrap()
        .field
        .as_ref()
        .unwrap();
    assert_eq!(field.initial_chars[cell], b'.');
    assert_eq!(view.packet.input().chars[cell], b'F');
    let mut policy = HumanPolicy::new(0, 771);
    let commands = policy.plan(screen(&view), Level::Normal).unwrap();
    let raw = commands
        .iter()
        .find(|c| c["t"] == "dig" && c["kind"] == "fill")
        .expect("visible battle flooding at a captured point schedules Fill");
    assert_eq!(raw["ids"], json!([worker]));
    let tick = m.world.game.tick as u64 + 1;
    m.enqueue_command(
        OrderKey {
            tick,
            issuer: 0,
            sequence: 0,
        },
        Command::from_json(raw).unwrap(),
    )
    .unwrap();
    let report = m.advance_tick().unwrap();
    assert_eq!(report.commands.len(), 1);
    assert_eq!(report.commands[0].denial, None, "{raw:?}");
    assert!(m.field_works.is_active(worker));
    assert!(m.world.game.units[&worker].dig);
    let mut saw_delivered_job = false;
    let mut saw_restored_ground = false;
    for _ in 0..200 {
        m.advance_tick().unwrap();
        let cache = SnapshotCache::new(&m).unwrap();
        for seat in 0..m.start.players.len() {
            cache.recipient(seat).unwrap();
        }
        if m.world.game.tick % 2 == 0 {
            let delivered = m.human_commander_observation(&mut memory).unwrap();
            let own = delivered
                .packet
                .input()
                .units
                .iter()
                .find(|u| u.id == worker)
                .unwrap();
            saw_delivered_job |= own.dig;
            let facts = delivered
                .packet
                .input()
                .runtime
                .as_ref()
                .unwrap()
                .field
                .as_ref()
                .unwrap();
            assert_eq!(facts.initial_chars[cell], b'.');
            saw_restored_ground |= delivered.packet.input().chars[cell] == b'.';
        }
    }
    assert!(saw_delivered_job && saw_restored_ground);
    assert_eq!(m.world.game.chars[cell], b'.');
    assert!(!m.field_works.is_active(worker));
    assert!(!m.world.game.units[&worker].dig);
    assert_eq!(m.tick_state.fault(), None);
}

#[test]
fn intent_ai48_authored_hidden_mines_do_not_enter_delivered_field_facts_or_policy() {
    use crate::{map::AuthoritativeMap, match_runtime::Match};
    let mut plain = runtime_match();
    let mut rows = vec![".".repeat(64); 64];
    rows[60].replace_range(10..11, "N");
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"AI runtime intent","w":64,"h":64,"rows":rows,
            "spawns":[{"x":4,"y":4},{"x":59,"y":59}],
            "points":[{"x":32,"y":32}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut mined = Match::new(&map, plain.options.clone()).unwrap();
    let cell = 60 * 64 + 10;
    assert_eq!(plain.world.initial().chars[cell], b'.');
    assert_eq!(mined.world.initial().chars[cell], b'N');
    let plain_view = plain
        .human_commander_observation(&mut HumanObservationMemory::new(0))
        .unwrap();
    let mined_view = mined
        .human_commander_observation(&mut HumanObservationMemory::new(0))
        .unwrap();
    assert_eq!(plain_view.packet, mined_view.packet);
    assert_eq!(
        plain_view.packet.input().runtime.as_ref().unwrap().field,
        mined_view.packet.input().runtime.as_ref().unwrap().field,
    );
    assert_eq!(
        HumanPolicy::new(0, 771)
            .plan(screen(&plain_view), Level::Normal)
            .unwrap(),
        HumanPolicy::new(0, 771)
            .plan(screen(&mined_view), Level::Normal)
            .unwrap(),
    );
}
