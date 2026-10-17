//! Behavior checks for engineering gaps, using native state and the pinned prototype's intent.
use crate::{
    catalog::Catalog,
    combat::{Impact, WeaponDamage},
    economy::{Faction, Mode},
    economy_state::ArmySize,
    environment::{Blast, Context, DamageContext, Effect, EnvironmentRuntime},
    game::{COVER, MOVE, Point, VBLOCK, WIRE},
    grid::SpatialGrid,
    map::AuthoritativeMap,
    match_combat::{CombatState, ContactTarget, DamageWorld, FlightSource, RetargetSink},
    match_commands::MatchCommands,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    navigation::{TerrainView, Versions},
    pathfinding::{PathRules, PathStatus, PathTerrain, Pathfinding},
    random::StreamId,
    rules::Rules,
    unit_spawn::{initial_aircraft, initial_unit},
    weather::{MapWeatherInfo, WeatherChoice, WeatherKind, WeatherRules},
    world::World,
};
use serde_json::json;
use std::collections::BTreeSet;

const WIDTH: usize = 40;
const BRIDGE: usize = 10 * WIDTH + 10;
const FIRE: usize = 20 * WIDTH + 24;

fn center(cell: usize) -> Point {
    Point {
        x: (cell % WIDTH) as f64 * 2. + 1.,
        z: (cell / WIDTH) as f64 * 2. + 1.,
    }
}

fn scene(living_weather: bool) -> Match {
    let mut rows = vec![vec![b'.'; WIDTH]; WIDTH];
    rows[10][10] = b'=';
    let rows: Vec<_> = rows
        .into_iter()
        .map(|row| String::from_utf8(row).unwrap())
        .collect();
    let map = AuthoritativeMap::from_value(json!({
        "name": "Engineering assertion scene", "w": WIDTH, "h": WIDTH,
        "rows": rows,
        "spawns": [{"x": 3, "y": 3}, {"x": 36, "y": 36}],
        "points": [{"id": "center", "x": 20, "y": 20}]
    }))
    .unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("p{slot}"),
                    team: slot,
                    faction: Faction::try_from(slot as u8).unwrap(),
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: if living_weather {
                WeatherChoice::Fixed(WeatherKind::Clear)
            } else {
                WeatherChoice::Map
            },
            weather_seed: Some(123),
            living_weather,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}

fn empty_scene() -> Match {
    let mut m = scene(false);
    m.world.game.units.clear();
    m.commands = MatchCommands::new(4096);
    m.combat = CombatState::default();
    m.grid = SpatialGrid::new(&m.world.game.units, 16.);
    m
}

fn add_unit(m: &mut Match, id: u32, name: &str, owner: usize, at: Point) {
    let kind = m.rules.kind(name).unwrap();
    let constructor = if m.rules.kinds[kind].air {
        initial_aircraft
    } else {
        initial_unit
    };
    let unit = constructor(&m.rules, Catalog::compiled().unwrap(), kind, id, owner, at).unwrap();
    m.combat.initialize(id);
    m.world.game.units.insert(id, unit);
    m.grid.rebuild(&m.world.game.units);
}

fn building(m: &mut Match, id: u32, name: &str) -> Vec<usize> {
    let cells = vec![
        15 * WIDTH + 15,
        15 * WIDTH + 16,
        16 * WIDTH + 15,
        16 * WIDTH + 16,
    ];
    add_unit(m, id, name, 0, Point { x: 32., z: 32. });
    let hp = m.world.game.units[&id].hp;
    m.world.game.units.get_mut(&id).unwrap().cells =
        Some(cells.iter().map(|cell| *cell as u32).collect());
    for &cell in &cells {
        m.world.set_cell(cell, b'K').unwrap();
    }
    m.world
        .register_building_structure(id, &cells, hp, hp)
        .unwrap();
    cells
}

fn environment<R>(
    m: &mut Match,
    operation: impl FnOnce(&mut EnvironmentRuntime, &mut World, &mut Context<'_>) -> R,
) -> R {
    let order: Vec<_> = m.world.game.units.keys().copied().collect();
    let mut ctx = Context {
        tick: m.world.game.tick as u64,
        seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &order,
        grid: &m.grid,
        rain: m.world.game.rain,
        wet: m.world.game.wet.unwrap_or(0.),
        wind_angle: m.start.wind.0,
        wind_strength: m.start.wind.1,
    };
    operation(&mut m.environment, &mut m.world, &mut ctx)
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

#[test]
fn intent_eng03_wire_path_cost_equals_four_extra_orthogonal_steps() {
    // The upper route takes six steps and crosses one wire cell. The lower
    // route takes ten steps. A small extra cost breaks either side of the tie.
    let rules = Rules::compiled().unwrap();
    let infantry = &rules.kinds[rules.kind("rifle").unwrap()];
    let width = 9;
    let height = 7;
    let mut flags = vec![MOVE; width * height];
    for x in 1..=7 {
        flags[2 * width + x] = 0;
        flags[4 * width + x] = 0;
    }
    flags[3 * width + 1] = 0;
    flags[3 * width + 7] = 0;
    let wire = 2 * width + 4;
    flags[wire] = WIRE;
    let hazards = vec![false; flags.len()];
    for (penalty_cell, detours) in [(wire, true), (4 * width + 4, false)] {
        let mut near = vec![0; flags.len()];
        near[penalty_cell] = 1;
        let mut costs = PathRules::compiled().unwrap();
        costs.wall_hug = 0.001;
        let result = Pathfinding::new()
            .find_path(
                PathTerrain {
                    navigation: TerrainView {
                        width,
                        height,
                        flags: &flags,
                        heights: None,
                        world_known: false,
                        knowledge_id: 1,
                        versions: Versions::default(),
                        changed: None,
                        changes: &[],
                    },
                    near_walls: &near,
                    known_mines: &hazards,
                    fires: &hazards,
                    bunkers: &[],
                    wear: None,
                    wetness: 0.,
                    roads: false,
                },
                costs,
                infantry,
                Point { x: 3., z: 5. },
                Point { x: 15., z: 5. },
                None,
            )
            .unwrap();
        assert_eq!(result.status, PathStatus::Found);
        assert_eq!(result.waypoints.iter().any(|point| point.z > 7.), detours);
    }
}

#[test]
fn intent_eng28_trench_cover_settles_only_against_its_front() {
    let mut m = empty_scene();
    let cell = 20 * WIDTH + 20;
    m.world.set_cell(cell, b'T').unwrap();
    m.world.game.trench_front = Some(vec![f32::NAN; WIDTH * WIDTH]);
    m.world.game.trench_front.as_mut().unwrap()[cell] = 0.;
    add_unit(&mut m, 901, "rifle", 0, center(cell));
    let mut squad = m.world.game.units[&901].clone();
    let front = Point {
        x: squad.x + 10.,
        z: squad.z,
    };
    for (still, expected) in [(0., 0.5), (4., 0.4), (8., 0.3), (20., 0.3)] {
        squad.still = Some(still);
        close(m.cover_mul(&squad, front).unwrap(), expected);
    }
    for from in [
        Point {
            x: squad.x,
            z: squad.z + 10.,
        },
        Point {
            x: squad.x - 10.,
            z: squad.z,
        },
    ] {
        close(m.cover_mul(&squad, from).unwrap(), 0.5);
    }
    for (cosine, expected) in [(0.299, 0.5), (0.301, 0.3)] {
        let from = Point {
            x: squad.x + cosine * 10.,
            z: squad.z + crate::math::pow(1. - cosine * cosine, 0.5) * 10.,
        };
        close(m.cover_mul(&squad, from).unwrap(), expected);
    }
    m.world.game.trench_front.as_mut().unwrap()[cell] = f32::NAN;
    close(
        m.cover_mul(
            &squad,
            Point {
                x: squad.x - 10.,
                z: squad.z,
            },
        )
        .unwrap(),
        0.3,
    );
}

#[test]
fn intent_eng38_building_loss_reaches_nearest_sections_without_second_debit() {
    for (origin, first, second) in [
        (Point { x: 20., z: 31. }, 0, 2),
        (Point { x: 44., z: 33. }, 3, 1),
    ] {
        let mut m = empty_scene();
        let cells = building(&mut m, 901, "hq");
        for &cell in &cells {
            let meta = &m.world.sections[&cell];
            assert_eq!(meta.material, "wood");
            assert!(meta.anchor);
            close(m.world.game.sections[&(cell as u32)].hp, 750.);
        }
        let unit = m.world.game.units.get_mut(&901).unwrap();
        unit.hp -= 900.;
        unit.hit_from = Some(origin);
        environment(&mut m, |runtime, world, ctx| {
            runtime
                .sync_building_sections(world, 901, Some(1), ctx)
                .unwrap();
        });
        close(m.world.game.units[&901].hp, 2100.);
        assert!(m.world.game.sections[&(cells[first] as u32)].failed);
        close(m.world.game.sections[&(cells[second] as u32)].hp, 600.);
        for (index, &cell) in cells.iter().enumerate() {
            if index != first && index != second {
                close(m.world.game.sections[&(cell as u32)].hp, 750.);
            }
        }
        close(
            cells
                .iter()
                .map(|cell| m.world.game.sections[&(*cell as u32)].hp)
                .sum(),
            2100.,
        );
        let before: Vec<_> = cells
            .iter()
            .map(|cell| m.world.game.sections[&(*cell as u32)].hp)
            .collect();
        environment(&mut m, |runtime, world, ctx| {
            assert!(
                runtime
                    .sync_building_sections(world, 901, Some(1), ctx)
                    .unwrap()
                    .is_empty()
            );
        });
        assert_eq!(
            before,
            cells
                .iter()
                .map(|cell| m.world.game.sections[&(*cell as u32)].hp)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn if25_building_loss_equal_distance_keeps_authored_section_order() {
    for reverse in [false, true] {
        let mut m = empty_scene();
        let mut cells = building(&mut m, 901, "hq");
        if reverse {
            let structure = m
                .world
                .structures
                .iter_mut()
                .find(|s| s.entity_id == Some(901))
                .unwrap();
            structure.cells.reverse();
            structure.section_ids.reverse();
            cells.reverse();
        }
        // All four section centers are equally far from the building center.
        let unit = m.world.game.units.get_mut(&901).unwrap();
        unit.hp -= 100.0;
        unit.hit_from = Some(Point { x: 32.0, z: 32.0 });
        environment(&mut m, |runtime, world, ctx| {
            runtime
                .sync_building_sections(world, 901, Some(1), ctx)
                .unwrap();
        });
        close(m.world.game.sections[&(cells[0] as u32)].hp, 650.0);
        for &cell in &cells[1..] {
            close(m.world.game.sections[&(cell as u32)].hp, 750.0);
        }
        close(m.world.game.units[&901].hp, 2900.0);
    }
}

#[test]
fn intent_eng38_each_match_tick_syncs_new_building_loss_from_its_hit_origin() {
    let mut m = scene(false);
    let id = m.next_unit_id;
    m.place_starting_building(0, "hq", 15 * WIDTH + 15).unwrap();
    let cells = m.world.game.units[&id].cells.clone().unwrap();
    let max = m.world.game.units[&id].hp / cells.len() as f64;
    let first = cells[0];
    let last = *cells.last().unwrap();
    {
        let unit = m.world.game.units.get_mut(&id).unwrap();
        unit.hp -= 100.;
        unit.hit_from = Some(Point { x: 20., z: 31. });
    }
    assert_eq!(m.advance_tick().unwrap().tick, 1);
    close(m.world.game.sections[&first].hp, max - 100.);
    close(m.world.game.sections[&last].hp, max);
    close(m.world.game.units[&id].hp, 2900.);
    {
        let unit = m.world.game.units.get_mut(&id).unwrap();
        unit.hp -= 50.;
        unit.hit_from = Some(Point { x: 44., z: 35. });
    }
    assert_eq!(m.advance_tick().unwrap().tick, 2);
    close(m.world.game.sections[&first].hp, max - 100.);
    close(m.world.game.sections[&last].hp, max - 50.);
    close(m.world.game.units[&id].hp, 2850.);
    close(
        cells
            .iter()
            .map(|cell| m.world.game.sections[cell].hp)
            .sum(),
        2850.,
    );
}

#[test]
fn intent_eng41_failed_bridge_kills_ground_occupants_and_spares_air_boats_riders() {
    let mut m = empty_scene();
    let at = center(BRIDGE);
    for (id, kind) in [
        (901, "rifle"),
        (902, "tank"),
        (903, "fighter"),
        (904, "gunboat"),
        (905, "rifle"),
    ] {
        add_unit(&mut m, id, kind, 0, at);
    }
    m.world.game.units.get_mut(&905).unwrap().riding = true;
    add_unit(&mut m, 906, "rifle", 0, center(BRIDGE + 1));
    let survivors: Vec<_> = [903, 904, 905, 906]
        .into_iter()
        .map(|id| (id, m.world.game.units[&id].hp))
        .collect();
    let skip = BTreeSet::new();
    let effects = environment(&mut m, |runtime, world, ctx| {
        runtime
            .damage_section(world, BRIDGE, 10000., ctx, &DamageContext::plain(&skip))
            .unwrap()
            .1
    });
    for id in [901, 902] {
        close(m.world.game.units[&id].hp, 0.);
        assert!(
            effects.iter().any(
                |effect| matches!(effect, Effect::Hurt(hurt) if hurt.unit == id && hurt.killed)
            )
        );
    }
    for (id, hp) in survivors {
        close(m.world.game.units[&id].hp, hp);
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::Hurt(hurt) if hurt.unit == id))
        );
    }
    assert!(m.world.game.sections[&(BRIDGE as u32)].failed);
    assert_ne!(m.world.game.physical(BRIDGE), b'=');
}

struct NoRetarget;
impl RetargetSink for NoRetarget {
    fn reset_retarget(&mut self, _: u32) {}
}

fn building_contact(target: &str, shooter_kind: &str, allied: bool, direction: f64) -> f64 {
    let mut m = empty_scene();
    let cells = building(&mut m, 901, target);
    add_unit(
        &mut m,
        902,
        shooter_kind,
        usize::from(!allied),
        Point { x: 20., z: 32. },
    );
    let shooter = &m.world.game.units[&902];
    let source = FlightSource {
        shooter: 902,
        kind: shooter.kind,
        owner: shooter.owner as i32,
        airborne: false,
    };
    let impact = Impact {
        owner: source.owner,
        shooter: source.shooter,
        weapon: WeaponDamage::from_catalog(shooter_kind).unwrap(),
        damage_scale: None,
        energy: 1.,
        armor_piercing: false,
        ap_naval_only: false,
        ap_multiplier: 1.,
        launch_origin: shooter.at(),
        at: center(cells[0]),
        direction: Point {
            x: direction,
            z: 0.,
        },
    };
    let before = m.world.game.units[&901].hp;
    let order: Vec<_> = m.world.game.units.keys().copied().collect();
    let mut damage = DamageWorld {
        world: &mut m.world,
        combat: &mut m.combat,
        grid: &mut m.grid,
        rules: &m.rules,
        logistics: &m.logistics,
        production: &m.production,
        mode: Mode::Conquest,
        tech_enabled: false,
        environment: &mut m.environment,
        environment_seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &order,
        wind_angle: 0.,
        wind_strength: 0.3,
    };
    let (taken, effects) = damage
        .local_contact(
            source,
            ContactTarget::Cell(cells[0]),
            impact,
            "engineering-contact",
            None,
            &mut NoRetarget,
        )
        .unwrap();
    close(before - m.world.game.units[&901].hp, taken);
    close(
        m.world.game.sections[&(cells[0] as u32)].hp,
        before / 4. - taken,
    );
    if allied {
        assert!(effects.is_empty());
        assert!(m.environment.contacts().is_empty());
        close(m.combat.get(902).unwrap().xp, 0.);
    } else {
        close(m.combat.get(902).unwrap().xp, taken);
    }
    taken
}

#[test]
fn intent_eng42_direct_building_hits_apply_weapon_slit_and_alliance_rules() {
    let rifle = WeaponDamage::from_catalog("rifle").unwrap();
    let tank = WeaponDamage::from_catalog("tank").unwrap();
    assert!(rifle.vehicle < 20. && tank.vehicle >= 20.);
    close(
        building_contact("hq", "rifle", false, -1.),
        rifle.infantry * 0.25,
    );
    close(building_contact("hq", "tank", false, -1.), tank.vehicle);
    close(
        building_contact("pillbox", "rifle", false, -1.),
        rifle.infantry * 0.25 * 0.2,
    );
    close(
        building_contact("pillbox", "rifle", false, 1.),
        rifle.infantry * 0.25 * 0.2 * 2.,
    );
    close(
        building_contact("pillbox", "tank", false, -1.),
        tank.vehicle,
    );
    close(
        building_contact("pillbox", "tank", false, 1.),
        tank.vehicle * 2.,
    );
    for target in ["hq", "pillbox"] {
        for shooter in ["rifle", "tank"] {
            close(building_contact(target, shooter, true, 1.), 0.);
        }
    }
}

// Prototype damageCells (sim.js:4734-4794) debits one nearest-cell-strength
// budget per building, carrying unused damage through cells in that order.
#[test]
fn eng42_blast_debits_building_once_at_nearest_cell_strength() {
    let mut m = empty_scene();
    let cells = building(&mut m, 901, "hq");
    environment(&mut m, |runtime, world, ctx| {
        runtime
            .damage_cells(
                world,
                Blast {
                    at: Point { x: 35., z: 33. },
                    radius: 8.,
                    damage: 200.,
                    scar: 0.,
                },
                ctx,
                &mut DamageContext::plain(&BTreeSet::new()),
            )
            .unwrap();
    });
    // The nearest cell is 2 m away: 200 * (1 - 2 / (8 + 2) * 0.5) = 180.
    close(m.world.game.units[&901].hp, 2820.);
    for (index, &cell) in cells.iter().enumerate() {
        close(
            m.world.game.sections[&(cell as u32)].hp,
            if index == 3 { 570. } else { 750. },
        );
        assert!(!m.world.fires.contains_key(&cell), "K cells never ignite");
    }
}

#[test]
fn eng42_blast_overflow_visits_nearest_cells_then_stable_distance_ties() {
    let mut m = empty_scene();
    let cells = building(&mut m, 901, "hq");
    environment(&mut m, |runtime, world, ctx| {
        runtime
            .damage_cells(
                world,
                Blast {
                    at: center(cells[3]),
                    radius: 8.,
                    damage: 1800.,
                    scar: 0.,
                },
                ctx,
                &mut DamageContext::plain(&BTreeSet::new()),
            )
            .unwrap();
    });
    // Last cell is nearest. The equally distant top-right and bottom-left
    // cells follow in prototype row scan order, then the far top-left cell.
    for (&cell, hp) in cells.iter().zip([750., 0., 450., 0.]) {
        close(m.world.game.sections[&(cell as u32)].hp, hp);
        assert_eq!(m.world.game.sections[&(cell as u32)].failed, hp == 0.);
    }
    close(m.world.game.units[&901].hp, 1200.);
}

fn rubble_scene(ground: u8, material: &str) -> Match {
    let mut m = empty_scene();
    let mut chars = vec![b'.'; WIDTH * WIDTH];
    chars[16 * WIDTH + 15] = ground;
    chars[15 * WIDTH + 15] = b'B';
    let rows: Vec<_> = chars
        .chunks(WIDTH)
        .map(|row| String::from_utf8(row.to_vec()).unwrap())
        .collect();
    let map = AuthoritativeMap::from_value(json!({
        "name": "Rubble settlement", "w": WIDTH, "h": WIDTH, "rows": rows,
        "spawns": [{"x": 3, "y": 3}, {"x": 36, "y": 36}],
        "points": [{"id": "center", "x": 20, "y": 20}]
    }))
    .unwrap();
    let players = m.world.game.players.clone();
    m.world = World::from_map(&map).unwrap();
    m.world.game.players = players;
    let source = 15 * WIDTH + 15;
    m.world.sections.get_mut(&source).unwrap().material = material.into();
    m.world.game.sections.get_mut(&(source as u32)).unwrap().hp = 1.;
    m.world.game.cell_hp[source] = 1.;
    m
}

fn settle_house(m: &mut Match, source: usize) {
    environment(m, |runtime, world, ctx| {
        let skip = BTreeSet::new();
        let mut hit = DamageContext::plain(&skip);
        hit.direction = Some(Point { x: 0., z: 1. });
        runtime
            .damage_section(world, source, 1., ctx, &hit)
            .unwrap();
        assert_eq!(world.game.physical(source), b'.');
        assert!(world.sections[&source].rubble.is_empty());
        runtime.step_sections(world, 2.5, ctx).unwrap();
    });
    assert!(m.environment.falling_sections().is_empty());
    assert!(m.world.sections[&source].failure.settled_at.is_some());
}

#[test]
fn eng44_settled_house_rubble_keeps_material_covers_two_cells_and_only_fill_clears_it() {
    let source = 15 * WIDTH + 15;
    for ground in *b".D+MR" {
        for material in ["wood", "stone"] {
            let mut m = rubble_scene(ground, material);
            settle_house(&mut m, source);
            let rubble = m.world.sections[&source].rubble.clone();
            // With a minimum impulse the body lands in its original cell;
            // the prototype's 1.4 m projection reaches the next row.
            assert_eq!(
                rubble,
                vec![source, source + WIDTH],
                "ground {}",
                ground as char
            );
            for &cell in &rubble {
                assert_eq!(m.world.game.physical(cell), b'R');
                assert_eq!(m.world.material_name(cell, false), Some(material));
                assert_eq!(
                    m.world.game.flags[cell] & (VBLOCK | COVER | MOVE),
                    VBLOCK | COVER
                );
                assert_eq!(m.world.game.cell_hp[cell], 0.);
                assert_eq!(m.world.max_hp(cell), 0.);
            }
            environment(&mut m, |runtime, world, ctx| {
                runtime
                    .damage_cells(
                        world,
                        Blast {
                            at: center(source),
                            radius: 4.,
                            damage: 5000.,
                            scar: 0.,
                        },
                        ctx,
                        &mut DamageContext::plain(&BTreeSet::new()),
                    )
                    .unwrap();
                for &cell in &rubble {
                    world.fires.insert(cell, 0.1);
                }
                runtime.burn(world, 2.5, ctx).unwrap();
                assert!(rubble.iter().all(|cell| !world.fires.contains_key(cell)));
                runtime.step_sections(world, 2.5, ctx).unwrap();
            });
            for &cell in &rubble {
                assert_eq!(m.world.game.physical(cell), b'R');
                assert!(m.world.fillable(cell));
                m.world.fill_cell(cell).unwrap();
                assert_ne!(m.world.game.physical(cell), b'R');
                assert_eq!(m.world.game.flags[cell] & VBLOCK, 0);
            }
        }
    }
}

#[test]
fn eng44_house_rubble_skips_live_vehicles_and_ineligible_ground() {
    let source = 15 * WIDTH + 15;
    for ground in *b"WFT#XY" {
        let mut m = rubble_scene(ground, "stone");
        settle_house(&mut m, source);
        assert!(!m.world.sections[&source].rubble.contains(&(source + WIDTH)));
        assert_eq!(m.world.game.physical(source + WIDTH), ground);
    }
    for alive in [true, false] {
        let mut m = rubble_scene(b'.', "wood");
        add_unit(&mut m, 901, "tank", 0, center(source + WIDTH));
        if !alive {
            m.world.game.units.get_mut(&901).unwrap().hp = 0.;
        }
        settle_house(&mut m, source);
        assert_eq!(m.world.game.physical(source + WIDTH) == b'R', !alive);
    }
}

#[test]
fn eng44_failed_bridge_settles_without_rubble() {
    let mut m = empty_scene();
    environment(&mut m, |runtime, world, ctx| {
        runtime
            .damage_section(
                world,
                BRIDGE,
                5000.,
                ctx,
                &DamageContext::plain(&BTreeSet::new()),
            )
            .unwrap();
        runtime.step_sections(world, 2.5, ctx).unwrap();
    });
    assert!(m.environment.falling_sections().is_empty());
    assert!(m.world.sections[&BRIDGE].failure.settled_at.is_some());
    assert!(m.world.sections[&BRIDGE].rubble.is_empty());
    assert!(!m.world.game.chars.contains(&b'R'));
}

#[test]
fn eng44_dead_building_footprint_becomes_durable_rubble_until_fill() {
    for occupied in [false, true] {
        let mut m = empty_scene();
        let cells = building(&mut m, 901, "hq");
        if occupied {
            add_unit(&mut m, 902, "tank", 0, center(cells[3]));
        }
        // Initialize the ordinary death phase's required unit sidecars.
        let unit = &m.world.game.units[&901];
        m.commands.units.insert(
            901,
            crate::match_commands::UnitControl::new(unit, &m.rules).unwrap(),
        );
        m.abilities.initialize(901);
        m.recovery.initialize(901);
        m.world.game.units.get_mut(&901).unwrap().hp = 0.;
        let result = m.retire_dead(&[901]).unwrap();
        assert_eq!(result.retired.len(), 1);
        assert!(!m.world.game.units.contains_key(&901));
        if occupied {
            assert!(
                m.world.game.units[&902].hp > 0.,
                "building footprint rubble is unconditional"
            );
        }
        for &cell in &cells {
            assert_eq!(m.world.game.physical(cell), b'R');
            assert_eq!(m.world.game.cell_hp[cell], 0.);
            assert_eq!(
                m.world.game.flags[cell] & (VBLOCK | COVER | MOVE),
                VBLOCK | COVER
            );
        }
        environment(&mut m, |runtime, world, ctx| {
            runtime.step_sections(world, 2.5, ctx).unwrap();
            runtime
                .damage_cells(
                    world,
                    Blast {
                        at: Point { x: 32., z: 32. },
                        radius: 8.,
                        damage: 5000.,
                        scar: 0.,
                    },
                    ctx,
                    &mut DamageContext::plain(&BTreeSet::new()),
                )
                .unwrap();
        });
        for &cell in &cells {
            assert_eq!(m.world.game.physical(cell), b'R');
            m.world.fill_cell(cell).unwrap();
            assert_eq!(m.world.game.physical(cell), b'.');
            assert_eq!(m.world.game.flags[cell] & VBLOCK, 0);
        }
    }
}

fn spawn_wreck(m: &mut Match, id: u32, at: Point) {
    let kind = m.rules.kind("tank").unwrap();
    let mut corpse = initial_unit(&m.rules, Catalog::compiled().unwrap(), kind, id, 0, at).unwrap();
    corpse.hp = 0.;
    environment(m, |runtime, world, ctx| {
        runtime
            .spawn_vehicle_wreck(world, &corpse, None, ctx)
            .unwrap();
    });
}

#[test]
fn intent_eng67_wrecks_settle_block_vehicles_give_cover_and_retire_oldest() {
    let mut m = empty_scene();
    let origin = center(8 * WIDTH + 8);
    spawn_wreck(&mut m, 901, origin);
    assert!(m.world.game.wrecks[0].moving);
    assert!(m.world.game.wrecks[0].cell.is_none());
    environment(&mut m, |runtime, world, ctx| {
        runtime.step_wrecks(world, 2.5, ctx).unwrap()
    });
    let hull = m.world.game.wrecks[0].clone();
    let cell = hull.cell.expect("settled hull stamps open ground");
    assert!(!hull.moving);
    assert!(m.environment.wreck_metadata()[&901].motion.is_none());
    assert!(crate::math::hypot(hull.x - origin.x, hull.z - origin.z) <= 2.);
    assert_eq!(m.world.game.physical(cell), b'Q');
    assert_eq!(m.world.game.flags[cell] & (VBLOCK | COVER), VBLOCK | COVER);
    assert_eq!(m.world.game.flags[cell] & MOVE, 0);
    close(m.world.max_hp(cell), 300.);
    add_unit(
        &mut m,
        902,
        "rifle",
        0,
        Point {
            x: hull.x - 2.,
            z: hull.z,
        },
    );
    let squad = &m.world.game.units[&902];
    assert!(
        m.cover_behind(
            squad,
            Point {
                x: hull.x + 20.,
                z: hull.z
            }
        )
        .unwrap()
            > 0.9
    );
    close(
        m.cover_behind(
            squad,
            Point {
                x: hull.x - 20.,
                z: hull.z,
            },
        )
        .unwrap(),
        0.,
    );
    for id in 1000..1040 {
        spawn_wreck(&mut m, id, center(30 * WIDTH + 30));
    }
    assert_eq!(m.world.game.wrecks.len(), 40);
    assert_eq!(m.world.game.wrecks.first().unwrap().id, Some(1000));
    assert!(!m.environment.wreck_metadata().contains_key(&901));
    assert_eq!(m.world.game.physical(cell), b'.');
    assert_ne!(m.world.burnt[cell], 0);
}

#[test]
fn intent_eng67_wrecks_skip_water_and_do_not_stamp_under_a_live_vehicle() {
    let mut m = empty_scene();
    let water = 8 * WIDTH + 8;
    m.world.set_cell(water, b'W').unwrap();
    spawn_wreck(&mut m, 901, center(water));
    assert!(m.world.game.wrecks.is_empty());
    let occupied = 20 * WIDTH + 20;
    add_unit(&mut m, 902, "tank", 0, center(occupied));
    spawn_wreck(&mut m, 903, center(occupied));
    environment(&mut m, |runtime, world, ctx| {
        runtime.step_wrecks(world, 2.5, ctx).unwrap()
    });
    let hull = &m.world.game.wrecks[0];
    assert!(!hull.moving);
    assert!(hull.cell.is_none());
    assert!(!m.world.game.chars.contains(&b'Q'));
}

#[test]
fn intent_eng67_destroying_settled_wreck_opens_a_crater_and_retires_its_hull() {
    let mut m = empty_scene();
    spawn_wreck(&mut m, 901, center(8 * WIDTH + 8));
    environment(&mut m, |runtime, world, ctx| {
        runtime.step_wrecks(world, 2.5, ctx).unwrap()
    });
    let cell = m.world.game.wrecks[0].cell.unwrap();
    let skip = BTreeSet::new();
    environment(&mut m, |runtime, world, ctx| {
        runtime
            .damage_cells(
                world,
                Blast {
                    at: center(cell),
                    radius: 0.,
                    damage: 301.,
                    scar: 0.,
                },
                ctx,
                &mut DamageContext::plain(&skip),
            )
            .unwrap();
    });
    assert_eq!(m.world.game.physical(cell), b'+');
    assert_eq!(m.world.game.flags[cell] & (MOVE | VBLOCK), 0);
    assert!(m.world.game.wrecks.is_empty());
    // The subsequent debris phase removes metadata for terrain-cleared hulls.
    environment(&mut m, |runtime, world, ctx| {
        runtime.step_wrecks(world, 0., ctx).unwrap()
    });
    assert!(!m.environment.wreck_metadata().contains_key(&901));
}

fn hedge_fire(m: &mut Match) {
    for dy in -1..=1 {
        for dx in -1..=1 {
            let cell = (FIRE as isize + dy * WIDTH as isize + dx) as usize;
            m.world.set_cell(cell, b'H').unwrap();
        }
    }
    environment(m, |runtime, world, ctx| {
        runtime.ignite(world, FIRE, ctx).unwrap()
    });
}

#[test]
fn intent_eng70_fire_spreads_on_half_second_ticks_and_new_fires_wait() {
    let mut m = scene(false);
    m.start.environment_seed = 35;
    hedge_fire(&mut m);
    for tick in 1..=9 {
        assert_eq!(m.advance_tick().unwrap().tick, tick);
        assert_eq!(m.world.fires.len(), 1);
        close(*m.world.fires.get(&FIRE).unwrap(), 14.);
    }
    assert_eq!(m.advance_tick().unwrap().tick, 10);
    close(*m.world.fires.get(&FIRE).unwrap(), 13.5);
    let spread: Vec<_> = m
        .world
        .fires
        .iter()
        .filter(|(cell, _)| **cell != FIRE)
        .map(|(cell, timer)| (*cell, *timer))
        .collect();
    assert!(
        !spread.is_empty(),
        "fixed seed spreads on the first burn phase"
    );
    for (_, timer) in &spread {
        close(*timer, 14.);
    }
    for tick in 11..=19 {
        assert_eq!(m.advance_tick().unwrap().tick, tick);
        for (cell, timer) in &spread {
            close(*m.world.fires.get(cell).unwrap(), *timer);
        }
    }
    assert_eq!(m.advance_tick().unwrap().tick, 20);
    close(*m.world.fires.get(&FIRE).unwrap(), 13.);
    for (cell, _) in spread {
        close(*m.world.fires.get(&cell).unwrap(), 13.5);
    }
}

fn isolated_fire(m: &mut Match, ch: u8, neighbor: Option<u8>) {
    for dy in -1..=1 {
        for dx in -1..=1 {
            let cell = (FIRE as isize + dy * WIDTH as isize + dx) as usize;
            m.world.set_cell(cell, b'D').unwrap();
        }
    }
    m.world.set_cell(FIRE, ch).unwrap();
    if let Some(ch) = neighbor {
        m.world.set_cell(FIRE + 1, ch).unwrap();
    }
    environment(m, |runtime, world, ctx| {
        runtime.ignite(world, FIRE, ctx).unwrap()
    });
}

#[test]
fn intent_eng70_all_fuel_timers_expire_faster_in_rain() {
    for (ch, duration) in [(b'H', 14.), (b'B', 25.), (b'.', 5.), (b'O', 20.)] {
        for rain in [0., 0.5, 1.] {
            let mut m = empty_scene();
            m.world.game.rain = rain;
            isolated_fire(&mut m, ch, None);
            close(*m.world.fires.get(&FIRE).unwrap(), duration);
            environment(&mut m, |runtime, world, ctx| {
                runtime.burn(world, 0.5, ctx).unwrap()
            });
            close(
                *m.world.fires.get(&FIRE).unwrap(),
                duration - 0.5 * (1. + 3. * rain),
            );
            // Exhaust the remaining timer through the same burn actuator.
            let left = *m.world.fires.get(&FIRE).unwrap();
            environment(&mut m, |runtime, world, ctx| {
                runtime
                    .burn(world, left / (1. + 3. * rain) + 0.001, ctx)
                    .unwrap()
            });
            assert!(!m.world.fires.contains_key(&FIRE));
            assert_eq!(m.world.burnt[FIRE], 1);
            if matches!(ch, b'H' | b'O') {
                assert_eq!(m.world.game.physical(FIRE), b'.');
            }
        }
    }
}

fn spread_attempt(
    ch: u8,
    dt: f64,
    wind_angle: f64,
    wind_strength: f64,
    rain: f64,
    seed: u32,
) -> bool {
    let mut m = empty_scene();
    m.world.game.rain = rain;
    m.start.environment_seed = seed;
    m.start.wind = (wind_angle, wind_strength);
    isolated_fire(&mut m, b'H', Some(ch));
    environment(&mut m, |runtime, world, ctx| {
        runtime.burn(world, dt, ctx).unwrap()
    });
    m.world.fires.contains_key(&(FIRE + 1))
}

#[test]
fn intent_eng70_spread_rates_wind_direction_floor_and_rain_use_seeded_thresholds() {
    // Seed 8's first environment draw is 0.15629516984336078. Each pair
    // brackets it using that terrain's pinned spread rate and calm wind.
    for (ch, below, above) in [
        (b'H', 0.624, 0.626),
        (b'B', 5.20, 5.22),
        (b'.', 7.10, 7.12),
        (b'O', 1.302, 1.304),
    ] {
        assert!(!spread_attempt(ch, below, 0., 0., 0., 8));
        assert!(spread_attempt(ch, above, 0., 0., 0., 8));
    }
    // Downwind probability is 0.21875, upwind 0.03125. Rain halves the
    // former to 0.109375, below the same seed's draw.
    assert!(spread_attempt(b'H', 0.5, 0., 0.5, 0., 8));
    assert!(!spread_attempt(b'H', 0.5, std::f64::consts::PI, 0.5, 0., 8));
    assert!(!spread_attempt(b'H', 0.5, 0., 0.5, 0.5, 8));
    assert!(!spread_attempt(b'H', 0.5, 0., 0.5, 1., 8));
    // Seed 7 draws 0.011704753153026104. Strong opposing wind keeps
    // the 0.1 floor (probability 0.0125); rain 0.1 lowers it to 0.01125.
    assert!(spread_attempt(b'H', 0.5, std::f64::consts::PI, 1., 0., 7));
    assert!(!spread_attempt(b'H', 0.5, std::f64::consts::PI, 1., 0.1, 7));
}

#[test]
fn intent_eng80_environment_and_combat_draws_do_not_perturb_each_other() {
    let mut base = scene(true);
    let mut extra_combat = scene(true);
    let mut no_fire = scene(true);
    for m in [&mut base, &mut extra_combat, &mut no_fire] {
        m.start.environment_seed = 35;
        m.start.living.as_mut().unwrap().next = 0.;
    }
    hedge_fire(&mut base);
    hedge_fire(&mut extra_combat);
    for _ in 0..100 {
        extra_combat.start.random.next_u64(StreamId::Combat);
    }
    for m in [&mut base, &mut extra_combat] {
        let cell = 13 * WIDTH + 24;
        m.world.set_cell(cell, b'O').unwrap();
        let skip = BTreeSet::new();
        environment(m, |runtime, world, ctx| {
            runtime
                .damage_cells(
                    world,
                    Blast {
                        at: center(cell),
                        radius: 0.,
                        damage: 60.,
                        scar: 0.,
                    },
                    ctx,
                    &mut DamageContext::plain(&skip),
                )
                .unwrap();
        });
        assert_ne!(m.start.environment_seed, 35);
        assert!(
            m.world.fires.contains_key(&cell),
            "the same environment seed also controls blast ignition"
        );
    }
    for _ in 0..30 {
        base.advance_tick().unwrap();
        extra_combat.advance_tick().unwrap();
        no_fire.advance_tick().unwrap();
        assert_eq!(
            base.start.environment_seed,
            extra_combat.start.environment_seed
        );
        assert_eq!(base.start.wind, extra_combat.start.wind);
        assert_eq!(base.world.game.rain, extra_combat.world.game.rain);
        assert_eq!(base.world.game.wet, extra_combat.world.game.wet);
        assert_eq!(
            base.start.living.unwrap().next,
            extra_combat.start.living.unwrap().next
        );
        assert_eq!(
            base.world.fires.iter().collect::<Vec<_>>(),
            extra_combat.world.fires.iter().collect::<Vec<_>>()
        );
    }
    assert!(base.start.living.unwrap().raining && base.world.game.rain > 0.);
    assert_ne!(base.start.environment_seed, no_fire.start.environment_seed);
    assert_eq!(
        base.start.random.next_u64(StreamId::Combat),
        no_fire.start.random.next_u64(StreamId::Combat)
    );
    let weather = WeatherRules::compiled().unwrap();
    let plan = weather.plan_weather(WeatherChoice::Random, MapWeatherInfo::default(), None, 123);
    for _ in 0..100 {
        base.start.random.next_u64(StreamId::Combat);
        crate::weather_runtime::environment_random(&mut base.start.environment_seed);
    }
    assert_eq!(
        weather.plan_weather(WeatherChoice::Random, MapWeatherInfo::default(), None, 123),
        plan
    );
}
