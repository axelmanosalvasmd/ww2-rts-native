//! Native MAP rubble lifecycle and exact public projection checks.
use super::*;
use crate::{field_works::FortKind, orders::OrderKey, recipient_snapshot::DeliveryState};

#[test]
fn map34_recorded_fill_clears_rubble_and_survives_ticks_and_recipient_snapshots() {
    let c = 25 * 64 + 25;
    let mut rows = vec![".".repeat(64); 64];
    rows[25].replace_range(25..26, "R");
    let mut m = Match::new(&map(rows, json!([])), options()).unwrap();
    let worker = publish(&mut m, "engineer", 0, center(c));
    assert_eq!(
        m.world.game.flags[c] & (crate::game::VBLOCK | crate::game::COVER),
        crate::game::VBLOCK | crate::game::COVER
    );
    assert_eq!(
        terrain::cover_rank(&m.world.game, &m.rules, center(c), None),
        1
    );
    let command =
        Command::from_json(&json!({"t":"dig","ids":[worker],"kind":"fill","x":51,"z":51,"dir":0}))
            .unwrap();
    m.enqueue_command(
        OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 1,
        },
        command,
    )
    .unwrap();
    let mut delivery = DeliveryState::new(&m).unwrap();
    let mut saw_active = false;
    let mut saw_completed = false;
    for _ in 0..160 {
        let tick = m.advance_tick().unwrap();
        assert!(tick.commands.iter().all(|result| result.denial.is_none()));
        saw_active |= m.field_works.is_active(worker);
        let frame = delivery
            .capture_with_lists(&mut m, &[false; 2], &[false; 2], &[])
            .unwrap();
        frame.recipient(0).unwrap().into_json();
        frame.recipient(1).unwrap().into_json();
        if !m.field_works.is_active(worker) && saw_active {
            saw_completed = true;
        }
    }
    assert!(saw_active && saw_completed);
    assert_eq!(m.world.game.chars[c], b'.');
    assert_eq!(m.world.game.flags[c] & crate::game::VBLOCK, 0);
    assert!(!m.world.game.units[&worker].dig);
    assert!(m.story[0].built > 0.0);
    // Retirement must remove an active Fill job using the existing cancellation owner.
    m.world.set_cell(c, b'R').unwrap();
    assert_eq!(
        m.execute_dig(
            0,
            &[worker.into()],
            FortKind::Fill,
            center(c),
            Some(0.0),
            false
        )
        .unwrap(),
        None
    );
    m.world.game.units.get_mut(&worker).unwrap().hp = 0.0;
    m.advance_tick().unwrap();
    assert!(!m.world.game.units.contains_key(&worker));
    assert!(!m.field_works.is_active(worker));
    delivery
        .capture_with_lists(&mut m, &[false; 2], &[false; 2], &[])
        .unwrap()
        .recipient(0)
        .unwrap()
        .into_json();
}

#[test]
fn map34_fillability_world_scenery_and_slope_limited_surface_restore() {
    let mut source = json!({"name":"Fill queries","w":20,"h":20,
        "rows":vec![".".repeat(20);20], "heights":vec!["0".repeat(20);20],
        "spawns":[{"x":2,"y":2}],"points":[],
        "world":{"size":"huge","total":64,"regionCells":64,"regionMap":vec![0;400],
            "regions":(0..64).map(|id| json!({"id":id,"x":2+id%8*2,"y":2+id/8*2,"bounds":[0,0,20,20],"home":if id==0 {json!(0)} else {serde_json::Value::Null}})).collect::<Vec<_>>()}});
    for region in source["world"]["regions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .skip(1)
    {
        region.as_object_mut().unwrap().remove("home");
    }
    source["rows"][0] = json!("RF..................");
    let authored = AuthoritativeMap::from_json(&serde_json::to_vec(&source).unwrap()).unwrap();
    let mut world = World::from_map(&authored).unwrap();
    assert!(!world.fillable(0), "initial World rubble is scenery");
    assert!(!world.fillable(1), "initial ford is scenery");
    for ch in *b"+RF" {
        world.set_cell(21, ch).unwrap();
        assert!(world.fillable(21));
    }
    assert_eq!(world.fill_cell(0), Err(crate::world::WorldError::InvalidChange));
    assert_eq!(world.game.chars[0], b'R');
    world.set_cell(22, b'D').unwrap();
    world.set_cell(22, b'R').unwrap();
    world.fill_cell(22).unwrap();
    assert_eq!(world.game.chars[22], b'D');
    // A deep bowl is restored one reachable level at a time, without raising above the map.
    for cell in [210, 190, 230, 209, 211] {
        world
            .mutate_cell(
                cell,
                CellChange {
                    height: Some(-2),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    assert!(world.fillable(210));
    world.fill_cell(210).unwrap();
    assert_eq!(world.game.height.as_ref().unwrap()[210], -1);
    for cell in [190, 230, 209, 211] {
        world.fill_cell(cell).unwrap();
    }
    world.fill_cell(210).unwrap();
    assert_eq!(world.game.height.as_ref().unwrap()[210], 0);
    assert!(!world.fillable(210));
    assert!(!world.fillable(400));
}

#[test]
fn map24_public_sanitizer_preserves_underlying_materials_and_removes_private_mines_and_scripts() {
    let mut source = json!({"name":"Material public projection","w":20,"h":20,
        "rows":vec![".".repeat(20);20],"spawns":[{"x":1,"y":1},{"x":18,"y":18}],"points":[{"x":10,"y":10}],
        "worldVersion":2,"layers":{"ground":vec![".".repeat(20);20],"objects":vec![".".repeat(20);20],"mines":vec![".".repeat(20);20],
        "materials":[{"c":0.0,"material":"steel"},{"c":1.0,"material":"stone"},{"c":2,"material":"concrete"}],
        "objectMaterials":[{"c":0,"material":"steel"},{"c":2,"material":"stone"}],
        "mineMaterials":[{"c":0,"material":"steel"},{"c":1,"material":"steel"},{"c":2,"material":"steel"}],
        "groundMaterials":[{"c":0,"material":"road"},{"c":1,"material":"soil"}]},
        "triggers":[{"at":1,"say":"private"}],"scenario":{"private":"server only"}});
    source["rows"][0] = json!("NNN.................");
    source["layers"]["ground"][0] = json!("D...................");
    source["layers"]["objects"][0] = json!("N.B.................");
    source["layers"]["mines"][0] = json!(".NN.................");
    let before = source.clone();
    let public = crate::map::map_for_client(&source).unwrap();
    assert_eq!(source, before);
    assert_eq!(public["rows"][0], json!("D.B................."));
    assert_eq!(
        public["layers"]["objects"][0],
        json!("..B.................")
    );
    assert_eq!(public["layers"]["mines"], json!(vec![".".repeat(20); 20]));
    assert_eq!(
        public["layers"]["materials"],
        json!([{"c":1.0,"material":"stone"},{"c":2,"material":"concrete"}])
    );
    assert_eq!(
        public["layers"]["objectMaterials"],
        json!([{"c":2,"material":"stone"}])
    );
    assert_eq!(public["layers"]["mineMaterials"], json!([]));
    assert_eq!(
        public["layers"]["groundMaterials"],
        source["layers"]["groundMaterials"]
    );
    assert!(public.get("triggers").is_none() && public.get("scenario").is_none());
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&public).unwrap()).unwrap();
    assert!(map.layers().mines.iter().all(|m| *m == 0));
}
