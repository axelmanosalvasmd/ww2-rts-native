use ww2_core::{game::Point, random::StreamId};

#[test]
fn if20_smoke_scatter_consumes_only_the_off_map_stream() {
    let mut control = super::interface_runtime_batch3::make(0x3c1055);
    let mut perturbed = super::interface_runtime_batch3::make(0x3c1055);
    for purpose in [
        StreamId::Terrain,
        StreamId::Weather,
        StreamId::Combat,
        StreamId::Ai(0),
        StreamId::Ai(1),
    ] {
        for _ in 0..17 {
            perturbed.start.random.next_u64(purpose);
        }
    }
    let mut expected_control = control.start.random.clone();
    let mut expected_perturbed = perturbed.start.random.clone();
    let environment_seed = control.start.environment_seed;
    for state in [&mut control, &mut perturbed] {
        state.start.economy.players[0].mp = 10_000.0;
        assert_eq!(
            state
                .admit_support(0, "smoke", Point { x: 30.0, z: 25.0 }, Some(0.3))
                .unwrap(),
            None
        );
        let ids = state.world.game.units.keys().copied().collect::<Vec<_>>();
        // Exercise the actual strike consumer without unrelated simulation phases.
        for _ in 0..200 {
            state.step_support_strikes(&ids, 0.05).unwrap();
            if !state.world.game.smokes.is_empty() {
                break;
            }
        }
        assert!(!state.world.game.smokes.is_empty());
        assert!(state.world.game.strikes.is_empty());
        assert_eq!(state.start.environment_seed, environment_seed);
    }
    let clouds = control.world.game.smokes.len();
    assert_eq!(clouds, perturbed.world.game.smokes.len());
    for (a, b) in control
        .world
        .game
        .smokes
        .iter()
        .zip(&perturbed.world.game.smokes)
    {
        assert_eq!((a.x.to_bits(), a.z.to_bits()), (b.x.to_bits(), b.z.to_bits()));
        expected_control.next_fraction(StreamId::OffMap);
        expected_perturbed.next_fraction(StreamId::OffMap);
    }
    assert_eq!(control.start.random.snapshot(), expected_control.snapshot());
    assert_eq!(
        perturbed.start.random.snapshot(),
        expected_perturbed.snapshot()
    );
}
