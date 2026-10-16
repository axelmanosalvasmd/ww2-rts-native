use super::*;

#[test]
fn authored_objectives_refuse_unimplemented_missions_before_startup() {
    let mut data = json!({"version":1,"objectives":[],"groups":[],"triggers":[]});
    assert!(validate_definition(&data, 2, Mode::Conquest).is_ok());
    for mode in [
        Mode::World,
        Mode::Classic,
        Mode::Annihilation,
        Mode::Horde,
        Mode::Tutorial,
    ] {
        assert!(validate_definition(&data, 2, mode).is_err());
    }
    data["groups"] = json!([{"id":"g","units":[]}]);
    assert!(validate_definition(&data, 2, Mode::Conquest).is_err());
    data["groups"] = json!([]);
    data["triggers"] = json!([{"id":"t","scope":"match","condition":{"kind":"time","seconds":0},"actions":[{"kind":"reinforce"}]}]);
    assert!(validate_definition(&data, 2, Mode::Conquest).is_err());
    data["triggers"][0]["actions"] = json!([{"kind":"say","text":"OK","side":2}]);
    assert!(validate_definition(&data, 2, Mode::Conquest).is_err());
    data["triggers"][0]["actions"] = json!([{"kind":"say","text":{"es":"First","fr":"Second"}}]);
    assert!(validate_definition(&data, 2, Mode::Conquest).is_err());
    data["triggers"][0]["actions"][0]["text"] = json!({"en":"Default","es":"Translation"});
    assert!(validate_definition(&data, 2, Mode::Conquest).is_ok());
}
