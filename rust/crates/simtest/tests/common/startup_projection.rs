use crate::startup::row;
use serde_json::{Value, json};
pub fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
pub fn runs(values: impl IntoIterator<Item = Value>) -> Value {
    let mut out: Vec<[Value; 2]> = Vec::new();
    for value in values {
        if let Some(last) = out.last_mut().filter(|r| r[0] == value) {
            last[1] = json!(last[1].as_u64().unwrap() + 1);
        } else {
            out.push([value, json!(1)]);
        }
    }
    json!(out)
}
pub fn equivalent(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, a)| b.get(k).is_some_and(|b| equivalent(a, b)))
        }
        _ => a == b,
    }
}
pub fn terrain(instance: &ww2_core::match_runtime::Match) -> Value {
    use ww2_core::world::Recipient;
    let w = &instance.world;
    let g = &w.game;
    let buildings: Vec<_> = w
        .structures
        .iter()
        .filter(|s| s.kind == "building")
        .map(|s| {
            let sections: Vec<_> = s
                .section_ids
                .iter()
                .zip(&s.cells)
                .map(|(id, c)| {
                    let (m, h) = w.section_by_id(id).unwrap();
                    json!([
                        m.id,
                        m.structure_id,
                        c,
                        bits(h.hp),
                        bits(h.max_hp.unwrap()),
                        m.material,
                        m.anchor,
                        m.supports,
                        "intact",
                        m.rubble
                    ])
                })
                .collect();
            json!([s.id, s.entity_id, s.cells, sections])
        })
        .collect();
    json!({"chars":runs(g.chars.iter().map(|c|json!(char::from(*c).to_string()))),
        "ground":runs(g.ground.as_ref().unwrap().iter().map(|c|json!(char::from(*c).to_string()))),
        "objects":runs(g.objects.as_ref().unwrap().iter().map(|c|json!(char::from(*c).to_string()))),
        "flags":runs(g.flags.iter().map(|v|json!(v))),"height":g.height.as_ref().map(|h|runs(h.iter().map(|v|json!(v)))),
        "hp":runs(g.cell_hp.iter().map(|v|json!(v.to_bits()))),"wear":w.cell_log().iter().map(|r|json!([r.cell,g.wear.as_ref().unwrap()[r.cell].to_bits()])).collect::<Vec<_>>(),
        "state":runs(w.cell_state.iter().map(|v|json!(v))),
        "versions":[g.terrain_version as u64,w.versions.obstruction,w.versions.infantry,w.versions.vehicle,w.versions.naval],
        "log":w.cell_log().iter().map(row).collect::<Vec<_>>(),"newCells":w.new_cells().iter().map(row).collect::<Vec<_>>(),"journal":w.journal(),
        "buildingCells":instance.building_cells.iter().map(|(c,b)|json!([c,b.unit,b.owner,bits(b.at.x),bits(b.at.z)])).collect::<Vec<_>>(),
        "pending":(0..instance.start.players.len()).map(|i|w.pending(Recipient::Player(i)).unwrap()).collect::<Vec<_>>(),"buildings":buildings})
}
