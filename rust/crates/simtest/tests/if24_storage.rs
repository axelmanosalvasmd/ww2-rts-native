//! Long-running storage rounding, qualified beside the full pathfinding fixtures.
use serde_json::json;
use std::collections::BTreeSet;
use ww2_core::{
    environment::{Blast, Context, DamageContext, EnvironmentRuntime},
    game::Point,
    grid::SpatialGrid,
    map::AuthoritativeMap,
    world::World,
};

#[test]
fn if24_repeated_terrain_hits_round_storage_after_each_of_6000_steps() {
    let mut rows = vec![vec![b'.'; 64]; 64];
    rows[30][32] = b'T';
    rows[30][34] = b'#';
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Float32 terrain storage","w":64,"h":64,
            "rows":rows.iter().map(|row| String::from_utf8(row.clone()).unwrap()).collect::<Vec<_>>(),
            "spawns":[{"x":2,"y":2},{"x":61,"y":61}],
            "points":[{"x":32,"y":32}]
        })).unwrap(),
    ).unwrap();
    let mut world = World::from_map(&map).unwrap();
    let mut runtime = EnvironmentRuntime::default();
    let grid = SpatialGrid::new(&world.game.units, 16.0);
    let mut seed = 123;
    let mut next_id = 1000;
    let skip = BTreeSet::new();
    let cells = [30 * 64 + 30, 30 * 64 + 32, 30 * 64 + 34];
    let damage = 0.013;
    let mut expected_scar = world.scar[cells[0]];
    let mut expected_wear = world.game.wear.as_ref().unwrap()[cells[1]];
    let mut expected_hp = world.game.cell_hp[cells[2]];
    for tick in 1..=6000 {
        let mut context = Context {
            tick,
            seed: &mut seed,
            next_id: &mut next_id,
            unit_order: &[],
            grid: &grid,
            rain: 0.0,
            wet: 0.0,
            wind_angle: 0.0,
            wind_strength: 0.5,
        };
        for cell in cells {
            runtime
                .damage_cells(
                    &mut world,
                    Blast {
                        at: Point {
                            x: (cell % 64) as f64 * 2.0 + 1.0,
                            z: (cell / 64) as f64 * 2.0 + 1.0,
                        },
                        radius: 0.0,
                        damage,
                        scar: 0.31,
                    },
                    &mut context,
                    &mut DamageContext::plain(&skip),
                )
                .unwrap();
        }
        expected_scar = (f64::from(expected_scar) + damage * 0.31 / 600.0) as f32;
        expected_wear = (f64::from(expected_wear) + damage / 400.0) as f32;
        expected_hp = (f64::from(expected_hp) - damage) as f32;
        assert_eq!(
            world.scar[cells[0]].to_bits(),
            expected_scar.to_bits(),
            "scar at {tick}"
        );
        assert_eq!(
            world.game.wear.as_ref().unwrap()[cells[1]].to_bits(),
            expected_wear.to_bits(),
            "wear at {tick}"
        );
        assert_eq!(
            world.game.cell_hp[cells[2]].to_bits(),
            expected_hp.to_bits(),
            "HP at {tick}"
        );
    }
    assert_ne!(
        expected_scar.to_bits(),
        ((6000.0 * damage * 0.31 / 600.0) as f32).to_bits()
    );
    assert_eq!(world.game.chars[cells[0]], b'.');
    assert_eq!(world.game.chars[cells[1]], b'T');
    assert_eq!(world.game.chars[cells[2]], b'#');

    // A typed-array assignment returns its unrounded expression. Storage can
    // reach one before the trench-collapse comparison does.
    world.game.wear.as_mut().unwrap()[cells[1]] = f32::from_bits(0x3f7f_ffff);
    let mut context = Context {
        tick: 6001,
        seed: &mut seed,
        next_id: &mut next_id,
        unit_order: &[],
        grid: &grid,
        rain: 0.0,
        wet: 0.0,
        wind_angle: 0.0,
        wind_strength: 0.5,
    };
    runtime
        .damage_cells(
            &mut world,
            Blast {
                at: Point { x: 65.0, z: 61.0 },
                radius: 0.0,
                damage: 400.0 * 3.0 / 67_108_864.0,
                scar: 0.0,
            },
            &mut context,
            &mut DamageContext::plain(&skip),
        )
        .unwrap();
    assert_eq!(world.game.wear.as_ref().unwrap()[cells[1]], 1.0);
    assert_eq!(world.game.chars[cells[1]], b'T');
}
