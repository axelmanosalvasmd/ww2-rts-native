use super::*;
use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{Game, Point},
    grid::UnitId,
    map::AuthoritativeMap,
    match_commands::{Command, MoveOrder},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::{DeliveryState, SelectedSeat},
    rules::Rules,
    unit_spawn,
    weather::WeatherChoice,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn sorted_ids(mut ids: Vec<UnitId>) -> Vec<UnitId> {
    ids.sort_unstable();
    ids
}

pub(crate) fn assert_current(game: &Game, grid: &SpatialGrid) {
    let units = &game.units;
    let fresh = SpatialGrid::new(units, 16.0);
    // A fresh BTreeMap index has ID order; the live index retains first-seen order.
    // Compare membership here. The Source placement fixture checks exact query order.
    let mut owners: BTreeSet<_> = (0..game.players.len()).map(|owner| owner as i32).collect();
    owners.extend(units.values().map(|unit| unit.owner as i32));
    for owner in owners {
        assert_eq!(
            sorted_ids(grid.owned_by(units, owner)),
            sorted_ids(fresh.owned_by(units, owner))
        );
    }
    let mut buckets = BTreeSet::new();
    for z in -1..=(game.h as i64 * 2 / 16 + 1) {
        for x in -1..=(game.w as i64 * 2 / 16 + 1) {
            buckets.insert((x, z));
        }
    }
    // Aircraft bases can lie farther outside the map than its border buckets.
    buckets.extend(units.values().map(|unit| {
        (
            (unit.x / 16.0).floor() as i64,
            (unit.z / 16.0).floor() as i64,
        )
    }));
    for (x, z) in buckets {
        let at = (x as f64 * 16.0 + 8.0, z as f64 * 16.0 + 8.0);
        assert_eq!(
            sorted_ids(grid.in_order(units, at, 0.0, |_, _| true)),
            sorted_ids(fresh.in_order(units, at, 0.0, |_, _| true)),
            "ordered query membership {at:?}"
        );
        let mut actual = grid.unordered(units, at, 0.0, |_, _| true);
        let mut expected = fresh.unordered(units, at, 0.0, |_, _| true);
        actual.sort_unstable();
        expected.sort_unstable();
        assert_eq!(actual, expected, "bucket membership {at:?}");
        let mut actual = grid.heavy(units, at, 0.0, |_, _| true);
        let mut expected = fresh.heavy(units, at, 0.0, |_, _| true);
        actual.sort_unstable();
        expected.sort_unstable();
        assert_eq!(actual, expected, "heavy membership {at:?}");
        assert_eq!(
            grid.has(units, at, 0.0, |_, _| true),
            fresh.has(units, at, 0.0, |_, _| true)
        );
        assert_eq!(
            grid.has_heavy(units, at, 0.0, |_, _| true),
            fresh.has_heavy(units, at, 0.0, |_, _| true)
        );
    }
    for unit in units.values() {
        let at = (unit.x, unit.z);
        assert_eq!(
            sorted_ids(grid.radius(units, at, 20.0, |_, _| true)),
            sorted_ids(fresh.radius(units, at, 20.0, |_, _| true))
        );
        let distance = |id| {
            let target = &units[&id];
            crate::math::hypot(target.x - at.0, target.z - at.1).to_bits()
        };
        assert_eq!(
            grid.nearest(units, at, 20.0, |_, _| true).map(distance),
            fresh.nearest(units, at, 20.0, |_, _| true).map(distance)
        );
    }
}

fn number(value: &Value, key: &str) -> f64 {
    value[key].as_f64().unwrap()
}
fn queries(grid: &SpatialGrid, units: &BTreeMap<UnitId, Unit>, rows: &Value) -> Value {
    json!(
        rows.as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let at = (number(row, "x"), number(row, "z"));
                let radius = number(row, "radius");
                let mut unordered = grid.unordered(units, at, radius, |_, _| true);
                unordered.sort_unstable();
                let mut heavy = grid.heavy(units, at, radius, |_, _| true);
                heavy.sort_unstable();
                json!({
                    "ordered": grid.in_order(units, at, radius, |_, _| true),
                    "unordered": unordered, "heavy": heavy,
                    "radius": grid.radius(units, at, radius, |_, _| true),
                    "nearest": grid.nearest(units, at, radius, |_, _| true),
                    "has": grid.has(units, at, radius, |_, _| true),
                    "hasHeavy": grid.has_heavy(units, at, radius, |_, _| true),
                })
            })
            .collect::<Vec<_>>()
    )
}

#[test]
fn mov67_source_placements_keep_fields_and_every_grid_query_current() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/unit-placement.json")).unwrap();
    assert_eq!(fixture["pin"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    let rules = Rules::compiled().unwrap();
    let catalog = Catalog::compiled().unwrap();
    let mut units = BTreeMap::new();
    for row in fixture["initial"].as_array().unwrap() {
        let id = row["id"].as_u64().unwrap() as u32;
        let mut unit = unit_spawn::initial_unit(
            &rules,
            catalog,
            rules.kind(row["type"].as_str().unwrap()).unwrap(),
            id,
            row["owner"].as_u64().unwrap() as usize,
            Point {
                x: number(row, "x"),
                z: number(row, "z"),
            },
        )
        .unwrap();
        unit.still = Some(0.0);
        units.insert(id, unit);
    }
    let mut grid = SpatialGrid::new(&units, 16.0);
    for (placement, expected) in fixture["placements"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fixture["rows"].as_array().unwrap())
    {
        let id = placement["id"].as_u64().unwrap() as u32;
        place_unit(&mut grid, units.get_mut(&id).unwrap(), |unit| {
            for (key, value) in placement["fields"].as_object().unwrap() {
                match key.as_str() {
                    "x" => unit.x = value.as_f64().unwrap(),
                    "z" => unit.z = value.as_f64().unwrap(),
                    "rot" => unit.rot = value.as_f64().unwrap(),
                    "owner" => unit.owner = value.as_u64().unwrap() as usize,
                    "garrison" => unit.garrison = usize::try_from(value.as_i64().unwrap()).ok(),
                    "riding" => unit.riding = value.as_u64().unwrap() != 0,
                    "supp" => unit.supp = value.as_f64().unwrap(),
                    "still" => unit.still = Some(value.as_f64().unwrap()),
                    _ => panic!("unrepresented placement field {key}"),
                }
            }
        });
        for row in expected["units"].as_array().unwrap() {
            let unit = &units[&(row["id"].as_u64().unwrap() as u32)];
            for (key, actual) in [
                ("x", unit.x),
                ("z", unit.z),
                ("rot", unit.rot),
                ("supp", unit.supp),
                ("still", unit.still.unwrap()),
            ] {
                assert_eq!(
                    actual.to_bits(),
                    number(row, key).to_bits(),
                    "placement {placement} field {key}"
                );
            }
            assert_eq!(unit.owner, row["owner"].as_u64().unwrap() as usize);
            assert_eq!(
                unit.garrison,
                usize::try_from(row["garrison"].as_i64().unwrap()).ok()
            );
            assert_eq!(unit.riding, row["riding"].as_u64().unwrap() != 0);
        }
        assert_eq!(
            queries(&grid, &units, &fixture["queries"]),
            expected["queries"]
        );
        let fresh = SpatialGrid::new(&units, 16.0);
        assert_eq!(
            queries(&grid, &units, &fixture["queries"]),
            queries(&fresh, &units, &fixture["queries"])
        );
        for owner in 0..2 {
            assert_eq!(
                json!(grid.owned_by(&units, owner)),
                expected["owners"][owner as usize]
            );
        }
    }
}

#[test]
fn mov67_relocations_survive_ordinary_ticks_and_all_seat_snapshots() {
    for mode in [Mode::Default, Mode::Conquest] {
        let map = AuthoritativeMap::from_json(
            &serde_json::to_vec(&json!({
                "name":"Placement integration", "w":64,"h":64,"rows":vec![".".repeat(64);64],
                "spawns":[{"x":6,"y":6},{"x":56,"y":56}],"points":[{"x":32,"y":32}]
            }))
            .unwrap(),
        )
        .unwrap();
        let mut state = Match::new(
            &map,
            MatchOptions {
                seed: 701,
                mode,
                players: (0..2)
                    .map(|slot| PlayerConfig {
                        name: format!("p{slot}"),
                        team: 0,
                        faction: Faction::try_from(slot as u8).unwrap(),
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
        assert_current(&state.world.game, &state.grid);
        let id = state
            .world
            .game
            .units
            .values()
            .find(|unit| unit.owner == 0 && state.rules.kinds[unit.kind].name == "rifle")
            .unwrap()
            .id;
        let selected = (0..2)
            .map(|slot| SelectedSeat {
                slot,
                full: false,
                omit: false,
            })
            .collect::<Vec<_>>();
        let mut delivery = DeliveryState::new(&state).unwrap();
        for at in [
            Point { x: 31.5, z: 31.5 },
            Point { x: 65.0, z: 65.0 },
            Point { x: 48.5, z: 47.5 },
        ] {
            place_unit(
                &mut state.grid,
                state.world.game.units.get_mut(&id).unwrap(),
                |unit| {
                    unit.x = at.x;
                    unit.z = at.z;
                    unit.path.clear();
                    unit.world_goal = None;
                },
            );
            assert_current(&state.world.game, &state.grid);
            assert_eq!(
                state
                    .execute_command(
                        0,
                        &Command::Move {
                            orders: vec![MoveOrder {
                                unit: i64::from(id),
                                at: Point {
                                    x: at.x + 3.0,
                                    z: at.z
                                }
                            }],
                            control_ids: None,
                            attack_move: false,
                            queue: false,
                            together: false,
                            face: None,
                        },
                        false
                    )
                    .unwrap(),
                None
            );
            for _ in 0..4 {
                let report = state.advance_tick().unwrap();
                assert_current(&state.world.game, &state.grid);
                let frame = delivery
                    .capture_selected_with_tick_events(&mut state, &selected, &report.events)
                    .unwrap()
                    .unwrap();
                for seat in &selected {
                    let snapshot = frame.recipient(seat.slot).unwrap().into_json();
                    assert_eq!(snapshot["t"], "s");
                    assert!(
                        snapshot["units"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|row| row[0].as_u64() == Some(u64::from(id)))
                    );
                }
            }
        }
        assert!(state.tick_state.fault().is_none());
    }
}
