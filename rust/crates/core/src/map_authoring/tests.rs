use super::*;

#[test]
fn authored_selection_move_remove_prune_preserves_layers_and_rejects_unsupported_survivor() {
    // The two-cell lifecycle in reference/test-engine-authoring.js uses these exact values.
    let w = 24;
    let mut ground = vec![b'.'; w * w];
    let mut objects = ground.clone();
    ground[8 * w + 8] = b'D';
    ground[8 * w + 9] = b'M';
    objects[8 * w + 8] = b'B';
    objects[8 * w + 9] = b'B';
    objects[8 * w + 12] = b'H';
    let rows = |cells: &[u8]| -> Vec<String> {
        cells
            .chunks(w)
            .map(|row| String::from_utf8(row.to_vec()).unwrap())
            .collect()
    };
    let composed: Vec<_> = ground
        .iter()
        .zip(&objects)
        .map(|(ground, object)| if *object == b'.' { *ground } else { *object })
        .collect();
    let map = AuthoritativeMap::from_value(json!({
        "name":"Authoring fixture", "w":w, "h":w,
        "rows":rows(&composed), "worldVersion":2,
        "spawns":[{"x":2,"y":2},{"x":21,"y":21}],
        "points":[{"id":"point:0","x":12,"y":12}], "pointSequence":1,
        "layers":{
            "ground":rows(&ground), "objects":rows(&objects),
            "groundMaterials":[{"c":200,"material":"concrete"},{"c":201,"material":"soil"}],
            "objectMaterials":[{"c":200,"material":"wood"},{"c":201,"material":"stone"}],
        },
        "structures":[{"id":"edited_house_200","kind":"house","sections":[
            {"id":"cell_200","c":200,"hp":321,"material":"steel","anchor":true,"supports":["cell_201"]},
            {"id":"cell_201","c":201,"hp":250,"material":"stone","anchor":false,"supports":["cell_200"]},
        ]}],
    })).unwrap();
    let original = map.source().clone();
    let mut draft = MapAuthoring::from_map(&map);
    let mut picked = LayerSelection {
        ch: b'B',
        cells: vec![[8, 8], [9, 8]],
    };
    assert!(draft.move_selection(&mut picked, 2, 0));
    assert_eq!(picked.cells, [[10, 8], [11, 8]]);
    assert_eq!(&draft.layers().ground[200..202], b"DM");
    assert_eq!(&draft.layers().objects[200..202], b"..");
    assert_eq!(&draft.layers().objects[202..204], b"BB");
    assert_eq!(draft.layers().base_materials.get(&200).unwrap(), "concrete");
    assert_eq!(draft.layers().base_materials.get(&201).unwrap(), "soil");
    assert_eq!(draft.layers().object_materials.get(&202).unwrap(), "wood");
    assert_eq!(draft.layers().object_materials.get(&203).unwrap(), "stone");
    assert!(!draft.layers().object_materials.contains_key(&200));
    assert_eq!(draft.layers().materials.get(&200).unwrap(), "concrete");
    assert_eq!(draft.layers().materials.get(&202).unwrap(), "wood");
    let sections = &draft.structures()[0].sections;
    assert_eq!(draft.structures()[0].id, "edited_house_200");
    assert_eq!(sections[0].id, "cell_200");
    assert_eq!(sections[0].cell, 202);
    assert_eq!(sections[0].hp, 321.0);
    assert_eq!(sections[0].material, "steel");
    assert!(sections[0].anchor);
    assert_eq!(sections[0].supports, ["cell_201"]);
    assert_eq!(sections[1].cell, 203);
    assert_eq!(sections[1].supports, ["cell_200"]);
    draft.validate().unwrap();
    let round_trip =
        AuthoritativeMap::from_json(&serde_json::to_vec(&draft.document()).unwrap()).unwrap();
    assert_eq!(round_trip.layers(), draft.layers());
    assert_eq!(round_trip.structures(), draft.structures());
    assert_eq!(map.source(), &original);

    let before = draft.document();
    let selection_before = picked.clone();
    assert!(!draft.move_selection(&mut picked, 30, 0));
    assert_eq!(draft.document(), before);
    assert_eq!(picked, selection_before);
    // One target overlaps the source and the other meets an occupied object.
    assert!(!draft.move_selection(&mut picked, 1, 0));
    assert_eq!(draft.document(), before);
    assert_eq!(picked, selection_before);

    assert!(draft.remove_selection(&LayerSelection {
        ch: b'B',
        cells: vec![[10, 8]]
    }));
    draft.prune_structures();
    assert_eq!(draft.structures()[0].sections.len(), 1);
    let survivor = &draft.structures()[0].sections[0];
    assert_eq!(survivor.id, "cell_201");
    assert_eq!(survivor.cell, 203);
    assert_eq!(survivor.hp, 250.0);
    assert_eq!(survivor.material, "stone");
    assert!(!survivor.anchor);
    assert!(survivor.supports.is_empty());
    assert_eq!(draft.layers().objects[202], b'.');
    assert!(!draft.layers().object_materials.contains_key(&202));
    assert_eq!(
        draft.validate().unwrap_err().to_string(),
        "every starting section needs a path to an anchor"
    );
    assert!(draft.remove_selection(&LayerSelection {
        ch: b'B',
        cells: vec![[11, 8]]
    }));
    draft.prune_structures();
    assert!(draft.structures().is_empty());
    draft.validate().unwrap();
}
