use super::*;

#[test]
fn if26_handoff_consumer_reuses_quarter_metre_keys_and_resets_on_epoch_and_geometry() {
    let width = 20;
    let mut flags = vec![0; width * 20];
    for y in 0..20 {
        flags[y * width + 10] = MOVE;
    }
    let near = vec![0; flags.len()];
    let empty = vec![false; flags.len()];
    let terrain = PathTerrain {
        navigation: TerrainView {
            width,
            height: 20,
            flags: &flags,
            heights: None,
            world_known: false,
            knowledge_id: 1,
            versions: Versions::default(),
            changed: None,
            changes: &[],
        },
        near_walls: &near,
        known_mines: &empty,
        fires: &empty,
        bunkers: &[],
        wear: None,
        wetness: 0.0,
        roads: false,
    };
    let rules = crate::rules::Rules::compiled().unwrap();
    let rifle = &rules.kinds[rules.kind("rifle").unwrap()];
    let mut routes = SupplyRouting::default();
    let mut request = Handoff {
        tick: 0,
        start: Point { x: -0.0, z: 10.0 },
        store: Point { x: 30.0, z: 10.0 },
        radius: 32.0,
    };
    assert!(!routes.handoff(request, terrain, rifle, &[]).unwrap());
    assert!(routes.handoffs.contains_key(&[0, 40, 120, 40, 128]));
    request.start.x = 0.0;
    request.store.x = 30.124;
    assert!(!routes.handoff(request, terrain, rifle, &[]).unwrap());
    assert_eq!(routes.handoffs.len(), 1);
    request.store.x = 30.125;
    assert!(!routes.handoff(request, terrain, rifle, &[]).unwrap());
    assert!(routes.handoffs.contains_key(&[0, 40, 121, 40, 128]));
    assert_eq!(routes.handoffs.len(), 2);
    request.tick = 40;
    assert!(!routes.handoff(request, terrain, rifle, &[]).unwrap());
    assert_eq!(routes.handoffs.len(), 1);
    request.store.x = 30.0;
    let updated = PathTerrain {
        navigation: TerrainView {
            versions: Versions { obstruction: 1, ..Default::default() },
            ..terrain.navigation
        },
        ..terrain
    };
    assert!(!routes.handoff(request, updated, rifle, &[]).unwrap());
    assert_eq!(routes.handoffs.len(), 1);
    assert!(routes.handoffs.contains_key(&[0, 40, 120, 40, 128]));
    request.start.x = f64::NAN;
    assert!(routes.handoff(request, updated, rifle, &[]).is_err());
    assert_eq!(routes.handoffs.len(), 1);
}
