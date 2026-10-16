use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, Denial},
    match_human_observation::HumanObservationMemory,
    match_runtime::{MatchOptions, PlayerConfig, foot_center},
    match_world::ResourceNode,
    recipient_snapshot::SnapshotCache,
    weather::WeatherChoice,
};
use serde_json::json;

fn scene(reverse: bool) -> (Match, u32) {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Depot selection", "w":80,"h":80,
            "rows":vec![".".repeat(80);80],"heights":vec!["0".repeat(80);80],
            "spawns":[{"x":8,"y":8},{"x":71,"y":71}],
            "points":[{"x":40,"y":40}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut state = Match::new_classic(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Classic,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("p{slot}"),
                    team: slot,
                    faction: Faction::Usa,
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    let worker = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "engineer")
        .unwrap()
        .id;
    let unit = state.world.game.units.get_mut(&worker).unwrap();
    unit.x = 22.0;
    unit.z = 22.0;
    state.grid.rebuild(&state.world.game.units);
    let mut nodes: Vec<_> = [(15, 11), (11, 15)]
        .into_iter()
        .enumerate()
        .map(|(index, (x, z))| {
            let cell = z * state.world.game.w + x;
            ResourceNode {
                cell,
                at: foot_center(&state.world.game, cell, 2),
                depot: None,
                rate: if index == 0 { 1.75 } else { 0.0 },
                fuel: index == 1,
            }
        })
        .collect();
    if reverse {
        nodes.reverse();
    }
    state.elimination.as_mut().unwrap().nodes = nodes;
    state.start.economy.players[0].mp = 5000.0;
    (state, worker)
}
fn command(worker: u32, at: Point) -> Command {
    Command::Build {
        ids: vec![i64::from(worker)],
        kind: "depot".into(),
        at: Some(at),
        direction: None,
        queue: false,
    }
}
fn ticks(state: &mut Match, count: usize) {
    for _ in 0..count {
        state.advance_tick().unwrap();
        assert!(state.tick_state.fault().is_none());
        let cache = SnapshotCache::new(state).unwrap();
        for slot in 0..state.start.players.len() {
            cache.recipient(slot).unwrap();
        }
    }
}

#[test]
fn if25_depot_placement_keeps_canonical_node_ties_through_ticks_and_snapshots() {
    for reverse in [false, true] {
        let (mut state, worker) = scene(reverse);
        let expected = state.elimination.as_ref().unwrap().nodes[0].clone();
        let before = state.start.economy.players[0].mp;
        let site = state.next_unit_id;
        assert_eq!(
            state
                .execute_command(0, &command(worker, Point { x: 28.0, z: 28.0 }), false)
                .unwrap(),
            None
        );
        assert_eq!(state.world.game.units[&site].at(), expected.at);
        assert_eq!(
            state.world.game.units[&site].cells.as_ref().unwrap()[0],
            expected.cell as u32
        );
        assert_eq!(
            state.elimination.as_ref().unwrap().nodes[0].depot,
            Some(site)
        );
        assert_eq!(state.elimination.as_ref().unwrap().nodes[1].depot, None);
        let cost = number(Catalog::compiled().unwrap().unit("depot").unwrap(), "cost").unwrap();
        assert_eq!(state.start.economy.players[0].mp, before - cost);
        ticks(&mut state, 4);
        assert_eq!(
            state.elimination.as_ref().unwrap().nodes[0].depot,
            Some(site)
        );
        let canonical = &state.elimination.as_ref().unwrap().nodes;
        let positions = canonical.iter().map(|node| node.at).collect::<Vec<_>>();
        let worth = canonical
            .iter()
            .map(|node| if node.fuel { 2.5 } else { node.rate })
            .collect::<Vec<_>>();
        assert_eq!(
            worth,
            if reverse {
                vec![2.5, 1.75]
            } else {
                vec![1.75, 2.5]
            }
        );
        let mut memory = HumanObservationMemory::new(0);
        let view = state.human_commander_observation(&mut memory).unwrap();
        let context = view.packet.input().runtime.as_ref().unwrap();
        assert_eq!(context.nodes, positions);
        assert_eq!(context.node_worth, worth);
        assert_eq!(context.claimed_nodes, std::collections::BTreeSet::from([0]));
        assert!(
            view.packet
                .input()
                .units
                .iter()
                .any(|unit| unit.id == site && unit.kind == "depot")
        );
        assert!(!state.recipient_sees(1, expected.at).unwrap());
        let other = state
            .human_commander_observation(&mut HumanObservationMemory::new(1))
            .unwrap();
        let other_context = other.packet.input().runtime.as_ref().unwrap();
        assert_eq!(
            other_context.nodes, positions,
            "resource positions are public"
        );
        assert_eq!(other_context.node_worth, worth);
        assert!(
            other_context.claimed_nodes.is_empty(),
            "unseen enemy Depot claims stay private"
        );
        assert!(other_context.known_depots.is_empty());
        assert!(
            other
                .packet
                .input()
                .units
                .iter()
                .all(|unit| unit.id != site)
        );
        assert_eq!(state.cancel_construction(0, site).unwrap(), None);
        assert_eq!(state.elimination.as_ref().unwrap().nodes[0].depot, None);
        ticks(&mut state, 2);
        let view = state.human_commander_observation(&mut memory).unwrap();
        assert!(
            view.packet
                .input()
                .runtime
                .as_ref()
                .unwrap()
                .claimed_nodes
                .is_empty()
        );
        ticks(&mut state, 1);
    }
}

#[test]
fn if25_depot_claim_skips_claimed_nodes_and_releases_on_ordinary_death() {
    let (mut state, worker) = scene(false);
    let site = state.next_unit_id;
    let node = state.elimination.as_ref().unwrap().nodes[0].at;
    assert_eq!(
        state
            .execute_command(0, &command(worker, node), false)
            .unwrap(),
        None
    );
    let other = state.next_unit_id;
    assert_eq!(
        state
            .execute_command(0, &command(worker, Point { x: 28.0, z: 28.0 }), false)
            .unwrap(),
        None
    );
    assert_eq!(
        state.elimination.as_ref().unwrap().nodes[1].depot,
        Some(other)
    );
    assert_eq!(
        state.world.game.units[&other].at(),
        state.elimination.as_ref().unwrap().nodes[1].at
    );
    state.world.game.units.get_mut(&site).unwrap().hp = 0.0;
    ticks(&mut state, 1);
    assert!(!state.world.game.units.contains_key(&site));
    assert_eq!(state.elimination.as_ref().unwrap().nodes[0].depot, None);
    assert_eq!(
        state.elimination.as_ref().unwrap().nodes[1].depot,
        Some(other)
    );
    ticks(&mut state, 3);
}

#[test]
fn if25_depot_rejects_nonfinite_and_out_of_range_clicks_without_claims_or_payment() {
    for at in [
        Point {
            x: f64::NAN,
            z: 28.0,
        },
        Point { x: 60.0, z: 60.0 },
    ] {
        let (mut state, worker) = scene(false);
        let wallet = state.start.economy.players[0].mp;
        let next = state.next_unit_id;
        assert_eq!(
            state
                .execute_command(0, &command(worker, at), false)
                .unwrap(),
            Some(Denial::Blocked)
        );
        assert_eq!(state.start.economy.players[0].mp, wallet);
        assert_eq!(state.next_unit_id, next);
        assert!(
            state
                .elimination
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .all(|n| n.depot.is_none())
        );
        ticks(&mut state, 2);
    }
}

#[test]
fn if25_classic_ghost_discovery_order_reaches_observation_and_tied_raid_selection() {
    use crate::{
        commander_ai::{Level, Observation, PolicyError},
        commander_tactics::Skill,
        unit_publication::{PublicationParts, StagedUnit, prepare_publication},
        vision::KnownBuilding,
    };
    let (mut state, _) = scene(false);
    // A second ordinary rifle lets the real raid planner choose its two-squad crew.
    let id = state.next_unit_id;
    let staged = StagedUnit::ordinary(
        &state.world.game,
        &state.rules,
        0,
        state.rules.kind("rifle").unwrap(),
        id,
        state.world.game.units.len() as u32,
        state.start.players[0].spawn,
        Some(Point { x: 24.0, z: 20.0 }),
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
        state.start.players.len(),
        vec![staged],
    )
    .unwrap()
    .publish_with(|unit, _| {
        state.combat.initialize(unit.id);
        state.firing.initialize(unit.id);
        state.abilities.initialize(unit.id);
        state.recovery.initialize(unit.id);
    });
    let first = state.next_unit_id + 101;
    let second = state.next_unit_id + 100;
    let positions = [Point { x: 100.0, z: 60.0 }, Point { x: 60.0, z: 100.0 }];
    let depot = state.rules.kind("depot").unwrap();
    let ghosts = [first, second]
        .into_iter()
        .zip(positions)
        .map(|(id, at)| KnownBuilding {
            id,
            kind: depot,
            owner: 1,
            at,
            built: Some(1.0),
        })
        .collect::<Vec<_>>();
    let team = state.world.game.players[0].team;
    state.classic_known_buildings = vec![(team, ghosts)];
    // These are last observed enemy buildings under fog, with no current live actor.
    assert!(
        positions
            .iter()
            .all(|&at| !state.recipient_sees(0, at).unwrap())
    );
    assert!(
        [first, second]
            .iter()
            .all(|id| !state.world.game.units.contains_key(id))
    );
    ticks(&mut state, 2);
    let view = state
        .human_commander_observation(&mut HumanObservationMemory::new(0))
        .unwrap();
    let context = view.packet.input().runtime.as_ref().unwrap();
    assert_eq!(
        context
            .known_buildings
            .iter()
            .take(2)
            .map(|row| row.id)
            .collect::<Vec<_>>(),
        [first, second]
    );
    assert_eq!(
        context
            .known_buildings
            .iter()
            .take(2)
            .map(|row| (row.kind.as_str(), row.owner, row.at))
            .collect::<Vec<_>>(),
        vec![("depot", 1, positions[0]), ("depot", 1, positions[1])]
    );
    assert_eq!(context.known_depots, positions);
    let spawn = view.packet.input().players[0].spawn;
    assert_eq!(
        math::hypot(positions[0].x - spawn.x, positions[0].z - spawn.z),
        math::hypot(positions[1].x - spawn.x, positions[1].z - spawn.z)
    );
    let mine = view
        .packet
        .input()
        .units
        .iter()
        .filter(|unit| unit.owner == 0 && unit.kind == "rifle")
        .collect::<Vec<_>>();
    assert_eq!(mine.len(), 2);
    let mut raid = None;
    let result = crate::commander_classic_rules::plan(
        &view.packet,
        context,
        &mine,
        &mine,
        &[],
        &mut raid,
        Skill::for_level(Level::Normal),
    )
    .unwrap();
    let raid = raid.unwrap();
    assert_eq!(
        raid.target, first,
        "equal-distance ghosts retain discovery order rather than ID order"
    );
    assert_eq!(raid.at, positions[0]);
    assert_eq!(result.attacks.len(), 2);
    assert!(result.attacks.iter().all(|(_, at)| *at == positions[0]));
    let mut duplicated = view.packet.input().clone();
    let context = duplicated.runtime.as_mut().unwrap();
    context
        .known_buildings
        .push(context.known_buildings[0].clone());
    assert!(matches!(
        Observation::new(duplicated),
        Err(PolicyError::RuntimeContext)
    ));
}
