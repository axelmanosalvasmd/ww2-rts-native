use serde_json::{Value, json};
pub fn row(r: &ww2_core::world::CellRow) -> Value {
    let d = &r.data;
    let mut data = json!({"ground":char::from(d.ground).to_string(),"object":char::from(d.object).to_string(),"material":d.material,"groundMaterial":d.ground_material,"groundWear":d.ground_wear});
    if let Some(front) = d.trench_front {
        data["trenchFront"] = json!(front);
    }
    if d.mine {
        data["mine"] = json!(true);
        if let Some(m) = &d.mine_material {
            data["mineMaterial"] = json!(m);
        }
    }
    if let Some(s) = &d.section {
        data["section"] = json!({"id":s.id,"structureId":s.structure_id,"state":s.state,"hp":s.hp,"maxHp":s.max_hp,"material":s.material,"anchor":s.anchor});
    }
    json!([
        r.cell,
        char::from(r.ch).to_string(),
        r.height,
        r.state,
        data
    ])
}
