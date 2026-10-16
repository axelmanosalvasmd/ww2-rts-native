use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    recipient_snapshot::SnapshotCache,
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};
use serde_json::json;

fn scene() -> (Match, u32) {
    scene_with("engineer", false)
}

fn scene_with(kind: &str, tech: bool) -> (Match, u32) {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Queued Dig", "w":60,"h":60,"rows":vec![".".repeat(60);60],
            "heights":vec!["0".repeat(60);60],
            "spawns":[{"x":4,"y":4},{"x":55,"y":55}],"points":[{"x":30,"y":30}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut state = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Default,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("p{slot}"),
                    team: slot,
                    faction: Faction::try_from(slot as u8).unwrap(),
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    state.start.economy.players[0].mp = 500.;
    let id = state.next_unit_id;
    let staged = StagedUnit::ordinary(
        &state.world.game,
        &state.rules,
        0,
        state.rules.kind(kind).unwrap(),
        id,
        state.world.game.units.len() as u32,
        state.start.players[0].spawn,
        Some(Point { x: 28., z: 24. }),
    )
    .unwrap();
    prepare_publication(
        PublicationParts {
            units: &mut state.world.game.units,
            grid: &mut state.grid,
            controls: &mut state.commands.units,
            recruitment: &mut state.recruit_metadata,
            next_id: &mut state.next_unit_id,
        },
        &state.rules,
        2,
        vec![staged],
    )
    .unwrap()
    .publish_with(|unit, _| {
        state.combat.initialize(unit.id);
        state.firing.initialize(unit.id);
        state.abilities.initialize(unit.id);
        state.recovery.initialize(unit.id);
    });
    (state, id)
}

fn dig(id: u32) -> Command {
    Command::Dig {
        ids: vec![i64::from(id)],
        kind: FortKind::Trench,
        at: Point { x: 30., z: 24. },
        direction: Some(0.),
        queue: true,
    }
}

#[test]
fn queued_dig_defers_payment_then_survives_ordinary_ticks_and_recipient_snapshot() {
    let (mut state, id) = scene();
    let before = state.start.economy.players[0].mp;
    assert_eq!(state.execute_command(0, &dig(id), false).unwrap(), None);
    assert_eq!(state.start.economy.players[0].mp, before);
    assert!(!state.world.game.units[&id].dig);
    assert_eq!(state.commands.units[&id].orders.len(), 1);
    state.world.game.units.get_mut(&id).unwrap().path = vec![Point { x: 32., z: 24. }];
    SnapshotCache::new(&state).unwrap().recipient(0).unwrap();
    for _ in 0..3 {
        state.advance_tick().unwrap();
        SnapshotCache::new(&state).unwrap().recipient(0).unwrap();
        assert_eq!(state.commands.units[&id].orders.len(), 1);
        assert!(!state.world.game.units[&id].dig);
    }
    state.world.game.units.get_mut(&id).unwrap().path.clear();
    let before_activation = state.start.economy.players[0].mp;
    state.start_queued_orders(id, false).unwrap();
    assert_eq!(state.start.economy.players[0].mp, before_activation - 30.);
    assert_eq!(state.story[0].mp_spent, 30.);
    for _ in 0..3 {
        state.advance_tick().unwrap();
        SnapshotCache::new(&state).unwrap().recipient(0).unwrap();
    }
    assert!(state.world.game.units[&id].dig);
    assert!(state.field_works.is_active(id));
    assert_eq!(state.story[0].mp_spent, 30.);
    assert!(state.commands.units[&id].orders.is_empty());
}

#[test]
fn unaffordable_queued_dig_is_skipped_when_its_turn_arrives() {
    let (mut state, id) = scene();
    assert_eq!(state.execute_command(0, &dig(id), false).unwrap(), None);
    state.start.economy.players[0].mp = 0.;
    state.start_queued_orders(id, false).unwrap();
    assert_eq!(state.start.economy.players[0].mp, 0.);
    for _ in 0..3 {
        state.advance_tick().unwrap();
        SnapshotCache::new(&state).unwrap().recipient(0).unwrap();
    }
    assert!(!state.world.game.units[&id].dig);
    assert!(!state.field_works.is_active(id));
    assert!(state.commands.units[&id].orders.is_empty());
    assert_eq!(state.story[0].mp_spent, 0.);
}

#[test]
fn dig_prerequisites_use_first_selection_and_queue_skips_only_current_payment() {
    let (mut state, id) = scene();
    let bad = i64::from(id) + 10_000;
    let at = Point { x: 30., z: 24. };
    assert_eq!(
        state
            .execute_command(
                0,
                &Command::Dig {
                    ids: vec![bad, i64::from(id)],
                    kind: FortKind::Trench,
                    at,
                    direction: None,
                    queue: true,
                },
                false,
            )
            .unwrap(),
        Some(Denial::NoBuilders)
    );
    assert!(state.commands.units[&id].orders.is_empty());
    assert_eq!(
        Command::from_json(&json!({"t":"dig","ids":[id],"kind":"unknown","x":30,"z":24})),
        Err(Denial::Blocked)
    );

    let (mut tank, tank_id) = scene_with("tank", false);
    assert_eq!(
        tank.execute_command(0, &dig(tank_id), false).unwrap(),
        Some(Denial::NoBuilders)
    );
    let (mut retreating, retreat_id) = scene();
    retreating
        .world
        .game
        .units
        .get_mut(&retreat_id)
        .unwrap()
        .retreating = true;
    assert_eq!(
        retreating
            .execute_command(0, &dig(retreat_id), false)
            .unwrap(),
        Some(Denial::Retreating)
    );

    state.start.economy.players[0].mp = 0.;
    assert_eq!(
        state
            .execute_command(
                0,
                &Command::Dig {
                    ids: vec![i64::from(id)],
                    kind: FortKind::Trench,
                    at,
                    direction: None,
                    queue: false,
                },
                false,
            )
            .unwrap(),
        Some(Denial::Manpower)
    );
    let mut fallback = dig(id);
    if let Command::Dig { direction, .. } = &mut fallback {
        *direction = None;
    }
    assert_eq!(state.execute_command(0, &fallback, false).unwrap(), None);
    assert_eq!(state.start.economy.players[0].mp, 0.);
    let Command::Dig { direction, .. } = &state.commands.units[&id].orders[0] else {
        panic!("queued Dig expected");
    };
    assert_eq!(
        direction.unwrap().to_bits(),
        std::f64::consts::FRAC_PI_2.to_bits()
    );
}

#[test]
fn queued_bridge_uses_bearing_and_concrete_requires_its_tier() {
    let (mut tiered, id) = scene_with("engineer", true);
    tiered.commands.units.get_mut(&id).unwrap().follow = 77;
    assert_eq!(
        tiered
            .execute_command(
                0,
                &Command::Dig {
                    ids: vec![i64::from(id)],
                    kind: FortKind::Wall,
                    at: Point { x: 30., z: 24. },
                    direction: None,
                    queue: true,
                },
                false,
            )
            .unwrap(),
        Some(Denial::Tier)
    );
    assert!(tiered.commands.units[&id].orders.is_empty());
    assert_eq!(tiered.commands.units[&id].follow, 0);

    let (mut bridge, id) = scene();
    for x in 15..=17 {
        bridge.world.set_cell(12 * 60 + x, b'W').unwrap();
    }
    assert_eq!(
        bridge
            .execute_command(
                0,
                &Command::Dig {
                    ids: vec![i64::from(id)],
                    kind: FortKind::Bridge,
                    at: Point { x: 30., z: 24. },
                    direction: None,
                    queue: true,
                },
                false,
            )
            .unwrap(),
        None
    );
    let Command::Dig { direction, .. } = &bridge.commands.units[&id].orders[0] else {
        panic!("queued Bridge expected");
    };
    assert_eq!(direction.unwrap().to_bits(), 0.0f64.to_bits());
}

#[test]
fn queued_dig_observes_eight_order_limit_without_spending() {
    let (mut state, id) = scene();
    let before = state.start.economy.players[0].mp;
    for _ in 0..8 {
        assert_eq!(state.execute_command(0, &dig(id), false).unwrap(), None);
    }
    assert_eq!(
        state.execute_command(0, &dig(id), false).unwrap(),
        Some(Denial::QueueFull)
    );
    assert_eq!(state.commands.units[&id].orders.len(), 8);
    assert_eq!(state.start.economy.players[0].mp, before);
}
