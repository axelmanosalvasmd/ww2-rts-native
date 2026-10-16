//! World core rules. Public server admission and the complete tick runner remain separate.
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{CELL, LOCAL_DEFENDER_OWNER, Point},
    map::AuthoritativeMap,
    match_commands::UnitControl,
    match_runtime::{BuildingCell, Match, MatchOptions, PlayerConfig},
    random::MatchRandom,
    unit_spawn,
    weather::WeatherChoice,
    world_generation::{WorldSize, generate_world},
};

fn assert_unit(actual: &ww2_core::game::Unit, expected: &ww2_core::game::Unit) {
    assert_eq!(actual.pace.to_bits(), expected.pace.to_bits());
    let (mut actual, mut expected) = (actual.clone(), expected.clone());
    actual.pace = 0.0;
    expected.pace = 0.0;
    assert_eq!(actual, expected);
}

fn options(seed: u64, teams: &[i32]) -> MatchOptions {
    MatchOptions {
        seed,
        mode: Mode::World,
        players: teams
            .iter()
            .enumerate()
            .map(|(slot, &team)| PlayerConfig {
                name: format!("Commander {slot}"),
                team,
                faction: Faction::Usa,
            })
            .collect(),
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
    }
}

fn world(size: WorldSize, teams: &[i32]) -> Match {
    let options = options(1821, teams);
    let mut random = MatchRandom::new(options.seed);
    let generated = generate_world(&mut random, size, teams, true).unwrap();
    let map = AuthoritativeMap::from_world(generated.into_world_map_input()).unwrap();
    Match::new_world(&map, options, random).unwrap()
}

fn centre(m: &Match, index: usize) -> Point {
    let region = &m.world.geography().unwrap().regions[index];
    Point {
        x: (region.x as f64 + 0.5) * CELL,
        z: (region.y as f64 + 0.5) * CELL,
    }
}

fn add(m: &mut Match, name: &str, owner: usize, at: Point) -> u32 {
    let id = m.next_unit_id;
    m.next_unit_id += 1;
    let kind = m.rules.kind(name).unwrap();
    let unit =
        unit_spawn::initial_unit(&m.rules, Catalog::compiled().unwrap(), kind, id, owner, at)
            .unwrap();
    m.commands
        .insert(id, UnitControl::new(&unit, &m.rules).unwrap());
    m.combat.initialize(id);
    m.firing.initialize(id);
    m.abilities.initialize(id);
    m.recovery.initialize(id);
    m.world.game.units.insert(id, unit);
    id
}

fn phase(m: &mut Match, dt: f64) -> ww2_core::match_world_elimination::WorldRegionEffects {
    m.grid.rebuild(&m.world.game.units);
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    m.apply_world_regions(&order, dt).unwrap()
}

#[test]
fn mode71_seeded_core_world_creation_supports_both_sizes_and_one_to_six_players() {
    for size in [WorldSize::Huge, WorldSize::Massive] {
        for count in 1..=6 {
            let teams = (0..count).map(|slot| slot as i32).collect::<Vec<_>>();
            let mut a = MatchRandom::new(1821);
            let mut b = MatchRandom::new(1821);
            let generated = generate_world(&mut a, size, &teams, true).unwrap();
            // Repeat one full roster per size; every player count still constructs below.
            if count == 6 {
                assert_eq!(
                    generated,
                    generate_world(&mut b, size, &teams, true).unwrap()
                );
                assert_eq!(a.snapshot(), b.snapshot());
            }
            let map = AuthoritativeMap::from_world(generated.into_world_map_input()).unwrap();
            let m = Match::new_world(&map, options(1821, &teams), a).unwrap();
            assert_eq!(
                m.world_runtime.as_ref().unwrap().regions.len(),
                size.region_count()
            );
            assert_eq!(m.world.game.players.len(), count);
            for (slot, &team) in teams.iter().enumerate() {
                let home = m
                    .world
                    .geography()
                    .unwrap()
                    .regions
                    .iter()
                    .position(|r| r.home == Some(slot))
                    .unwrap();
                assert_eq!(m.world_runtime.as_ref().unwrap().regions[home].team, team);
                assert_eq!(
                    m.world_region_index(m.start.players[slot].spawn).unwrap(),
                    Some(home)
                );
                let units = m
                    .world
                    .game
                    .units
                    .values()
                    .filter(|u| u.owner == slot)
                    .collect::<Vec<_>>();
                for name in ["hq", "engineer", "rifle"] {
                    assert_eq!(
                        units
                            .iter()
                            .filter(|u| m.rules.kinds[u.kind].name == name)
                            .count(),
                        1
                    );
                }
                let hq = units
                    .iter()
                    .find(|u| m.rules.kinds[u.kind].name == "hq")
                    .unwrap();
                assert_eq!(m.world_region_index(hq.at()).unwrap(), Some(home));
                assert_eq!(hq.at(), m.start.players[slot].spawn);
            }
        }
        let mut a = MatchRandom::new(1821);
        let mut b = MatchRandom::new(1822);
        assert_ne!(
            generate_world(&mut a, size, &[0, 1], false)
                .unwrap()
                .geography,
            generate_world(&mut b, size, &[0, 1], false)
                .unwrap()
                .geography
        );
    }
}

#[test]
fn mode74_region_capture_contests_locks_drains_and_transfers_all_four_forts() {
    let mut m = world(WorldSize::Huge, &[0, 1, 0]);
    let region = m
        .world_runtime
        .as_ref()
        .unwrap()
        .regions
        .iter()
        .position(|r| r.team < 0)
        .unwrap();
    let at = centre(&m, region);
    let capturer = add(&mut m, "rifle", 0, at);
    // The actual constructor guard base locks the neutral region.
    assert!(phase(&mut m, 8.0).captures.is_empty());
    assert_eq!(
        m.world_runtime.as_ref().unwrap().regions[region].locked,
        Some(true)
    );
    for unit in m
        .world
        .game
        .units
        .values_mut()
        .filter(|u| u.owner == LOCAL_DEFENDER_OWNER)
    {
        unit.hp = 0.0;
    }
    let rival = add(
        &mut m,
        "rifle",
        1,
        Point {
            x: at.x + 8.01,
            z: at.z,
        },
    );
    phase(&mut m, 0.0);
    assert!(!m.world_runtime.as_ref().unwrap().regions[region].contested);
    m.world.game.units.get_mut(&rival).unwrap().x = at.x + 8.0;
    assert!(phase(&mut m, 8.0).captures.is_empty());
    assert!(m.world_runtime.as_ref().unwrap().regions[region].contested);
    m.world.game.units.get_mut(&rival).unwrap().retreating = true;
    let mut forts = Vec::new();
    let mut sites = m
        .world
        .geography()
        .unwrap()
        .region_map
        .as_ref()
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, r)| **r == region as i16)
        .map(|(cell, _)| cell)
        .collect::<Vec<_>>();
    sites.sort_by(|&a, &b| {
        let distance = |cell: usize| {
            let x = (cell % m.world.game.w) as f64 * CELL + CELL / 2.0;
            let z = (cell / m.world.game.w) as f64 * CELL + CELL / 2.0;
            ww2_core::math::hypot(x - at.x, z - at.z)
        };
        distance(a).total_cmp(&distance(b)).then(a.cmp(&b))
    });
    for (offset, name) in ["pillbox", "tower", "wall", "gate"].into_iter().enumerate() {
        let cell = sites[offset];
        let pose = Point {
            x: (cell % m.world.game.w) as f64 * CELL + CELL / 2.0,
            z: (cell / m.world.game.w) as f64 * CELL + CELL / 2.0,
        };
        let id = add(&mut m, name, 1, pose);
        let unit = m.world.game.units.get_mut(&id).unwrap();
        unit.hp = 73.0;
        unit.built = Some(0.7);
        unit.cells = Some(vec![cell as u32]);
        m.building_cells.insert(
            cell,
            BuildingCell {
                unit: id,
                owner: 1,
                at: unit.at(),
            },
        );
        forts.push((id, cell, unit.clone()));
    }
    let ally = add(&mut m, "wall", 2, at);
    let dead = add(&mut m, "wall", 1, at);
    m.world.game.units.get_mut(&dead).unwrap().hp = 0.0;
    let enemy_spawn = m.start.players[1].spawn;
    let outside = add(&mut m, "tower", 1, enemy_spawn);
    // An owning team's production building also locks an enemy-held region.
    m.world_runtime.as_mut().unwrap().regions[region].team = 1;
    m.world_runtime.as_mut().unwrap().regions[region].progress = 1.0;
    let producer = add(&mut m, "barracks", 1, at);
    assert!(phase(&mut m, 8.0).captures.is_empty());
    assert_eq!(m.world_runtime.as_ref().unwrap().regions[region].team, 1);
    m.world.game.units.get_mut(&producer).unwrap().hp = 0.0;
    assert!(phase(&mut m, 8.0).captures.is_empty());
    assert_eq!(m.world_runtime.as_ref().unwrap().regions[region].team, -1);
    assert_eq!(
        m.world_runtime.as_ref().unwrap().regions[region].progress,
        0.0
    );
    let story = m.story[0].captures;
    assert!(phase(&mut m, 4.0).captures.is_empty());
    assert_eq!(
        m.world_runtime.as_ref().unwrap().regions[region].progress,
        0.5
    );
    // A late story failure must not publish the prepared fort or region changes.
    m.story[0].captures = f64::NAN;
    m.grid.rebuild(&m.world.game.units);
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    assert!(m.apply_world_regions(&order, 4.0).is_err());
    assert_eq!(
        m.world_runtime.as_ref().unwrap().regions[region].progress,
        0.5
    );
    for (id, cell, before) in &forts {
        assert_unit(&m.world.game.units[id], before);
        assert_eq!(m.building_cells[cell].owner, 1);
    }
    m.story[0].captures = story;
    let report = phase(&mut m, 4.0);
    assert_eq!(report.captures.len(), 1);
    assert_eq!(
        report.captures[0].owner,
        m.world.game.units[&capturer].owner
    );
    assert_eq!(m.story[0].captures, story + 1.0);
    for (id, cell, mut before) in forts {
        before.owner = 0;
        assert_unit(&m.world.game.units[&id], &before);
        assert_eq!(m.building_cells[&cell].owner, 0);
        assert!(m.grid.owned_by(&m.world.game.units, 0).contains(&id));
        assert!(!m.grid.owned_by(&m.world.game.units, 1).contains(&id));
    }
    assert_eq!(m.world.game.units[&ally].owner, 2);
    assert_eq!(m.world.game.units[&dead].owner, 1);
    assert_eq!(m.world.game.units[&outside].owner, 1);
}

#[test]
fn mode74_forts_seized_before_former_owner_elimination_survive_the_same_phase() {
    let mut m = world(WorldSize::Huge, &[0, 1]);
    let home = m
        .world_region_index(m.start.players[1].spawn)
        .unwrap()
        .unwrap();
    let at = centre(&m, home);
    let capturer = add(&mut m, "rifle", 0, at);
    let site = m
        .world
        .geography()
        .unwrap()
        .region_map
        .as_ref()
        .unwrap()
        .iter()
        .enumerate()
        .find(|(cell, region)| {
            **region == home as i16
                && m.world.game.chars[*cell] == b'.'
                && ww2_core::math::hypot(
                    (*cell % m.world.game.w) as f64 * CELL - at.x,
                    (*cell / m.world.game.w) as f64 * CELL - at.z,
                ) > 40.0
        })
        .unwrap()
        .0;
    let pose = Point {
        x: (site % m.world.game.w) as f64 * CELL + CELL / 2.0,
        z: (site / m.world.game.w) as f64 * CELL + CELL / 2.0,
    };
    let fort = add(&mut m, "gate", 1, pose);
    m.world.game.units.get_mut(&fort).unwrap().cells = Some(vec![site as u32]);
    m.building_cells.insert(
        site,
        BuildingCell {
            unit: fort,
            owner: 1,
            at: pose,
        },
    );
    for unit in m
        .world
        .game
        .units
        .values_mut()
        .filter(|u| u.owner == 1 && u.id != fort)
    {
        unit.hp = 0.0;
    }
    let region = &mut m.world_runtime.as_mut().unwrap().regions[home];
    region.team = -1;
    region.progress = 0.5;
    region.capper = Some(0);
    let report = phase(&mut m, 4.0);
    assert_eq!(report.captures.len(), 1);
    assert!(m.start.economy.players[1].out);
    assert_eq!(m.world.game.units[&fort].owner, 0);
    assert_eq!(m.building_cells[&site].owner, 0);
    assert!(report.removed.iter().all(|u| u.id != fort));
    assert!(report.removed.iter().any(|u| u.owner == 1));
    assert!(m.world.game.units.contains_key(&capturer));
    assert!(m.world.game.units.values().all(|u| u.owner != 1));
    assert_eq!(m.finish.unwrap().winner, 0);
}

#[test]
fn mode75_region_loss_rehomes_allies_and_deletes_armies_without_hq_defeat_or_bounty() {
    let mut m = world(WorldSize::Huge, &[0, 1, 0]);
    for unit in m.world.game.units.values_mut() {
        if unit.owner != LOCAL_DEFENDER_OWNER
            && m.rules.kinds[unit.kind].name == "hq"
            && unit.owner != 1
        {
            unit.hp = 0.0;
        }
    }
    phase(&mut m, 0.0);
    assert!(!m.start.economy.players[0].out && !m.start.economy.players[2].out);
    let enemy_home = m.start.players[1].spawn;
    m.start.players[0].spawn = enemy_home;
    let expected_home = m
        .world_runtime
        .as_ref()
        .unwrap()
        .regions
        .iter()
        .enumerate()
        .filter(|(_, r)| r.team == 0)
        .map(|(index, _)| centre(&m, index))
        .min_by(|a, b| {
            ww2_core::math::hypot(a.x - enemy_home.x, a.z - enemy_home.z).total_cmp(
                &ww2_core::math::hypot(b.x - enemy_home.x, b.z - enemy_home.z),
            )
        })
        .unwrap();
    for region in &mut m.world_runtime.as_mut().unwrap().regions {
        if region.team == 1 {
            region.team = -1;
            region.progress = 0.0;
        }
    }
    let removed = m
        .world
        .game
        .units
        .values()
        .filter(|u| u.owner == 1)
        .map(|u| u.id)
        .collect::<Vec<_>>();
    let hq = removed
        .iter()
        .copied()
        .find(|id| m.rules.kinds[m.world.game.units[id].kind].name == "hq")
        .unwrap();
    let cells = m.world.game.units[&hq].cells.clone().unwrap();
    let story = m.story.clone();
    let balances = m
        .start
        .economy
        .players
        .iter()
        .map(|p| (p.mp, p.mun, p.fuel))
        .collect::<Vec<_>>();
    let report = phase(&mut m, 0.0);
    assert!(m.start.economy.players[1].out);
    assert_eq!(
        report.removed.iter().map(|u| u.id).collect::<Vec<_>>(),
        removed
    );
    assert!(
        removed
            .iter()
            .all(|id| !m.world.game.units.contains_key(id))
    );
    assert!(removed.iter().all(|id| !m.commands.units.contains_key(id)));
    assert!(
        removed
            .iter()
            .all(|id| m.production.building(*id).is_none())
    );
    assert!(
        cells
            .iter()
            .all(|&cell| m.world.game.chars[cell as usize] != b'K')
    );
    assert_eq!(m.start.players[0].spawn, expected_home);
    assert!(!m.start.economy.players[0].out && !m.start.economy.players[2].out);
    assert_eq!(m.story, story);
    assert_eq!(
        m.start
            .economy
            .players
            .iter()
            .map(|p| (p.mp, p.mun, p.fuel))
            .collect::<Vec<_>>(),
        balances
    );
    assert!(
        m.world
            .game
            .units
            .values()
            .any(|u| u.owner == LOCAL_DEFENDER_OWNER)
    );
}

#[test]
fn mode77_core_metadata_carries_home_goal_cap_regions_stations_and_recovery() {
    for size in [WorldSize::Huge, WorldSize::Massive] {
        let mut m = world(size, &[0, 1, 0]);
        let initial = m.world_recipient_metadata(0, false).unwrap().world;
        assert_eq!(initial.size, size);
        assert_eq!(initial.total, size.region_count());
        assert_eq!(
            initial.goal,
            (size.region_count() as f64 * 0.6).ceil() as usize
        );
        assert_eq!(initial.owned, 2);
        assert!(initial.cap > 0.0);
        assert!(!initial.regions.is_empty());
        assert!(initial.regions.iter().all(|r| r.team != 1));
        assert!(initial.stations.is_some());
        let runtime = m.world_runtime.as_ref().unwrap();
        let memory = runtime.memory.team(0).unwrap();
        let stations = runtime
            .rails
            .stations
            .iter()
            .filter(|s| memory.contains_cell(s.cell))
            .count();
        assert_eq!(initial.stations.as_ref().unwrap().len(), stations);
        assert!(!initial.recovery.available);
        assert!(initial.recovery.hq > 0.0 && initial.recovery.engineer > 0.0);
        let gained = m
            .world_runtime
            .as_ref()
            .unwrap()
            .regions
            .iter()
            .position(|r| r.team < 0)
            .unwrap();
        m.world_runtime.as_mut().unwrap().regions[gained].team = 0;
        let expanded = m.world_recipient_metadata(0, false).unwrap().world;
        assert_eq!(expanded.owned, initial.owned + 1);
        assert!(expanded.cap > initial.cap);
        let hq = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "hq")
            .unwrap()
            .id;
        m.world.game.units.get_mut(&hq).unwrap().hp = 0.0;
        assert!(
            m.world_recipient_metadata(0, false)
                .unwrap()
                .world
                .recovery
                .available
        );
        m.world.game.units.get_mut(&hq).unwrap().hp = 500.0;
        let engineer = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "engineer")
            .unwrap()
            .id;
        m.world.game.units.get_mut(&engineer).unwrap().hp = 0.0;
        assert!(
            m.world_recipient_metadata(0, false)
                .unwrap()
                .world
                .recovery
                .available
        );
        m.start.economy.players[0].out = true;
        assert!(
            !m.world_recipient_metadata(0, false)
                .unwrap()
                .world
                .recovery
                .available
        );
        m.start.economy.players[0].out = false;
        m.world.game.reveal = true;
        let full = m.world_recipient_metadata(0, false).unwrap().world;
        assert_eq!(full.regions.len(), size.region_count());
        assert!(full.regions.iter().any(|r| r.team == 1));
        assert_eq!(full.home, initial.home);
    }
    let mut m = world(WorldSize::Huge, &[3, 3]);
    let panel = m.world_recipient_metadata(0, false).unwrap().world;
    assert_eq!(panel.goal, panel.total);
    assert_eq!(panel.owned, 2);
}

#[test]
fn mode36_world_out_player_commands_income_and_kill_bounties_are_blocked() {
    use serde_json::json;
    use ww2_core::{
        match_commands::{Command, Denial},
        match_world_phases::WorldIncomeUnit,
    };
    let mut m = world(WorldSize::Huge, &[0, 1]);
    let id = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    m.start.economy.players[0].out = true;
    let before = m.start.economy.players[0].clone();
    for raw in [
        json!({"t":"buy","unit":"rifle"}),
        json!({"t":"tech","kind":"tier"}),
        json!({"t":"move","orders":[[id,20,20]]}),
        json!({"t":"amove","orders":[[id,20,20]]}),
        json!({"t":"stop","ids":[id]}),
        json!({"t":"cover","ids":[id]}),
        json!({"t":"attack","ids":[id],"target":2}),
        json!({"t":"escort","ids":[id],"target":2}),
        json!({"t":"retreat","ids":[id]}),
        json!({"t":"ability","ids":[id],"x":20,"z":20}),
        json!({"t":"stance","ids":[id],"key":"holdPos","on":true}),
        json!({"t":"autocast","ids":[id],"on":true}),
        json!({"t":"support","kind":"para","x":20,"z":20}),
        json!({"t":"board","ids":[id],"target":2}),
        json!({"t":"unload","ids":[id]}),
        json!({"t":"build","ids":[id],"kind":"hq","x":20,"z":20}),
        json!({"t":"assist","ids":[id],"id":2}),
        json!({"t":"destroy","ids":[id]}),
        json!({"t":"dig","ids":[id],"kind":"trench","x":20,"z":20}),
        json!({"t":"entrench","ids":[id],"fort":"trench","x":20,"z":20,"x2":24,"z2":20}),
    ] {
        let command = Command::from_json(&raw).expect("authored valid command");
        assert!(!matches!(command, Command::Unsupported), "{raw}");
        assert_eq!(
            m.execute_command(0, &command, false).unwrap(),
            Some(Denial::Blocked),
            "{raw}"
        );
    }
    let captured = m
        .world
        .game
        .units
        .values()
        .map(WorldIncomeUnit::from)
        .collect::<Vec<_>>();
    m.apply_world_income(&captured, 1.0).unwrap();
    assert_eq!(m.start.economy.players[0].mp, before.mp);
    assert_eq!(m.start.economy.players[0].mun, before.mun);
    assert_eq!(m.start.economy.players[0].fuel, before.fuel);
    assert_eq!(m.start.economy.players[0].inc, 0.0);
    let at = m.start.players[1].spawn;
    let victim = add(&mut m, "rifle", 1, at);
    m.world.game.units.get_mut(&victim).unwrap().hp = 0.0;
    m.combat.get_mut(victim).unwrap().last_hit = Some(0);
    m.grid.rebuild(&m.world.game.units);
    m.retire_dead(&[victim]).unwrap();
    assert_eq!(m.start.economy.players[0].mp, before.mp);
}
