use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::LOCAL_DEFENDER_OWNER,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    random::MatchRandom,
    weather::WeatherChoice,
    world_generation::{WorldSize, generate_world},
};

fn scene() -> Match {
    let options = MatchOptions {
        seed: 1821,
        mode: Mode::World,
        players: vec![
            PlayerConfig {
                name: "a".into(),
                team: 0,
                faction: Faction::Usa,
            },
            PlayerConfig {
                name: "b".into(),
                team: 1,
                faction: Faction::Germany,
            },
        ],
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: None,
        living_weather: false,
        map_key: None,
        tech: false,
        supply: false,
        logistics: false,
        sandbox: false,
    };
    let mut random = MatchRandom::new(options.seed);
    let mut input = generate_world(&mut random, WorldSize::Huge, &[0, 1], false)
        .unwrap()
        .into_world_map_input();
    input.chars.fill(b'.');
    input.heights.as_mut().unwrap().fill(0);
    input.layers = None;
    for region in &mut input.geography.regions {
        region.kind = Some("rural".into());
    }
    let map = AuthoritativeMap::from_world(input).unwrap();
    Match::new_world(&map, options, random).unwrap()
}

fn ranked_guards(m: &Match) -> Vec<usize> {
    let geography = m.world.geography().unwrap();
    let mut rows = geography
        .regions
        .iter()
        .enumerate()
        .filter(|(index, _)| m.world_runtime.as_ref().unwrap().regions[*index].team < 0)
        .map(|(index, region)| {
            let at = Point {
                x: (region.x as f64 + 0.5) * CELL,
                z: (region.y as f64 + 0.5) * CELL,
            };
            let nearest = m
                .start
                .players
                .iter()
                .map(|p| math::hypot(p.spawn.x - at.x, p.spawn.z - at.z))
                .fold(f64::INFINITY, f64::min);
            (index, nearest)
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    rows.into_iter().map(|(index, _)| index).collect()
}

fn fort_ids(m: &Match, region: usize) -> Vec<u32> {
    m.world
        .game
        .units
        .values()
        .filter(|unit| {
            unit.hp > 0.0
                && ["pillbox", "tower", "wall", "gate"]
                    .contains(&m.rules.kinds[unit.kind].name.as_str())
                && m.world_region_index(unit.at()).unwrap() == Some(region)
        })
        .map(|unit| unit.id)
        .collect()
}

fn capture_scene() -> (Match, usize, Vec<u32>) {
    let mut m = scene();
    let region = ranked_guards(&m)
        .into_iter()
        .find(|&i| !fort_ids(&m, i).is_empty())
        .unwrap();
    let base = *m
        .world_runtime
        .as_ref()
        .unwrap()
        .guard_bases
        .iter()
        .find(|entry| *entry.1 == region)
        .unwrap()
        .0;
    let removed = m
        .world
        .game
        .units
        .values()
        .filter(|unit| {
            unit.id == base
                || (unit.owner == LOCAL_DEFENDER_OWNER
                    && m.rules.kinds[unit.kind].infantry
                    && m.world_region_index(unit.at()).unwrap() == Some(region))
        })
        .map(|unit| unit.id)
        .collect::<Vec<_>>();
    for id in &removed {
        m.world.game.units.get_mut(id).unwrap().hp = 0.0;
    }
    m.retire_dead(&removed).unwrap();
    assert!(
        removed
            .iter()
            .all(|id| !m.world.game.units.contains_key(id))
    );
    let at = {
        let r = &m.world.geography().unwrap().regions[region];
        Point {
            x: (r.x as f64 + 0.5) * CELL,
            z: (r.y as f64 + 0.5) * CELL,
        }
    };
    let rifle = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let u = m.world.game.units.get_mut(&rifle).unwrap();
    u.x = at.x;
    u.z = at.z;
    m.grid.rebuild(&m.world.game.units);
    let state = &mut m.world_runtime.as_mut().unwrap().regions[region];
    state.capper = Some(0);
    state.progress = 1.0;
    let forts = fort_ids(&m, region);
    (m, region, forts)
}

#[test]
fn base54_world_fort_ranks_are_confined_to_source_shares() {
    let m = scene();
    let ranks = ranked_guards(&m);
    let pillbox_limit = (ranks.len() as f64 * 0.4).ceil() as usize;
    let strong_limit = (ranks.len() as f64 * 0.15).ceil() as usize;
    let mut pillbox_sites = Vec::new();
    let mut tower_sites = Vec::new();
    let mut wall_sites = Vec::new();
    for (rank, &region) in ranks.iter().enumerate() {
        let mut has_box = false;
        let mut has_tower = false;
        let mut has_wall = false;
        let mut counts = [0; 3];
        let definition = &m.world.geography().unwrap().regions[region];
        let at = Point {
            x: (definition.x as f64 + 0.5) * CELL,
            z: (definition.y as f64 + 0.5) * CELL,
        };
        let home = m
            .start
            .players
            .iter()
            .map(|p| p.spawn)
            .min_by(|a, b| {
                math::hypot(a.x - at.x, a.z - at.z).total_cmp(&math::hypot(b.x - at.x, b.z - at.z))
            })
            .unwrap();
        let face = math::atan2(home.z - at.z, home.x - at.x);
        let (fx, fz) = (math::cos(face), math::sin(face));
        let forts = fort_ids(&m, region);
        let pillbox = forts
            .iter()
            .map(|id| &m.world.game.units[id])
            .find(|u| m.rules.kinds[u.kind].name == "pillbox");
        let mut wall_sides = [0; 2];
        for id in forts {
            let fort = &m.world.game.units[&id];
            assert_eq!(fort.built, Some(1.0));
            match m.rules.kinds[fort.kind].name.as_str() {
                "pillbox" => {
                    assert!(rank < pillbox_limit);
                    has_box = true;
                    counts[0] += 1;
                    assert_eq!(fort.rot.to_bits(), face.to_bits());
                    let target = Point {
                        x: at.x + fx * 14.0,
                        z: at.z + fz * 14.0,
                    };
                    assert!(
                        math::hypot(fort.x - target.x, fort.z - target.z) < 2.0 * CELL,
                        "Pillbox must be near the 14 m target toward its nearest home"
                    );
                }
                "tower" => {
                    assert!(rank < strong_limit);
                    has_tower = true;
                    counts[1] += 1;
                }
                "wall" => {
                    assert!(rank < strong_limit);
                    has_wall = true;
                    counts[2] += 1;
                    let box_at = pillbox.unwrap().at();
                    let side = (fort.x - box_at.x) * -fz + (fort.z - box_at.z) * fx;
                    assert!(side.abs() > CELL);
                    wall_sides[usize::from(side > 0.0)] += 1;
                    assert_eq!(
                        fort.rot.to_bits(),
                        (face + std::f64::consts::FRAC_PI_2).to_bits()
                    );
                }
                _ => unreachable!(),
            }
        }
        assert_eq!(
            counts,
            [
                usize::from(rank < pillbox_limit),
                usize::from(rank < strong_limit),
                if rank < strong_limit { 8 } else { 0 }
            ],
            "fort population at rank {rank}"
        );
        assert_eq!(
            wall_sides,
            if rank < strong_limit { [4, 4] } else { [0, 0] }
        );
        if has_box {
            pillbox_sites.push(rank);
        }
        if has_tower {
            tower_sites.push(rank);
        }
        if has_wall {
            wall_sites.push(rank);
        }
    }
    assert!(pillbox_limit > strong_limit);
    assert_eq!(pillbox_sites, (0..pillbox_limit).collect::<Vec<_>>());
    assert_eq!(tower_sites, (0..strong_limit).collect::<Vec<_>>());
    assert_eq!(wall_sites, (0..strong_limit).collect::<Vec<_>>());
}

#[test]
fn base54_fort_capture_refusal_keeps_elimination_atomic() {
    let (mut m, region, forts) = capture_scene();
    let id = forts[0];
    m.world.game.units.get_mut(&id).unwrap().hp *= 0.5;
    let blocker = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 1 && m.rules.kinds[u.kind].building)
        .unwrap()
        .id;
    m.world.game.units.get_mut(&blocker).unwrap().build = true;
    for state in &mut m.world_runtime.as_mut().unwrap().regions {
        if state.team == 1 {
            state.team = -1;
            state.progress = 0.0;
            state.capper = None;
        }
    }
    let units = m.world.game.units.clone();
    let regions = m.world_runtime.as_ref().unwrap().regions.clone();
    let wallets = m.start.economy.clone();
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    assert!(
        m.apply_world_regions(&order, 0.05)
            .unwrap_err()
            .contains("fort")
    );
    assert_eq!(
        m.world.game.units.keys().collect::<Vec<_>>(),
        units.keys().collect::<Vec<_>>()
    );
    // Canonical units contain NaN pace values, so structural PartialEq is unsuitable.
    for (id, before) in units {
        let after = &m.world.game.units[&id];
        assert_eq!(format!("{after:?}"), format!("{before:?}"), "unit {id}");
        assert_eq!(
            after.pace.to_bits(),
            before.pace.to_bits(),
            "unit {id} pace"
        );
    }
    assert_eq!(m.world_runtime.as_ref().unwrap().regions, regions);
    assert_eq!(m.start.economy, wallets);
    for &cell in m.world.game.units[&id].cells.as_ref().unwrap() {
        assert_eq!(
            m.building_cells[&(cell as usize)].owner,
            LOCAL_DEFENDER_OWNER
        );
    }
    assert_eq!(m.world_runtime.as_ref().unwrap().regions[region].team, -1);
}

fn detached_capture(
    m: &mut Match,
) -> Result<crate::match_world_elimination::WorldRegionEffects, String> {
    m.grid.rebuild(&m.world.game.units);
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    m.apply_world_regions_with_fort_capture(&order, 0.05)
}

fn add_gate(m: &mut Match, region: usize, owner: usize) -> u32 {
    let cell = m
        .world
        .geography()
        .unwrap()
        .region_map
        .as_ref()
        .unwrap()
        .iter()
        .enumerate()
        .find(|(cell, index)| {
            **index == region as i16 && world_can_stamp(&m.world.game, &m.rules, &[*cell])
        })
        .unwrap()
        .0;
    let id = m.world_building(owner, "gate", cell).unwrap();
    m.commands.units.get_mut(&id).unwrap().gate = Some(crate::match_gates::GateControl {
        open: Some(false),
        remaining: 0.35,
    });
    id
}

#[test]
fn base54_detached_capture_keeps_damage_and_resets_only_seized_gate_controls() {
    let (mut m, region, mut forts) = capture_scene();
    let gate = add_gate(&mut m, region, LOCAL_DEFENDER_OWNER);
    forts.push(gate);
    let friendly = add_gate(&mut m, region, 0);
    let dead = add_gate(&mut m, region, LOCAL_DEFENDER_OWNER);
    m.world.game.units.get_mut(&dead).unwrap().hp = 0.0;
    let outside_region = ranked_guards(&m)
        .into_iter()
        .find(|&r| r != region)
        .unwrap();
    let outside = add_gate(&mut m, outside_region, LOCAL_DEFENDER_OWNER);
    let untouched = [friendly, dead, outside].map(|id| {
        (
            id,
            format!("{:?}", m.world.game.units[&id]),
            m.commands.units[&id].gate,
        )
    });
    let before = forts
        .iter()
        .map(|id| {
            let unit = m.world.game.units.get_mut(id).unwrap();
            unit.hp *= 0.37;
            (
                *id,
                unit.hp,
                unit.built,
                unit.cells.clone().unwrap(),
                unit.at(),
            )
        })
        .collect::<Vec<_>>();
    let effects = detached_capture(&mut m).unwrap();
    assert_eq!(
        effects.captures,
        vec![crate::match_world_regions::WorldCapture {
            region_index: region,
            owner: 0
        }]
    );
    for (id, hp, built, cells, at) in before {
        let unit = &m.world.game.units[&id];
        assert_eq!(
            (unit.owner, unit.hp, unit.built, unit.at()),
            (0, hp, built, at)
        );
        for cell in cells {
            assert_eq!(m.building_cells[&(cell as usize)].owner, 0);
            assert_eq!(m.building_cells[&(cell as usize)].unit, id);
        }
        assert!(m.grid.owned_by(&m.world.game.units, 0).contains(&id));
        assert!(
            !m.grid
                .owned_by(&m.world.game.units, LOCAL_DEFENDER_OWNER)
                .contains(&id)
        );
    }
    assert_eq!(
        m.commands.units[&gate].gate,
        Some(crate::match_gates::GateControl::default())
    );
    for (id, unit, gate) in untouched {
        assert_eq!(format!("{:?}", m.world.game.units[&id]), unit);
        assert_eq!(m.commands.units[&id].gate, gate);
    }
    // Repeating the detached phase preserves the newly owned assets and their damage.
    assert!(detached_capture(&mut m).unwrap().captures.is_empty());
}

#[test]
fn base54_detached_capture_survives_previous_nation_elimination() {
    let (mut m, region, mut forts) = capture_scene();
    forts.push(add_gate(&mut m, region, 1));
    for id in &forts {
        let unit = m.world.game.units.get_mut(id).unwrap();
        unit.owner = 1;
        unit.hp *= 0.5;
        for &cell in unit.cells.as_ref().unwrap() {
            m.building_cells.get_mut(&(cell as usize)).unwrap().owner = 1;
        }
    }
    for state in &mut m.world_runtime.as_mut().unwrap().regions {
        if state.team == 1 {
            state.team = -1;
            state.progress = 0.0;
            state.capper = None;
        }
    }
    let before = forts
        .iter()
        .map(|id| (*id, m.world.game.units[id].hp))
        .collect::<Vec<_>>();
    let effects = detached_capture(&mut m).unwrap();
    assert!(m.start.economy.players[1].out);
    assert!(!effects.removed.is_empty());
    assert!(effects.removed.iter().all(|u| !forts.contains(&u.id)));
    for (id, hp) in before {
        assert_eq!(
            (m.world.game.units[&id].owner, m.world.game.units[&id].hp),
            (0, hp)
        );
    }
    assert!(m.world.game.units.values().all(|u| u.owner != 1));
    assert_eq!(m.finish.unwrap().winner, 0);
    assert!(detached_capture(&mut m).unwrap().captures.is_empty());
}

#[test]
fn base54_detached_capture_leaves_replacement_footprint_rows_unchanged() {
    let (mut m, _, forts) = capture_scene();
    let fort = forts[0];
    let cells = m.world.game.units[&fort].cells.clone().unwrap();
    let cell = cells[0] as usize;
    let replacement = BuildingCell {
        unit: u32::MAX,
        owner: 1,
        at: Point { x: 9.0, z: 11.0 },
    };
    m.building_cells.insert(cell, replacement);
    detached_capture(&mut m).unwrap();
    assert_eq!(m.world.game.units[&fort].owner, 0);
    assert_eq!(m.building_cells[&cell], replacement);
    for cell in cells.into_iter().skip(1) {
        assert_eq!(m.building_cells[&(cell as usize)].owner, 0);
    }
}

#[test]
fn base54_detached_capture_failure_keeps_gate_owner_and_footprint_atomic() {
    let (mut m, region, _) = capture_scene();
    let gate = add_gate(&mut m, region, LOCAL_DEFENDER_OWNER);
    m.story[0].captures = f64::NAN;
    let units = format!("{:?}", m.world.game.units);
    let footprints = m.building_cells.clone();
    let control = m.commands.units[&gate].gate;
    let regions = m.world_runtime.as_ref().unwrap().regions.clone();
    let wallets = m.start.economy.clone();
    assert!(detached_capture(&mut m).is_err());
    assert_eq!(format!("{:?}", m.world.game.units), units);
    assert_eq!(m.building_cells, footprints);
    assert_eq!(m.commands.units[&gate].gate, control);
    assert_eq!(m.world_runtime.as_ref().unwrap().regions, regions);
    assert_eq!(m.start.economy, wallets);
    m.story[0].captures = 0.0;
    detached_capture(&mut m).unwrap();
    assert_eq!(m.world.game.units[&gate].owner, 0);
    assert_eq!(
        m.commands.units[&gate].gate,
        Some(crate::match_gates::GateControl::default())
    );
}
