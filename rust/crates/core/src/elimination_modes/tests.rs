use super::*;
use crate::{
    economy::{Faction, Technology},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::{Command, Denial},
    match_runtime::PlayerConfig,
    production,
    unit_publication::StagedUnit,
    weather::WeatherChoice,
};
use serde_json::json;

pub(super) fn scene(mode: Mode, teams: &[i32]) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Elimination intent", "w":100,"h":100,
            "rows":vec![".".repeat(100);100],"heights":vec!["0".repeat(100);100],
            "spawns":[{"x":8,"y":8},{"x":91,"y":91},{"x":8,"y":91},{"x":91,"y":8}],
            "points":[{"x":50,"y":50,"mp":2,"vp":1}]
        }))
        .unwrap(),
    )
    .unwrap();
    let options = MatchOptions {
        seed: 701,
        mode,
        players: teams
            .iter()
            .enumerate()
            .map(|(slot, &team)| PlayerConfig {
                name: format!("p{slot}"),
                team,
                faction: Faction::try_from(0).unwrap(),
            })
            .collect(),
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: Some(1),
        living_weather: false,
        map_key: None,
        tech: true,
        supply: false,
        logistics: false,
        sandbox: false,
    };
    match mode {
        Mode::Annihilation => Match::new_annihilation(&map, options),
        Mode::Classic => Match::new_classic(&map, options),
        _ => unreachable!(),
    }
    .unwrap()
}
#[test]
fn classic_and_annihilation_initialize_physical_logistics_and_deliver_every_seat() {
    for mode in [Mode::Classic, Mode::Annihilation] {
        let mut m = scene(mode, &[0, 1]);
        m.options.logistics = true;
        m.initialize_logistics().unwrap();
        assert!(m.convoys.is_some());
        assert!(
            m.world
                .game
                .units
                .values()
                .any(|u| { !m.rules.kinds[u.kind].structure && m.logistics.get(u.id).is_some() })
        );
        m.validate_tick_state().unwrap();
        assert!(m.logistics.iter().next().is_some());
        let selected = (0..m.start.players.len())
            .map(|slot| crate::recipient_snapshot::SelectedSeat {
                slot,
                full: false,
                omit: false,
            })
            .collect::<Vec<_>>();
        let mut delivery = crate::recipient_snapshot::DeliveryState::new(&m).unwrap();
        for _ in 0..3 {
            let report = m.advance_tick().unwrap();
            m.validate_logistics_runtime().unwrap();
            let frame = delivery
                .capture_selected_with_tick_events(&mut m, &selected, &report.events)
                .unwrap()
                .unwrap();
            for seat in &selected {
                assert_eq!(
                    frame.recipient(seat.slot).unwrap().into_json()["logistics"]["enabled"],
                    true
                );
            }
        }
    }
}

pub(super) fn ids(m: &Match, owner: usize, name: &str) -> Vec<u32> {
    m.world
        .game
        .units
        .values()
        .filter(|u| u.owner == owner && m.rules.kinds[u.kind].name == name)
        .map(|u| u.id)
        .collect()
}
fn destroy(m: &mut Match, owner: usize, name: &str) {
    for id in ids(m, owner, name) {
        m.world.game.units.get_mut(&id).unwrap().hp = 0.0;
    }
}
pub(super) fn add(m: &mut Match, owner: usize, name: &str, at: Point) -> u32 {
    let id = m.next_unit_id;
    let draft = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        owner,
        m.rules.kind(name).unwrap(),
        id,
        m.world.game.units.len() as u32,
        m.start.players[owner].spawn,
        Some(at),
    )
    .unwrap();
    m.publish_starting_unit(draft).unwrap();
    id
}

#[test]
fn classic_save_load_preserves_elimination_state_and_next_tick() {
    let mut original = scene(Mode::Classic, &[0, 1]);
    original.elimination.as_mut().unwrap().time_left = Some(0.05);
    original.advance_tick().unwrap();
    let expected = original.elimination.clone().unwrap();
    assert!(expected.sudden_death);
    assert!(!expected.nodes.is_empty());
    original.validate_tick_state().unwrap();

    // Room save admission still excludes Classic. Exercise its full Match checkpoint codec.
    let pointers = original.save_storage_pointers();
    let saved = crate::save_codec::to_value(&original).unwrap();
    let text = serde_json::to_vec(&saved).unwrap();
    let mut restored: Match =
        crate::save_codec::from_value(serde_json::from_slice(&text).unwrap()).unwrap();
    restored.relocate_save_storage(&pointers).unwrap();
    restored.validate_tick_state().unwrap();
    assert_eq!(restored.elimination.as_ref().unwrap(), &expected);
    assert_eq!(
        crate::save_codec::to_value(&restored.elimination).unwrap(),
        crate::save_codec::to_value(&original.elimination).unwrap()
    );

    original.advance_tick().unwrap();
    restored.advance_tick().unwrap();
    assert_eq!(restored.elimination, original.elimination);
    assert_eq!(restored.world.game.tick, original.world.game.tick);
    assert_eq!(
        crate::save_codec::to_value(&restored.world.game).unwrap(),
        crate::save_codec::to_value(&original.world.game).unwrap()
    );
    assert_eq!(
        restored.start.random.snapshot(),
        original.start.random.snapshot()
    );

    let mut without_elimination = saved;
    without_elimination[1]
        .as_array_mut()
        .unwrap()
        .retain(|entry| entry[0] != json!(["string", "elimination"]));
    let missing: Match = crate::save_codec::from_value(without_elimination).unwrap();
    assert!(missing.elimination.is_none());
    assert_eq!(
        missing.validate_elimination_state().unwrap_err(),
        "missing elimination state"
    );
}

#[test]
fn mode33_annihilation_setup_fortifies_every_player_and_tracks_bunker_maxima() {
    let m = scene(Mode::Annihilation, &[0, 0, 2]);
    let state = m.elimination.as_ref().unwrap();
    assert_eq!(state.teams, 2);
    assert_eq!(state.bunkers, vec![2, 0, 1]);
    assert_eq!(state.time_left, None);
    assert!(!state.sudden_death);
    for slot in 0..3 {
        assert_eq!(ids(&m, slot, "bunker").len(), 1);
        assert!(!ids(&m, slot, "flakpos").is_empty());
        assert_eq!(ids(&m, slot, "hq").len(), 1);
        assert_eq!(ids(&m, slot, "barracks").len(), 1);
        assert_eq!(m.start.economy.players[slot].vp, 0.0);
    }
    assert!(m.world.game.chars.contains(&b'T'));
    assert!(m.finish.is_none());
}

#[test]
fn mode34_annihilation_defeat_transfers_to_nearest_teammate_and_removes_lone_army() {
    let mut m = scene(Mode::Annihilation, &[0, 0, 1, 2]);
    let rifle = ids(&m, 0, "rifle")[0];
    let destination = m.start.players[1].spawn;
    let unit = m.world.game.units.get_mut(&rifle).unwrap();
    unit.x = destination.x;
    unit.z = destination.z;
    unit.path.push(Point { x: 80.0, z: 80.0 });
    unit.amove = Some(Point { x: 90.0, z: 90.0 });
    unit.retreating = true;
    m.commands.units.get_mut(&rifle).unwrap().attack_id = 123;
    m.commands
        .units
        .get_mut(&rifle)
        .unwrap()
        .orders
        .push_back(Command::Stop {
            ids: vec![i64::from(rifle)],
        });
    let old_building = ids(&m, 0, "hq")[0];
    let footprint = m.world.game.units[&old_building].cells.clone().unwrap();
    destroy(&mut m, 0, "bunker");
    let report = m.apply_annihilation_elimination().unwrap();
    assert!(m.start.economy.players[0].out);
    assert_eq!(m.world.game.units[&rifle].owner, 1);
    assert!(m.world.game.units[&rifle].path.is_empty());
    assert!(m.world.game.units[&rifle].amove.is_none());
    assert!(!m.world.game.units[&rifle].retreating);
    assert_eq!(m.commands.units[&rifle].attack_id, 0);
    assert!(m.commands.units[&rifle].orders.is_empty());
    assert!(report.removed.iter().any(|u| u.id == old_building));
    assert!(
        footprint
            .iter()
            .any(|&c| m.world.game.chars[c as usize] != b'K')
    );
    assert!(m.finish.is_none());
    // Three teams still stood at the first loss. A lone member now loses their army
    // while two other teams retain bunkers and can keep fighting.
    destroy(&mut m, 2, "bunker");
    m.apply_annihilation_elimination().unwrap();
    assert!(m.start.economy.players[2].out);
    assert!(m.world.game.units.values().all(|u| u.owner != 2));
    assert!(m.finish.is_none());
    let mut nearest = scene(Mode::Annihilation, &[0, 0, 0, 1]);
    let rifle = ids(&nearest, 0, "rifle")[0];
    let at = nearest.start.players[2].spawn;
    nearest.world.game.units.get_mut(&rifle).unwrap().x = at.x;
    nearest.world.game.units.get_mut(&rifle).unwrap().z = at.z;
    destroy(&mut nearest, 0, "bunker");
    nearest.apply_annihilation_elimination().unwrap();
    assert_eq!(nearest.world.game.units[&rifle].owner, 2);
}

#[test]
fn mode35_annihilation_terminal_reveal_draw_and_solo_nontermination() {
    let mut m = scene(Mode::Annihilation, &[0, 1]);
    let army = ids(&m, 1, "rifle");
    destroy(&mut m, 1, "bunker");
    m.fallen.bunker = Some(Point { x: 61.25, z: 83.25 });
    m.apply_annihilation_elimination().unwrap();
    assert!(m.start.economy.players[1].out);
    assert!(army.iter().all(|id| m.world.game.units.contains_key(id)));
    m.apply_elimination_victory(0.05).unwrap();
    let finish = m.finish.unwrap();
    assert_eq!((finish.winner, finish.reason), (0, EndReason::Bunkers));
    assert_eq!(finish.end_at, Point { x: 61.3, z: 83.3 });
    assert!(m.world.game.reveal);
    let mut draw = scene(Mode::Annihilation, &[0, 1]);
    destroy(&mut draw, 0, "bunker");
    destroy(&mut draw, 1, "bunker");
    draw.apply_annihilation_elimination().unwrap();
    draw.apply_elimination_victory(0.05).unwrap();
    assert_eq!(
        (draw.finish.unwrap().winner, draw.finish.unwrap().reason),
        (-1, EndReason::Draw)
    );
    let mut solo = scene(Mode::Annihilation, &[0]);
    destroy(&mut solo, 0, "bunker");
    solo.apply_annihilation_elimination().unwrap();
    solo.apply_elimination_victory(100.0).unwrap();
    assert!(solo.finish.is_none());
}

#[test]
fn mode36_out_players_cannot_command_capture_earn_income_or_kill_bounties() {
    for mode in [Mode::Annihilation, Mode::Classic] {
        let mut m = scene(mode, &[0, 1, 2]);
        let id = ids(&m, 0, "rifle")[0];
        m.start.economy.players[0].out = true;
        let commands = [
            json!({"t":"stop","ids":[id]}),
            json!({"t":"move","orders":[[id,20,20]]}),
            json!({"t":"amove","orders":[[id,20,20]]}),
            json!({"t":"attack","ids":[id],"target":2}),
            json!({"t":"retreat","ids":[id]}),
            json!({"t":"buy","unit":"rifle"}),
            json!({"t":"tech","kind":"tier"}),
            json!({"t":"build","ids":[id],"kind":"hq","x":70,"z":70}),
            json!({"t":"assist","ids":[id],"id":2}),
            json!({"t":"ability","ids":[id],"x":20,"z":20}),
            json!({"t":"support","kind":"recon","x":20,"z":20}),
            json!({"t":"stance","ids":[id],"key":"holdPos","on":true}),
            json!({"t":"escort","ids":[id],"target":2}),
            json!({"t":"cover","ids":[id]}),
            json!({"t":"board","ids":[id],"target":2}),
            json!({"t":"unload","ids":[id]}),
            json!({"t":"destroy","ids":[id]}),
            json!({"t":"autocast","ids":[id],"on":true}),
            json!({"t":"dig","ids":[id],"kind":"trench","x":20,"z":20}),
            json!({"t":"entrench","ids":[id],"fort":"trench","x":20,"z":20,"x2":24,"z2":20}),
        ];
        for raw in commands {
            let command = Command::from_json(&raw).expect("authored valid command");
            assert!(!matches!(command, Command::Unsupported), "{raw}");
            assert_eq!(
                m.execute_command(0, &command, false).unwrap(),
                Some(Denial::Blocked),
                "{mode:?}: {raw}"
            );
        }
        let point = m.start.points[0].at;
        let unit = m.world.game.units.get_mut(&id).unwrap();
        unit.x = point.x;
        unit.z = point.z;
        m.start.points[0].owner = Some(0);
        let progress = m.start.points[0].progress;
        let wallet = m.start.economy.players[0].clone();
        let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
        m.apply_point_capture_income(&order).unwrap();
        assert_eq!(m.start.points[0].progress, progress);
        assert!(!m.start.points[0].on_point.contains(&id));
        assert_eq!(m.start.economy.players[0].mp, wallet.mp);
        assert_eq!(m.start.economy.players[0].mun, wallet.mun);
        assert_eq!(m.start.economy.players[0].fuel, wallet.fuel);
        let victim = ids(&m, 1, "rifle")[0];
        m.world.game.units.get_mut(&victim).unwrap().hp = 0.0;
        m.combat.get_mut(victim).unwrap().last_hit = Some(0);
        m.retire_dead(&[victim]).unwrap();
        assert_eq!(m.start.economy.players[0].mp, wallet.mp);
    }
}

#[test]
fn mode37_classic_setup_has_clock_hq_engineers_and_separated_home_fuel_nodes() {
    let m = scene(Mode::Classic, &[0, 1]);
    let state = m.elimination.as_ref().unwrap();
    assert_eq!(state.time_left, Some(1500.0));
    assert_eq!(state.teams, 2);
    assert_eq!(state.nodes.iter().filter(|n| !n.fuel).count(), 4);
    assert_eq!(state.nodes.iter().filter(|n| n.fuel).count(), 2);
    for slot in 0..2 {
        let hq = ids(&m, slot, "hq");
        assert_eq!(hq.len(), 1);
        assert!(ids(&m, slot, "barracks").is_empty());
        assert_eq!(ids(&m, slot, "engineer").len(), 1);
        assert_eq!(ids(&m, slot, "rifle").len(), 1);
        assert_eq!(m.world.game.units[&hq[0]].at(), m.start.players[slot].spawn);
        assert_eq!(m.world.game.units[&hq[0]].built, Some(1.0));
        for unit in m
            .world
            .game
            .units
            .values()
            .filter(|u| u.owner == slot && !m.rules.kinds[u.kind].structure)
        {
            assert_eq!(m.world.game.flags_at(unit.x, unit.z) & crate::game::MOVE, 0);
        }
    }
    for (i, node) in state.nodes.iter().enumerate() {
        assert!(node.depot.is_none());
        assert!(
            state.nodes[..i]
                .iter()
                .all(|other| math::hypot(node.at.x - other.at.x, node.at.z - other.at.z) >= 10.0)
        );
        assert!(
            m.start
                .players
                .iter()
                .all(|p| math::hypot(node.at.x - p.spawn.x, node.at.z - p.spawn.z) >= 10.0)
        );
    }
}

#[test]
fn mode38_classic_sudden_death_clears_queues_decays_and_blocks_paid_actions() {
    let mut m = scene(Mode::Classic, &[0, 1]);
    let hq = ids(&m, 0, "hq")[0];
    let eng = ids(&m, 0, "engineer")[0];
    m.start.economy.players[0].mp = 1000.0;
    m.recorded_world_buy(0, "rifle", Some(hq)).unwrap();
    let waiting = m.recorded_world_buy(0, "rifle", Some(hq)).unwrap();
    assert_eq!(m.production.building(hq).unwrap().jobs().len(), 2);
    let wallet = m.start.economy.players[0].mp;
    m.elimination.as_mut().unwrap().time_left = Some(0.05);
    let hp = m.world.game.units[&hq].hp;
    m.apply_elimination_victory(0.05).unwrap();
    assert!(m.elimination_sudden_death());
    assert_eq!(m.production.building(hq).unwrap().jobs().len(), 0);
    assert_eq!(m.production.building(hq).unwrap().progress(), 0.0);
    assert_eq!(m.start.economy.players[0].mp, wallet);
    assert!((m.world.game.units[&hq].hp - (hp - 3000.0 * 0.01 * 0.05)).abs() < 1e-9);
    assert_eq!(
        m.recorded_world_buy(0, "rifle", Some(hq)).unwrap_err(),
        crate::match_runtime::ProductionStepError::Production(production::Error::Denied(
            production::Denial::SuddenDeath
        ))
    );
    assert_eq!(
        m.cancel_training(0, hq, waiting).unwrap_err(),
        crate::match_runtime::ProductionStepError::Production(production::Error::Denied(
            production::Denial::SuddenDeath
        ))
    );
    assert_eq!(
        m.request_research(0, Technology::Tier).unwrap_err(),
        crate::match_runtime::ProductionStepError::Production(production::Error::Denied(
            production::Denial::SuddenDeath
        ))
    );
    assert_eq!(
        m.execute_build(
            0,
            &[i64::from(eng)],
            "barracks",
            Some(Point { x: 50.0, z: 30.0 }),
            None,
            false
        )
        .unwrap(),
        Some(Denial::SuddenDeath)
    );
    // A site remains incomplete even with a ready worker on it.
    let cell = 25 * m.world.game.w + 25;
    m.place_starting_building(0, "barracks", cell).unwrap();
    let site = ids(&m, 0, "barracks")[0];
    m.world.game.units.get_mut(&site).unwrap().built = Some(0.5);
    let at = m.world.game.units[&site].at();
    let worker = m.world.game.units.get_mut(&eng).unwrap();
    worker.x = at.x;
    worker.z = at.z;
    worker.build = true;
    m.commands.units.get_mut(&eng).unwrap().build_target = Some(site);
    let mut frame = m.begin_construction_tick().unwrap();
    m.step_construction_worker(eng, &mut frame).unwrap();
    m.finish_construction_tick(frame).unwrap();
    assert_eq!(m.world.game.units[&site].built, Some(0.5));
    // Enough engineers would exceed decay. Sudden Death caps the combined repair
    // at 0.6% of maximum HP per second so repair cannot defeat the timer.
    m.world.game.units.get_mut(&site).unwrap().built = Some(1.0);
    m.world.game.units.get_mut(&site).unwrap().hp = 750.0;
    let sections = m
        .world
        .structures
        .iter()
        .find(|s| s.entity_id == Some(site))
        .unwrap()
        .section_ids
        .clone();
    for section in sections {
        let (_, health) = m.world.section_by_id_mut(&section).unwrap();
        health.hp = health.max_hp.unwrap() * 0.5;
    }
    let mut workers = vec![eng];
    for _ in 0..3 {
        let id = add(&mut m, 0, "engineer", at);
        m.world.game.units.get_mut(&id).unwrap().build = true;
        m.commands.units.get_mut(&id).unwrap().build_target = Some(site);
        workers.push(id);
    }
    let hp = m.world.game.units[&site].hp;
    let mut frame = m.begin_construction_tick().unwrap();
    for id in workers {
        m.step_construction_worker(id, &mut frame).unwrap();
    }
    m.finish_construction_tick(frame).unwrap();
    let repaired = m.world.game.units[&site].hp - hp;
    assert!(repaired > 0.0);
    assert!((repaired - 1500.0 * 0.006 * m.rules.movement.tick).abs() < 1e-7);
}

#[test]
fn mode39_classic_sites_keep_players_alive_and_defeat_transfers_armies_wrecks_buildings() {
    let mut m = scene(Mode::Classic, &[0, 0, 1]);
    let cell = 30 * m.world.game.w + 30;
    m.place_starting_building(1, "barracks", cell).unwrap();
    let site = ids(&m, 1, "barracks")[0];
    m.world.game.units.get_mut(&site).unwrap().built = Some(0.0);
    destroy(&mut m, 1, "hq");
    m.apply_elimination_victory(0.05).unwrap();
    assert!(!m.start.economy.players[1].out);
    assert!(m.finish.is_none());
    let army = ids(&m, 1, "rifle");
    m.place_starting_building(1, "flakpos", 35 * m.world.game.w + 35)
        .unwrap();
    let decorative = ids(&m, 1, "flakpos")[0];
    destroy(&mut m, 1, "barracks");
    let report = m.apply_elimination_victory(0.05).unwrap();
    assert!(m.start.economy.players[1].out);
    assert!(army.iter().all(|id| m.world.game.units[id].owner == 0));
    assert!(!m.world.game.units.contains_key(&decorative));
    assert!(report.removed.iter().any(|u| u.id == decorative));
    assert!(m.finish.is_none());
}

#[test]
fn mode40_classic_last_producer_wins_simultaneous_destruction_draws_and_clock_stops() {
    let mut m = scene(Mode::Classic, &[0, 1]);
    let at = m.world.game.units[&ids(&m, 1, "hq")[0]].at();
    destroy(&mut m, 1, "hq");
    m.apply_elimination_victory(0.05).unwrap();
    assert_eq!(
        (m.finish.unwrap().winner, m.finish.unwrap().reason),
        (0, EndReason::Headquarters)
    );
    assert_eq!(m.finish.unwrap().end_at, at);
    assert!(m.world.game.units.values().all(|u| u.owner != 1));
    let clock = m.elimination.as_ref().unwrap().time_left;
    m.apply_elimination_victory(1.0).unwrap();
    assert_eq!(m.elimination.as_ref().unwrap().time_left, clock);
    let mut draw = scene(Mode::Classic, &[0, 1]);
    destroy(&mut draw, 0, "hq");
    destroy(&mut draw, 1, "hq");
    draw.apply_elimination_victory(0.05).unwrap();
    assert_eq!(
        (draw.finish.unwrap().winner, draw.finish.unwrap().reason),
        (-1, EndReason::Draw)
    );
}

#[test]
fn elimination_modes_run_connected_ordinary_ticks() {
    for mode in [Mode::Annihilation, Mode::Classic] {
        let mut m = scene(mode, &[0, 1]);
        let before = m.start.economy.players[0].mp;
        let report = m.advance_tick().unwrap();
        assert_eq!(report.tick, 1);
        assert!(m.start.economy.players[0].mp > before);
        assert!(m.finish.is_none());
        if mode == Mode::Classic {
            assert_eq!(m.elimination.as_ref().unwrap().time_left, Some(1499.95));
        }
    }
}

#[test]
fn annihilation_tick_eliminates_before_capture_and_income_while_other_teams_play() {
    let mut m = scene(Mode::Annihilation, &[0, 0, 1, 2]);
    let point = m.start.points[0].at;
    m.start.points[0].owner = None;
    m.start.points[0].capper = Some(0);
    m.start.points[0].progress = 0.999;
    let id = ids(&m, 0, "rifle")[0];
    let unit = m.world.game.units.get_mut(&id).unwrap();
    unit.x = point.x;
    unit.z = point.z;
    destroy(&mut m, 0, "bunker");
    destroy(&mut m, 1, "bunker");
    m.grid.rebuild(&m.world.game.units);
    let balances = m
        .start
        .economy
        .players
        .iter()
        .map(|p| p.mp)
        .collect::<Vec<_>>();
    let report = m.advance_tick().unwrap();
    assert_eq!(report.tick, 1);
    assert!(m.start.economy.players[0].out && m.start.economy.players[1].out);
    assert!(m.world.game.units.values().all(|u| u.owner >= 2));
    assert_eq!(m.start.points[0].owner, None);
    assert_eq!(m.start.points[0].progress, 0.999);
    assert_eq!(m.start.economy.players[0].mp, balances[0]);
    assert_eq!(m.start.economy.players[1].mp, balances[1]);
    assert!(m.start.economy.players[2].mp > balances[2]);
    assert!(m.finish.is_none());
}

#[test]
fn classic_tick_pays_defeat_tick_income_then_finishes_at_last_production_death() {
    let mut m = scene(Mode::Classic, &[0, 1]);
    let hq = ids(&m, 1, "hq")[0];
    let at = m.world.game.units[&hq].at();
    let balance = m.start.economy.players[1].mp;
    destroy(&mut m, 1, "hq");
    let report = m.advance_tick().unwrap();
    assert_eq!(report.tick, 1);
    assert!(m.start.economy.players[1].mp > balance);
    assert!(m.start.economy.players[1].out);
    assert!(m.world.game.units.values().all(|u| u.owner == 0));
    let finish = m.finish.unwrap();
    assert_eq!(
        (finish.winner, finish.reason, finish.end_tick),
        (0, EndReason::Headquarters, 1)
    );
    assert_eq!(finish.end_at, at);
    assert_eq!(m.elimination.as_ref().unwrap().time_left, Some(1499.95));
}

#[test]
fn elimination_modes_recorded_buy_and_research_reach_their_live_tick_phases() {
    for mode in [Mode::Annihilation, Mode::Classic] {
        let mut m = scene(mode, &[0, 1]);
        m.start.economy.players[0].mp = 5000.0;
        m.start.economy.players[0].mun = 5000.0;
        m.start.economy.players[0].fuel = 5000.0;
        let rifles = ids(&m, 0, "rifle").len();
        m.enqueue_command(
            crate::orders::OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 1,
            },
            Command::Technology {
                kind: Technology::Tier,
            },
        )
        .unwrap();
        m.enqueue_command(
            crate::orders::OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 2,
            },
            Command::Buy {
                kind: "rifle".into(),
                from: None,
            },
        )
        .unwrap();
        let report = m.advance_tick().unwrap();
        assert_eq!(report.commands.len(), 2);
        assert!(report.commands.iter().all(|result| result.denial.is_none()));
        assert_eq!(m.production.player(0).unwrap().jobs().len(), 1);
        assert_eq!(
            m.production.player(0).unwrap().jobs()[0].progress,
            m.rules.movement.tick
        );
        assert!(m.story[0].mp_spent > 0.0);
        if mode == Mode::Classic {
            let hq = ids(&m, 0, "hq")[0];
            assert_eq!(m.production.building(hq).unwrap().jobs().len(), 1);
            assert_eq!(
                m.production.building(hq).unwrap().progress(),
                m.rules.movement.tick
            );
            assert_eq!(ids(&m, 0, "rifle").len(), rifles);
        } else {
            assert_eq!(ids(&m, 0, "rifle").len(), rifles + 1);
        }
    }
}

#[test]
fn classic_uses_own_producers_for_retreat_and_allied_producers_for_recovery() {
    let mut m = scene(Mode::Classic, &[0, 0, 1]);
    let rifle = ids(&m, 0, "rifle")[0];
    m.place_starting_building(0, "barracks", 30 * m.world.game.w + 30)
        .unwrap();
    let base = ids(&m, 0, "barracks")[0];
    let at = m.world.game.units[&base].at();
    let unit = m.world.game.units.get_mut(&rifle).unwrap();
    unit.x = at.x;
    unit.z = at.z;
    assert_eq!(m.reaction_home(rifle).unwrap(), at);
    m.world.game.units.get_mut(&base).unwrap().built = Some(0.5);
    assert_eq!(m.reaction_home(rifle).unwrap(), m.start.players[0].spawn);
    m.world.game.units.get_mut(&base).unwrap().built = Some(1.0);
    let ally = m.start.players[1].spawn;
    let radius = number(
        Catalog::compiled().unwrap().table("CFG").unwrap(),
        "reinforceRadius",
    )
    .unwrap();
    assert!(
        m.world
            .game
            .units
            .values()
            .filter(|u| u.owner == 0 && m.rules.kinds[u.kind].structure)
            .all(|u| math::hypot(u.x - ally.x, u.z - ally.z) > radius)
    );
    let unit = m.world.game.units.get_mut(&rifle).unwrap();
    unit.x = ally.x;
    unit.z = ally.z;
    unit.hp = 60.0;
    m.start.economy.players[0].mp = 1000.0;
    m.recovery.get_mut(rifle).unwrap().reinforcement = number(
        Catalog::compiled().unwrap().table("CFG").unwrap(),
        "reinforceEvery",
    )
    .unwrap();
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let frame = m.prepare_recovery(&order).unwrap();
    let wallet = m.start.economy.players[0].mp;
    m.reinforce(&frame, 0.05).unwrap();
    assert!(m.world.game.units[&rifle].hp > 60.0);
    assert!(m.start.economy.players[0].mp < wallet);
}

#[test]
fn classic_support_uses_munitions_and_abilities_refuse_an_empty_munitions_wallet() {
    let mut m = scene(Mode::Classic, &[0, 1]);
    let at = m.start.players[0].spawn;
    let price = crate::economy::sup_cost(Mode::Classic, "recon").unwrap();
    let before = m.story[0].mp_spent;
    m.start.economy.players[0].mp = 0.0;
    m.start.economy.players[0].mun = price.cost;
    assert_eq!(m.admit_support(0, "recon", at, None).unwrap(), None);
    assert_eq!(m.start.economy.players[0].mp, 0.0);
    assert_eq!(m.start.economy.players[0].mun, 0.0);
    assert_eq!(m.story[0].mp_spent, before);
    m.start.players[0].support.insert("recon".into(), 0.0);
    m.start.economy.players[0].mp = 1000.0;
    assert_eq!(
        m.admit_support(0, "recon", at, None).unwrap(),
        Some(crate::match_support::SupportDenial::Munitions)
    );
    let rifle = ids(&m, 0, "rifle")[0];
    let cmd = Command::from_json(&json!({"t":"ability","ids":[rifle],"x":at.x,"z":at.z})).unwrap();
    assert_eq!(
        m.execute_command(0, &cmd, false).unwrap(),
        Some(Denial::Munitions)
    );
}

#[test]
fn if25_elimination_reuses_teammate_sort_order_for_later_equal_distance_transfer() {
    let mut m = scene(Mode::Annihilation, &[0, 0, 0, 1]);
    let at = m.start.players[2].spawn;
    let middle = Point {
        x: (m.start.players[1].spawn.x + at.x) / 2.0,
        z: (m.start.players[1].spawn.z + at.z) / 2.0,
    };
    let army: Vec<_> = m
        .world
        .game
        .units
        .values()
        .filter(|u| u.owner == 0 && !m.rules.kinds[u.kind].building)
        .map(|u| u.id)
        .collect();
    assert!(army.len() >= 2);
    for id in &army {
        let u = m.world.game.units.get_mut(id).unwrap();
        u.x = at.x;
        u.z = at.z;
    }
    let later = m.world.game.units.get_mut(&army[1]).unwrap();
    later.x = middle.x;
    later.z = middle.z;
    m.grid.rebuild(&m.world.game.units);
    destroy(&mut m, 0, "bunker");
    m.apply_annihilation_elimination().unwrap();
    assert_eq!(m.world.game.units[&army[0]].owner, 2);
    assert_eq!(m.world.game.units[&army[1]].owner, 2);
    for _ in 0..3 {
        m.advance_tick().unwrap();
        assert!(m.tick_state.fault().is_none());
    }
}

#[test]
fn if25_classic_node_pairs_keep_enemy_ties_and_first_discovery_order() {
    let mut m = scene(Mode::Classic, &[0, 1, 2, 3]);
    for player in &mut m.start.players {
        player.spawn = Point { x: 100.0, z: 100.0 };
    }
    assert_eq!(
        classic_node_pairs(&m.start.players),
        vec![(0, 1), (0, 2), (1, 2), (0, 3), (1, 3)]
    );
    // Slot 0 has three equally distant enemies. Only the first two qualify.
    m.start.players[0].config.team = 0;
    m.start.players[1].config.team = 0;
    m.start.players[2].config.team = 1;
    m.start.players[3].config.team = 1;
    assert_eq!(
        classic_node_pairs(&m.start.players),
        vec![(0, 2), (0, 3), (1, 2), (1, 3)]
    );
}

#[path = "classic_bases_tests.rs"]
mod classic_bases_tests;
