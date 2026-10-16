use super::{
    tests::{add, ids, scene},
    *,
};

#[test]
fn base10_four_players_place_fuel_in_first_discovered_enemy_pair_order() {
    let mut m = scene(Mode::Classic, &[0, 1, 2, 3]);
    // Player 3 is nearer player 0 than player 2. Sorting pair keys would
    // incorrectly publish the 0/2 node before the 0/3 node.
    for (player, at) in m.start.players.iter_mut().zip([
        Point { x: 17.0, z: 17.0 },
        Point { x: 183.0, z: 183.0 },
        Point { x: 17.0, z: 183.0 },
        Point { x: 153.0, z: 17.0 },
    ]) {
        player.spawn = at;
    }
    m.setup_classic_nodes().unwrap();
    let first = m
        .elimination
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.fuel)
        .unwrap();
    let a = m.start.players[0].spawn;
    let b = m.start.players[3].spawn;
    let c = m.start.players[2].spawn;
    let midpoint = |p: Point| Point {
        x: (a.x + p.x) / 2.0,
        z: (a.z + p.z) / 2.0,
    };
    let distance = |p: Point| math::hypot(first.at.x - p.x, first.at.z - p.z);
    assert!(distance(midpoint(b)) < 36.0);
    assert!(distance(midpoint(c)) > 60.0);
    assert_eq!(
        m.elimination
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter(|n| !n.fuel)
            .count(),
        8
    );
}

#[test]
fn base10_node_search_skips_live_ground_troops_without_changing_walk_terrain() {
    let mut m = scene(Mode::Classic, &[0, 1]);
    let original = m.elimination.as_ref().unwrap().nodes[0].clone();
    let actor = add(&mut m, 0, "rifle", original.at);
    let occupied = m.world.game.cell_of(original.at.x, original.at.z).unwrap();
    let flags = m.world.game.flags.clone();
    m.setup_classic_nodes().unwrap();
    let displaced = &m.elimination.as_ref().unwrap().nodes[0];
    assert_ne!(displaced.cell, original.cell);
    assert!(
        !footprint(&m.world.game, displaced.cell, 2)
            .unwrap()
            .contains(&occupied)
    );
    assert_eq!(m.world.game.flags, flags);

    // Dead and transported squads do not reserve a node footprint.
    m.world.game.units.get_mut(&actor).unwrap().hp = 0.0;
    m.setup_classic_nodes().unwrap();
    assert_eq!(m.elimination.as_ref().unwrap().nodes[0].cell, original.cell);
    let unit = m.world.game.units.get_mut(&actor).unwrap();
    unit.hp = 1.0;
    unit.riding = true;
    m.setup_classic_nodes().unwrap();
    assert_eq!(m.elimination.as_ref().unwrap().nodes[0].cell, original.cell);
}

#[test]
fn base10_three_players_get_home_and_contested_rates_with_legal_footprints() {
    let m = scene(Mode::Classic, &[0, 1, 2]);
    let nodes = &m.elimination.as_ref().unwrap().nodes;
    assert_eq!(nodes.iter().filter(|n| !n.fuel).count(), 6);
    assert_eq!(nodes.iter().filter(|n| n.fuel).count(), 3);
    let catalog = Catalog::compiled().unwrap();
    let point_radius = number(catalog.table("CFG").unwrap(), "pointRadius").unwrap();
    for (index, node) in nodes.iter().enumerate() {
        assert_eq!(node.rate, if node.fuel { 1.5 } else { 2.5 });
        assert!(node.depot.is_none());
        let cells = footprint(&m.world.game, node.cell, 2).unwrap();
        assert_eq!(cells.len(), 4);
        assert_eq!(node.at, foot_center(&m.world.game, node.cell, 2));
        assert!(
            cells
                .iter()
                .all(|&c| m.world.game.flags[c] & crate::game::MOVE == 0)
        );
        for player in &m.start.players {
            assert!(math::hypot(node.at.x - player.spawn.x, node.at.z - player.spawn.z) >= 10.0);
        }
        for point in &m.start.points {
            assert!(
                math::hypot(node.at.x - point.at.x, node.at.z - point.at.z) >= point_radius + 3.0
            );
        }
        for earlier in &nodes[..index] {
            assert!(math::hypot(node.at.x - earlier.at.x, node.at.z - earlier.at.z) >= 10.0);
        }
    }
    for owner in 0..3 {
        assert_eq!(ids(&m, owner, "hq").len(), 1);
    }
}

#[test]
fn base10_fuel_search_improves_walking_balance_around_a_river_detour() {
    let mut m = scene(Mode::Classic, &[0, 1, 2]);
    for (player, at) in m.start.players.iter_mut().zip([
        Point { x: 17.0, z: 17.0 },
        Point { x: 183.0, z: 183.0 },
        Point { x: 17.0, z: 183.0 },
    ]) {
        player.spawn = at;
    }
    // The first enemy pair is on the left. Its direct midpoint lies on
    // a river whose only crossing is far to the right.
    for x in 0..70 {
        m.world.set_cell(50 * m.world.game.w + x, b'W').unwrap();
    }
    m.setup_classic_nodes().unwrap();
    let nodes = &m.elimination.as_ref().unwrap().nodes;
    let fuel = nodes.iter().find(|n| n.fuel).unwrap();
    let a = m.start.players[0].spawn;
    let b = m.start.players[2].spawn;
    let midpoint = Point {
        x: (a.x + b.x) / 2.0,
        z: (a.z + b.z) / 2.0,
    };
    let mut avoid = m
        .start
        .players
        .iter()
        .map(|p| (p.spawn, 10.0))
        .collect::<Vec<_>>();
    let catalog = Catalog::compiled().unwrap();
    let point_radius = number(catalog.table("CFG").unwrap(), "pointRadius").unwrap();
    avoid.extend(m.start.points.iter().map(|p| (p.at, point_radius + 3.0)));
    avoid.extend(nodes.iter().take_while(|n| !n.fuel).map(|n| (n.at, 10.0)));
    let direct = find_site(&m.world.game, midpoint, 2, &avoid).unwrap();
    let direct = foot_center(&m.world.game, direct, 2);
    let mut walking = ClassicWalking::new(&m.world.game, &m.rules).unwrap();
    let mut score = |at| {
        let da = walking.distance(&m.world.game, &m.rules, a, at).unwrap();
        let db = walking.distance(&m.world.game, &m.rules, b, at).unwrap();
        (da - db).abs() + 0.1 * (da + db)
    };
    let original_score = score(direct);
    let selected_score = score(fuel.at);
    assert!(original_score.is_finite() && selected_score.is_finite());
    assert!(
        selected_score < original_score,
        "fuel must improve route balance: {selected_score} versus {original_score}"
    );
}
