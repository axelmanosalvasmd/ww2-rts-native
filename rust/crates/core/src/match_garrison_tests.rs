use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};
use serde_json::json;

fn scene() -> Match {
    let mut rows = vec![".".repeat(64); 64];
    rows[20].replace_range(20..22, "BB");
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"House entry", "w":64,"h":64,"rows":rows,
            "spawns":[{"x":4,"y":4},{"x":59,"y":59}],
            "points":[{"x":32,"y":32}], "buildings":[]
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "A".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "B".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
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
    .unwrap()
}

fn add(m: &mut Match, name: &str, at: Point) -> u32 {
    let id = m.next_unit_id;
    let staged = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        0,
        m.rules.kind(name).unwrap(),
        id,
        m.world.game.units.len() as u32,
        m.start.players[0].spawn,
        Some(at),
    )
    .unwrap();
    prepare_publication(
        PublicationParts {
            units: &mut m.world.game.units,
            grid: &mut m.grid,
            controls: &mut m.commands.units,
            recruitment: &mut m.recruit_metadata,
            next_id: &mut m.next_unit_id,
        },
        &m.rules,
        2,
        vec![staged],
    )
    .unwrap()
    .publish_with(|u, _| {
        m.combat.initialize(u.id);
        m.firing.initialize(u.id);
        m.abilities.initialize(u.id);
        m.recovery.initialize(u.id);
    });
    crate::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    id
}

fn command(m: &mut Match, ids: &[u32], queue: bool) -> Option<Denial> {
    let command = Command::from_json(&json!({
        "t":"garrison", "ids":ids, "x":41,"z":41,"queue":queue
    }))
    .unwrap();
    m.execute_command(0, &command, false).unwrap()
}

#[test]
fn eng33_all_nine_squads_enter_through_the_native_tick_but_tanks_and_retreaters_cannot() {
    for name in [
        "rifle",
        "mg",
        "sniper",
        "medic",
        "flamer",
        "ranger",
        "conscript",
        "commando",
        "engineer",
    ] {
        let mut m = scene();
        let at = Point { x: 38.0, z: 41.0 };
        let id = add(&mut m, name, at);
        let house = m.world.game.cell_of(41.0, 41.0).unwrap();
        assert_eq!(command(&mut m, &[id], false), None, "{name}");
        assert_eq!(m.commands.units[&id].enter, Some(house));
        assert_eq!(
            m.world.game.units[&id].garrison, None,
            "entry waits for the unit phase"
        );
        m.advance_tick().unwrap();
        assert_eq!(m.world.game.units[&id].garrison, Some(house), "{name}");
        assert_eq!(m.world.game.units[&id].at(), Point { x: 41.0, z: 41.0 });
        assert_eq!(m.commands.units[&id].garrison_entry, Some(at));
        assert_eq!(m.commands.units[&id].enter, None);
        assert!(m.world.game.units[&id].path.is_empty());
    }
    let mut m = scene();
    let tank = add(&mut m, "tank", Point { x: 37.0, z: 45.0 });
    assert_eq!(command(&mut m, &[tank], false), Some(Denial::Needs));
    assert_eq!(m.commands.units[&tank].enter, None);
    let rifle = add(&mut m, "rifle", Point { x: 38.0, z: 41.0 });
    m.world.game.units.get_mut(&rifle).unwrap().retreating = true;
    assert_eq!(command(&mut m, &[rifle], false), Some(Denial::Retreating));
    assert_eq!(m.commands.units[&rifle].enter, None);
}

#[test]
fn eng33_nearest_cells_reserve_capacity_while_squads_walk_then_hold_one_squad_each() {
    let mut m = scene();
    let left = add(&mut m, "rifle", Point { x: 31.0, z: 41.0 });
    let right = add(&mut m, "mg", Point { x: 53.0, z: 41.0 });
    let extra = add(&mut m, "medic", Point { x: 37.0, z: 47.0 });
    let a = m.world.game.cell_of(41.0, 41.0).unwrap();
    let b = m.world.game.cell_of(43.0, 41.0).unwrap();
    assert_eq!(command(&mut m, &[left, right], false), None);
    assert_eq!(m.commands.units[&left].enter, Some(a));
    assert_eq!(m.commands.units[&right].enter, Some(b));
    assert!(!m.world.game.units[&left].path.is_empty());
    assert!(!m.world.game.units[&right].path.is_empty());
    assert_eq!(
        command(&mut m, &[extra], false),
        Some(Denial::Maximum),
        "pending entries reserve cells"
    );
    m.advance_tick().unwrap();
    assert_eq!(
        m.world.game.units[&left].garrison, None,
        "a distant squad must approach"
    );
    assert_eq!(m.world.game.units[&right].garrison, None);
    for _ in 0..300 {
        if m.world.game.units[&left].garrison.is_some()
            && m.world.game.units[&right].garrison.is_some()
        {
            break;
        }
        m.advance_tick().unwrap();
    }
    assert_eq!(m.world.game.units[&left].garrison, Some(a));
    assert_eq!(m.world.game.units[&right].garrison, Some(b));
    assert_eq!(
        command(&mut m, &[extra], false),
        Some(Denial::Maximum),
        "occupied cells retain capacity"
    );
}

#[test]
fn eng33_only_open_house_edges_are_entries_and_a_lost_house_cancels_pending_entry() {
    let mut m = scene();
    let id = add(&mut m, "rifle", Point { x: 31.0, z: 41.0 });
    let a = m.world.game.cell_of(41.0, 41.0).unwrap();
    let b = m.world.game.cell_of(43.0, 41.0).unwrap();
    // Every external neighbour of the nearer cell blocks infantry. The connected
    // farther cell is still a valid entry, reached through the same clicked house.
    for cell in [a - 1, a - 64, a + 64] {
        m.world.set_cell(cell, b'W').unwrap();
    }
    assert_eq!(command(&mut m, &[id], false), None);
    assert_eq!(m.commands.units[&id].enter, Some(b));
    m.world.set_cell(b, b'.').unwrap();
    m.advance_tick().unwrap();
    assert_eq!(m.commands.units[&id].enter, None);
    assert_eq!(m.world.game.units[&id].garrison, None);

    let mut m = scene();
    let id = add(&mut m, "rifle", Point { x: 38.0, z: 41.0 });
    assert_eq!(command(&mut m, &[id], true), None);
    assert_eq!(m.commands.units[&id].enter, None);
    assert_eq!(m.commands.units[&id].orders.len(), 1);
    m.advance_tick().unwrap();
    assert!(m.world.game.units[&id].garrison.is_some());
    assert!(m.commands.units[&id].orders.is_empty());
}

#[test]
fn eng33_wounded_garrison_automatically_exits_and_retreats_without_faulting_the_tick() {
    let mut m = scene();
    let id = add(&mut m, "rifle", Point { x: 38.0, z: 41.0 });
    assert!(
        m.commands.units[&id].auto_retreat,
        "ordinary default remains enabled"
    );
    assert_eq!(command(&mut m, &[id], false), None);
    m.advance_tick().unwrap();
    assert!(m.world.game.units[&id].garrison.is_some());
    m.world.game.units.get_mut(&id).unwrap().hp = 1.0;
    assert!(m.automatic_retreat_candidate(id).unwrap());
    let before = m.world.game.tick;
    m.advance_tick().unwrap();
    assert_eq!(m.world.game.tick, before + 1);
    let unit = &m.world.game.units[&id];
    assert_eq!(unit.garrison, None);
    assert!(unit.retreating);
    assert_eq!(unit.world_goal, Some(m.reaction_home(id).unwrap()));
    assert!(!unit.path.is_empty());
    assert_eq!(m.world.game.flags_at(unit.x, unit.z) & MOVE, 0);
    m.advance_tick().unwrap();
    assert_eq!(
        m.world.game.tick,
        before + 2,
        "runner remains usable after the exit"
    );
}
