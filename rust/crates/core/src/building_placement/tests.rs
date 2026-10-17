use super::*;
use crate::{map::AuthoritativeMap, world::World};
use serde_json::json;

fn scene() -> (World, Rules) {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Placement component", "w":40,"h":40,
            "rows":vec![".".repeat(40);40], "heights":vec!["0".repeat(40);40],
        "spawns":[{"x":3,"y":3},{"x":36,"y":36}], "points":[{"x":20,"y":20}]
        }))
        .unwrap(),
    )
    .unwrap();
    (World::from_map(&map).unwrap(), Rules::compiled().unwrap())
}

fn context<'a>(game: &'a Game, rules: &'a Rules) -> Context<'a> {
    Context {
        game,
        rules,
        mode: Mode::Conquest,
        logistics: false,
        nodes: &[],
        owned_cells: None,
        rail_yard: None,
    }
}

#[test]
fn base27_building_placement_preserves_footprint_rounding_and_reason_precedence() {
    // Literal Source sim.js:3277-3313: size 3 at (20,20) rounds (8.5,8.5) up.
    let (mut world, rules) = scene();
    let at = Point { x: 20.0, z: 20.0 };
    let expected_cells = vec![369, 370, 371, 409, 410, 411, 449, 450, 451];
    let clear = check(&context(&world.game, &rules), "barracks", at, |_| Ok(true)).unwrap();
    assert_eq!(
        clear,
        Placement {
            reason: None,
            at: Point { x: 21.0, z: 21.0 },
            cell: Some(369),
            cells: Some(expected_cells.clone()),
            node: None,
        }
    );
    world.game.chars[369] = b'K';
    let unseen = check(&context(&world.game, &rules), "barracks", at, |_| Ok(false)).unwrap();
    assert_eq!(unseen.reason, Some(Reason::NotVisible));
    assert_eq!(unseen.cell, clear.cell);
    assert_eq!(unseen.cells, clear.cells);
    assert_eq!(
        check(&context(&world.game, &rules), "barracks", at, |_| Ok(true))
            .unwrap()
            .reason,
        Some(Reason::Blocked)
    );

    let owned = expected_cells.iter().copied().collect::<BTreeSet<_>>();
    let yard = [369].into_iter().collect();
    let mut c = context(&world.game, &rules);
    c.mode = Mode::World;
    c.rail_yard = Some(&yard);
    assert_eq!(
        check(&c, "barracks", at, |_| Ok(false)).unwrap().reason,
        Some(Reason::NotVisible)
    );
    // The click is visible; one footprint cell is not. Sight still masks territory/rail/terrain.
    assert_eq!(
        check(&c, "barracks", at, |p| Ok(p != Point { x: 19.0, z: 19.0 }))
            .unwrap()
            .reason,
        Some(Reason::NotVisible)
    );
    assert_eq!(
        check(&c, "barracks", at, |_| Ok(true)).unwrap().reason,
        Some(Reason::Territory)
    );
    c.owned_cells = Some(&owned);
    assert_eq!(
        check(&c, "barracks", at, |_| Ok(true)).unwrap().reason,
        Some(Reason::Rail)
    );
    c.rail_yard = None;
    assert_eq!(
        check(&c, "barracks", at, |_| Ok(true)).unwrap().reason,
        Some(Reason::Blocked)
    );
    let offmap = check(&c, "barracks", Point { x: 80.0, z: 80.0 }, |_| Ok(true)).unwrap();
    assert_eq!(offmap.reason, Some(Reason::Blocked));
    assert!(offmap.cells.is_none());
}

#[test]
fn base27_shipyard_coast_precedes_stamping_and_accepts_water_two_cells_away() {
    let (mut world, rules) = scene();
    let at = Point { x: 20.0, z: 20.0 };
    world.game.chars[369] = b'K';
    assert_eq!(
        check(&context(&world.game, &rules), "shipyard", at, |_| Ok(true))
            .unwrap()
            .reason,
        Some(Reason::Blocked)
    );
    world.game.naval = true;
    assert_eq!(
        check(&context(&world.game, &rules), "shipyard", at, |_| Ok(true))
            .unwrap()
            .reason,
        Some(Reason::Coast)
    );
    // Footprint starts at x=9; x=7 is in the 2-cell coast search, x=6 is outside it.
    world.game.flags[9 * 40 + 6] &= !LAND;
    assert_eq!(
        check(&context(&world.game, &rules), "shipyard", at, |_| Ok(true))
            .unwrap()
            .reason,
        Some(Reason::Coast)
    );
    world.game.flags[9 * 40 + 7] &= !LAND;
    assert_eq!(
        check(&context(&world.game, &rules), "shipyard", at, |_| Ok(true))
            .unwrap()
            .reason,
        Some(Reason::Blocked)
    );
    world.game.chars[369] = b'.';
    assert_eq!(
        check(&context(&world.game, &rules), "shipyard", at, |_| Ok(true))
            .unwrap()
            .reason,
        None
    );
}

#[test]
fn base12_depot_uses_nearest_free_node_stable_ties_and_hidden_claimed_refusal() {
    let (world, rules) = scene();
    let at = Point { x: 20.0, z: 20.0 };
    let mut nodes = [
        Node {
            cell: 409,
            at: Point { x: 20.0, z: 22.0 },
            claimed: true,
        },
        Node {
            cell: 329,
            at: Point { x: 20.0, z: 18.0 },
            claimed: false,
        },
        Node {
            cell: 368,
            at: Point { x: 18.0, z: 20.0 },
            claimed: false,
        },
    ];
    let c = Context {
        nodes: &nodes,
        mode: Mode::Classic,
        ..context(&world.game, &rules)
    };
    let placement = check(&c, "depot", at, |_| Ok(true)).unwrap();
    assert_eq!(placement.reason, None);
    assert_eq!(placement.node, Some(1));
    assert_eq!(placement.cell, Some(329));
    assert_eq!(placement.at, Point { x: 20.0, z: 18.0 });
    assert_eq!(placement.cells, Some(vec![329, 330, 369, 370]));
    // A nearer claimed node is skipped; equally distant free nodes retain input order.
    nodes[0].at = at;
    nodes[1].claimed = true;
    nodes[2].claimed = true;
    let c = Context {
        nodes: &nodes,
        mode: Mode::Classic,
        ..context(&world.game, &rules)
    };
    assert_eq!(
        check(&c, "depot", at, |_| Ok(true)).unwrap().reason,
        Some(Reason::Blocked)
    );
    assert_eq!(
        check(&c, "depot", at, |_| Ok(false)).unwrap().reason,
        Some(Reason::NotVisible)
    );
    nodes[0].claimed = false;
    nodes[0].at = Point { x: 28.0, z: 20.0 };
    let c = Context {
        nodes: &nodes,
        mode: Mode::Classic,
        ..context(&world.game, &rules)
    };
    assert_eq!(check(&c, "depot", at, |_| Ok(true)).unwrap().node, Some(0));
    nodes[0].at.x = 28.0001;
    let c = Context {
        nodes: &nodes,
        mode: Mode::Classic,
        ..context(&world.game, &rules)
    };
    assert_eq!(
        check(&c, "depot", at, |_| Ok(true)).unwrap().reason,
        Some(Reason::Blocked)
    );
}

#[test]
fn base12_nodes_block_other_buildings_even_when_claimed_or_unseen() {
    let (world, rules) = scene();
    let nodes = [Node {
        cell: 369,
        at: Point { x: 20.0, z: 20.0 },
        claimed: true,
    }];
    let c = Context {
        nodes: &nodes,
        mode: Mode::Classic,
        ..context(&world.game, &rules)
    };
    let at = Point { x: 20.0, z: 20.0 };
    assert_eq!(
        check(&c, "barracks", at, |_| Ok(false)).unwrap().reason,
        Some(Reason::NotVisible)
    );
    assert_eq!(
        check(&c, "barracks", at, |_| Ok(true)).unwrap().reason,
        Some(Reason::Blocked)
    );
    assert_eq!(
        check(&c, "barracks", Point { x: 28.0, z: 28.0 }, |_| Ok(true))
            .unwrap()
            .reason,
        None
    );
}

#[test]
fn base27_stamping_checks_grade_and_living_ground_occupancy() {
    let (mut world, rules) = scene();
    let at = Point { x: 20.0, z: 20.0 };
    let reason = |game: &Game| {
        check(&context(game, &rules), "barracks", at, |_| Ok(true))
            .unwrap()
            .reason
    };
    world.game.height.as_mut().unwrap()[369] = 1;
    assert_eq!(reason(&world.game), None);
    world.game.height.as_mut().unwrap()[369] = 2;
    assert_eq!(reason(&world.game), Some(Reason::Blocked));
    world.game.height.as_mut().unwrap()[369] = 0;
    let rifle = crate::unit_spawn::initial_unit(
        &rules,
        Catalog::compiled().unwrap(),
        rules.kind("rifle").unwrap(),
        1,
        0,
        at,
    )
    .unwrap();
    world.game.units.insert(1, rifle);
    assert_eq!(reason(&world.game), Some(Reason::Blocked));
    world.game.units.get_mut(&1).unwrap().hp = 0.0;
    assert_eq!(reason(&world.game), None);
    let unit = world.game.units.get_mut(&1).unwrap();
    unit.hp = 1.0;
    unit.riding = true;
    assert_eq!(reason(&world.game), None);
    let unit = world.game.units.get_mut(&1).unwrap();
    unit.riding = false;
    unit.kind = rules.kind("fighter").unwrap();
    assert_eq!(reason(&world.game), None);
    let unit = world.game.units.get_mut(&1).unwrap();
    unit.kind = rules.kind("rifle").unwrap();
    unit.x = f64::NAN;
    unit.z = 1.0;
    // JS NaN never equals any footprint index; Rust's float-to-usize cast must not create cell 0.
    assert_eq!(
        check(
            &context(&world.game, &rules),
            "barracks",
            Point { x: 2.0, z: 2.0 },
            |_| Ok(true)
        )
        .unwrap()
        .reason,
        None
    );
}

#[test]
fn base27_query_keeps_mode_logistics_and_nonfinite_coordinate_rules() {
    let (world, rules) = scene();
    let at = Point { x: 20.0, z: 20.0 };
    for mode in [
        Mode::Default,
        Mode::Conquest,
        Mode::Assault,
        Mode::Annihilation,
        Mode::Horde,
        Mode::World,
        Mode::Classic,
        Mode::Tutorial,
        Mode::Other,
    ] {
        let c = Context {
            mode,
            ..context(&world.game, &rules)
        };
        // World without territory can select the HQ but may not stamp it; other non-skirmish modes cannot select it.
        assert_eq!(
            check(&c, "hq", at, |_| Ok(true)).unwrap().reason,
            if mode == Mode::World {
                Some(Reason::Territory)
            } else if mode.is_skirmish() {
                None
            } else {
                Some(Reason::Blocked)
            }
        );
    }
    let mut c = context(&world.game, &rules);
    assert_eq!(
        check(&c, "supplycache", at, |_| Ok(true)).unwrap().reason,
        Some(Reason::Blocked)
    );
    c.logistics = true;
    assert_eq!(
        check(&c, "supplycache", at, |_| Ok(true)).unwrap().reason,
        None
    );
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            check(&c, "barracks", Point { x: bad, z: 20.0 }, |_| Ok(true)).unwrap(),
            Placement::denied(Point { x: 0.0, z: 20.0 }, Reason::Blocked)
        );
    }
    // JS Math.round(-0.5) is negative zero, which is still the first footprint column.
    let rounded = check(&c, "barracks", Point { x: 2.0, z: 2.0 }, |_| Ok(true)).unwrap();
    assert_eq!(rounded.cell, Some(0));
    assert_eq!(rounded.at, Point { x: 3.0, z: 3.0 });
}

#[test]
fn base27_recipient_uses_delivered_weather_aircraft_and_remembered_terrain_only() {
    use crate::client_view::ClientView;
    let view = |kind: &str, weather: &str, flags: u32, cells: serde_json::Value| {
        let mut view = ClientView::new();
        view.apply(
            &json!({"t":"start", "you":0,"teams":[0,1],"map":{
            "w":40,"h":40,"rows":vec![".".repeat(40);40]},"cells":cells})
            .to_string(),
        )
        .unwrap();
        view.apply(
            &json!({"t":"s", "all":true,"weather":[weather],"wx":[0,0,0,0],
            "units":[[1,kind,0,5,5,0,0,100,0,0,0,0,flags,0,1]]})
            .to_string(),
        )
        .unwrap();
        view
    };
    let rules = Rules::compiled().unwrap();
    let rifle = &rules.kinds[rules.kind("rifle").unwrap()];
    let fog = rules.weather[rules.weather_kind("fog").unwrap()].sight;
    let x = 5.0 + ((rifle.vision * (1.0 + fog) / 2.0) / 2.0).floor() * 2.0;
    let spot = Point { x, z: 5.0 };
    assert_eq!(
        check_recipient(&view("rifle", "clear", 0, json!([])), "barracks", spot)
            .unwrap()
            .reason,
        None
    );
    assert_eq!(
        check_recipient(&view("rifle", "fog", 0, json!([])), "barracks", spot)
            .unwrap()
            .reason,
        Some(Reason::NotVisible)
    );
    // A wall lies between the recipient's observer and the proposed footprint.
    let wall = json!(
        (0..40)
            .map(|z| json!([z * 40 + 8, "K", 0, 0]))
            .collect::<Vec<_>>()
    );
    let spot = Point { x: 23.0, z: 5.0 };
    assert_eq!(
        check_recipient(&view("rifle", "clear", 0, wall.clone()), "barracks", spot)
            .unwrap()
            .reason,
        Some(Reason::NotVisible)
    );
    assert_eq!(
        check_recipient(&view("fighter", "fog", 0, wall.clone()), "barracks", spot)
            .unwrap()
            .reason,
        None
    );
    assert_eq!(
        check_recipient(&view("fighter", "clear", 512, wall), "barracks", spot)
            .unwrap()
            .reason,
        Some(Reason::NotVisible)
    );
    // Remembered occupancy is used without an authoritative terrain fallback.
    let changed = json!([[2 * 40 + 11, "K", 0, 0]]);
    assert_eq!(
        check_recipient(&view("fighter", "clear", 0, changed), "barracks", spot)
            .unwrap()
            .reason,
        Some(Reason::Blocked)
    );
}

#[test]
fn base27_recipient_nodes_snap_reserve_and_claim_only_delivered_depots() {
    use crate::client_view::ClientView;
    let mut view = ClientView::new();
    view.apply(
        &json!({"t":"start", "you":0,"teams":[0,1],"map":{
        "w":40,"h":40,"rows":vec![".".repeat(40);40]}})
        .to_string(),
    )
    .unwrap();
    let observer = json!([1, "fighter", 0, 5, 5, 0, 0, 100, 0, 0, 0, 0, 0, 0, 1]);
    let mut snapshot =
        json!({"t":"s","all":true,"units":[observer],"nodes":[[20,20,1,0],[24,20,1,0]]});
    view.apply(&snapshot.to_string()).unwrap();
    let at = Point { x: 21.0, z: 20.0 };
    let place = check_recipient(&view, "depot", at).unwrap();
    assert_eq!(place.reason, None);
    assert_eq!(place.at, Point { x: 20.0, z: 20.0 });
    assert_eq!(place.node, Some(0));
    assert_eq!(place.cells, Some(vec![369, 370, 409, 410]));
    assert_eq!(
        check_recipient(&view, "barracks", at).unwrap().reason,
        Some(Reason::Blocked)
    );
    snapshot["units"].as_array_mut().unwrap().push(json!([
        2, "depot", 1, 20, 20, 0, 0, 100, 0, 0, 0, 0, 0, 0, 1
    ]));
    view.apply(&snapshot.to_string()).unwrap();
    let next = check_recipient(&view, "depot", at).unwrap();
    assert_eq!(next.reason, None);
    assert_eq!(next.node, Some(1));
    assert_eq!(next.at, Point { x: 24.0, z: 20.0 });
    snapshot["nodes"] = json!([[20, 20, 1, 0]]);
    view.apply(&snapshot.to_string()).unwrap();
    assert_eq!(
        check_recipient(&view, "depot", at).unwrap().reason,
        Some(Reason::Blocked)
    );
}
