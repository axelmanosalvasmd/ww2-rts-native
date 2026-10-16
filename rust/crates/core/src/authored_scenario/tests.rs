use super::*;

#[test]
fn authored_scenarios_validate_ordinary_sides_and_localization_before_startup() {
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
    assert!(validate_definition(&data, 2, Mode::Conquest).is_ok());
    data["triggers"] = json!([{"id":"t","scope":"match","condition":{"kind":"time","seconds":0},"actions":[{"kind":"reinforce","group":"g","side":0,"at":[4,4],"expires":10,"roster":[{"type":"rifle","count":1}],"order":{"kind":"hold"}}]}]);
    assert!(validate_definition(&data, 2, Mode::Conquest).is_ok());
    data["triggers"][0]["actions"] = json!([{"kind":"say","text":"OK","side":2}]);
    assert!(validate_definition(&data, 2, Mode::Conquest).is_err());
    data["triggers"][0]["actions"] = json!([{"kind":"say","text":{"es":"First","fr":"Second"}}]);
    assert!(validate_definition(&data, 2, Mode::Conquest).is_err());
    data["triggers"][0]["actions"][0]["text"] = json!({"en":"Default","es":"Translation"});
    assert!(validate_definition(&data, 2, Mode::Conquest).is_ok());
    data["triggers"][0]["condition"] = json!({"kind":"ownership","target":"region:1","side":0});
    assert!(validate_definition(&data, 2, Mode::Conquest).is_err());
    data["triggers"][0]["condition"]["target"] = json!("ordinary-point");
    assert!(validate_definition(&data, 2, Mode::Conquest).is_ok());
}

#[test]
fn localization_side_key_is_text_and_not_an_authored_side_reference() {
    let data = json!({"version":1.0,"groups":[],"objectives":[],"triggers":[
        {"id":"intro","scope":"match","condition":{"kind":"time","seconds":0.0},
         "actions":[{"kind":"say","text":{"side":"A translation"}}]}]});
    assert!(validate_definition(&data, 2, Mode::Conquest).is_ok());
}
