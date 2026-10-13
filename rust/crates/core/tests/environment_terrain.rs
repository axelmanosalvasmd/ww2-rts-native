use ww2_core::{
    environment::{Context, EnvironmentError, EnvironmentRuntime},
    grid::SpatialGrid,
    map::AuthoritativeMap,
    world::World,
};

#[test]
fn terrain_shapes_reject_before_retirement_or_debris_writes() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/environment.json")).unwrap();
    let map =
        AuthoritativeMap::from_json(&serde_json::to_vec(&fixture["cases"][0]["map"]).unwrap())
            .unwrap();
    let mutations: &[fn(&mut World)] = &[
        |w| w.game.w = 0,
        |w| w.game.h = usize::MAX,
        |w| w.game.chars.clear(),
        |w| w.game.flags.clear(),
        |w| w.game.house.clear(),
        |w| w.game.cell_hp.clear(),
        |w| w.mine_layer.clear(),
        |w| w.surface_wear.clear(),
        |w| w.burnt.clear(),
        |w| w.scar.clear(),
        |w| w.cell_state.clear(),
        |w| w.game.ground = None,
        |w| w.game.ground = Some(Vec::new()),
        |w| w.game.objects = None,
        |w| w.game.objects = Some(Vec::new()),
        |w| w.game.wear = None,
        |w| w.game.wear = Some(Vec::new()),
        |w| w.game.height = Some(Vec::new()),
        |w| w.game.trench_front = Some(Vec::new()),
        |w| w.game.tough = Some(Vec::new()),
        |w| w.shoal = Some(Vec::new()),
    ];
    for (index, mutate) in mutations.iter().enumerate() {
        let mut w = World::from_map(&map).unwrap();
        mutate(&mut w);
        let before = format!("{w:?}");
        let mut runtime = EnvironmentRuntime::default();
        let order: Vec<_> = w.game.units.keys().copied().collect();
        let grid = SpatialGrid::new(&w.game.units, 16.);
        let mut seed = 123;
        let mut next_id = 1000;
        let mut ctx = Context {
            tick: 10,
            seed: &mut seed,
            next_id: &mut next_id,
            unit_order: &order,
            grid: &grid,
            rain: 0.,
            wet: 0.,
            wind_angle: 0.,
            wind_strength: 0.5,
        };
        let error = Err(EnvironmentError::InvalidInput("terrain shape"));
        assert_eq!(runtime.validate_retirement(&w, &[], &ctx), error, "{index}");
        assert_eq!(
            runtime.step_wrecks(&mut w, 0.05, &mut ctx),
            error,
            "{index}"
        );
        assert_eq!(format!("{w:?}"), before, "{index}");
        assert_eq!((*ctx.seed, *ctx.next_id), (123, 1000), "{index}");
    }
    let mut flat = World::from_map(&map).unwrap();
    flat.game.height = None;
    flat.game.trench_front = None;
    flat.game.tough = None;
    flat.shoal = None;
    let order: Vec<_> = flat.game.units.keys().copied().collect();
    let grid = SpatialGrid::new(&flat.game.units, 16.);
    let mut seed = 123;
    let mut next_id = 1000;
    let ctx = Context {
        tick: 10,
        seed: &mut seed,
        next_id: &mut next_id,
        unit_order: &order,
        grid: &grid,
        rain: 0.,
        wet: 0.,
        wind_angle: 0.,
        wind_strength: 0.5,
    };
    assert!(
        EnvironmentRuntime::default()
            .validate_retirement(&flat, &[], &ctx)
            .is_ok()
    );
}
