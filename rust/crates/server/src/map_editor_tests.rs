use super::*;
use crate::maps::serve;
use axum::{
    Extension,
    body::{Body, to_bytes},
    extract::OriginalUri,
    http::{HeaderMap, Method, StatusCode},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

pub(crate) struct Dir(pub(crate) PathBuf);
impl Dir {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ww2-save2-editor-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
pub(crate) fn raw_map() -> Value {
    let mut rows = vec![".".repeat(24); 24];
    rows[0].replace_range(1..2, "N");
    json!({"name":"Editor map","w":24,"h":24,"rows":rows,
        "spawns":[{"x":3,"y":3,"extra":"drop"},{"x":20,"y":20}],
        "points":[{"id":"front","x":12,"y":12,"extra":"drop"}],
        "defend":[0],"trenchFacing":true,"triggers":[{"at":6000,"say":"Private trigger","extra":"drop"}],"extra":"drop"})
}
async fn request(
    editor: &MapEditor,
    method: Method,
    path: &str,
    password: Option<&str>,
    body: Vec<u8>,
) -> (StatusCode, Vec<u8>) {
    let mut headers = HeaderMap::new();
    if let Some(password) = password {
        headers.insert("x-edit-password", password.parse().unwrap());
    }
    let response = serve(
        OriginalUri(path.parse().unwrap()),
        Extension(Some(editor.clone())),
        method,
        headers,
        Body::from(body),
    )
    .await;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap();
    (status, bytes.to_vec())
}
#[tokio::test]
async fn net12_editor_http_auth_limits_validation_whitelist_and_raw_reload() {
    let dir = Dir::new();
    let editor = MapEditor::open(dir.0.clone(), "test-password").unwrap();
    for password in [None, Some("wrong")] {
        let (status, body) = request(
            &editor,
            Method::POST,
            "/maps/saved",
            password,
            b"bad".to_vec(),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            json!({"error":"wrong password"})
        );
    }
    let mut invalid = raw_map();
    invalid["w"] = json!(1);
    for (body, status, message) in [
        (b"{".to_vec(), StatusCode::BAD_REQUEST, "bad JSON"),
        (
            vec![b' '; 8 * 1024 * 1024 + 1],
            StatusCode::PAYLOAD_TOO_LARGE,
            "map too large",
        ),
        (
            serde_json::to_vec(&invalid).unwrap(),
            StatusCode::BAD_REQUEST,
            "size must be 20-1024 cells",
        ),
    ] {
        let (actual, body) = request(
            &editor,
            Method::POST,
            "/maps/saved",
            Some("test-password"),
            body,
        )
        .await;
        assert_eq!(actual, status);
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            json!({"error":message})
        );
        assert!(editor.raw("saved").is_none());
        assert!(!dir.0.join("saved.json").exists());
    }
    let source = raw_map();
    let bytes = serde_json::to_vec(&source).unwrap();
    let expected: Value =
        serde_json::from_slice(&ww2_core::map_save::prepare_map_save(&bytes).unwrap()).unwrap();
    let (status, body) = request(
        &editor,
        Method::POST,
        "/maps/saved.json",
        Some("test-password"),
        bytes,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"ok":true})
    );
    assert!(expected.get("extra").is_none());
    assert!(expected["spawns"][0].get("extra").is_none());
    assert!(expected["points"][0].get("extra").is_none());
    assert_eq!(expected["points"][0]["vp"], 1);
    assert_eq!(expected["points"][0]["mp"], 1);
    for password in [Some("test-password"), None] {
        let (status, body) = request(&editor, Method::GET, "/maps/saved", password, vec![]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            if password.is_some() {
                expected.clone()
            } else {
                ww2_core::map::map_for_client(&expected).unwrap()
            }
        );
    }
    assert_eq!(
        request(&editor, Method::GET, "/maps/saved", Some("wrong"), vec![]).await,
        (StatusCode::FORBIDDEN, b"wrong password".to_vec())
    );
    assert_eq!(
        request(
            &editor,
            Method::POST,
            "/maps/%2fsecret",
            Some("test-password"),
            vec![]
        )
        .await,
        (StatusCode::BAD_REQUEST, b"bad map name".to_vec())
    );
    let mut boundary = source.to_string().into_bytes();
    boundary.resize(8 * 1024 * 1024, b' ');
    assert_eq!(
        request(
            &editor,
            Method::POST,
            "/maps/saved",
            Some("test-password"),
            boundary
        )
        .await
        .0,
        StatusCode::OK
    );
    let reloaded = MapEditor::open(dir.0.clone(), "test-password").unwrap();
    assert_eq!(reloaded.raw("saved"), Some(expected));
    let (_, body) = request(&reloaded, Method::GET, "/maps", None, vec![]).await;
    assert!(
        serde_json::from_slice::<Vec<String>>(&body)
            .unwrap()
            .contains(&"saved".into())
    );
}
#[test]
fn net13_password_file_is_reused_and_override_does_not_expose_or_replace_it() {
    let dir = Dir::new();
    let file = dir.0.join(".edit-password");
    let first = MapEditor::from_password_file(dir.0.join("maps"), None, &file).unwrap();
    let password = fs::read_to_string(&file).unwrap();
    assert_eq!(password.len(), 12);
    assert!(
        password
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    assert!(first.authorized(password.as_bytes()));
    let second = MapEditor::from_password_file(dir.0.join("maps"), None, &file).unwrap();
    assert!(second.authorized(password.as_bytes()));
    let third = MapEditor::from_password_file(dir.0.join("maps"), Some("override"), &file).unwrap();
    assert!(third.authorized(b"override"));
    assert!(!third.authorized(password.as_bytes()));
    assert_eq!(fs::read_to_string(&file).unwrap(), password);
    assert!(!format!("{third:?}").contains("override"));
    fs::write(&file, "  trimmed secret\n").unwrap();
    let trimmed = MapEditor::from_password_file(dir.0.join("maps"), Some(""), &file).unwrap();
    assert!(trimmed.authorized(b"trimmed secret"));
    assert!(!trimmed.authorized(b"  trimmed secret\n"));
    let absent = dir.0.join("absent-password");
    let env =
        MapEditor::from_password_file(dir.0.join("maps"), Some(" env secret "), &absent).unwrap();
    assert!(env.authorized(b" env secret "));
    assert!(!env.authorized(b"env secret"));
    assert!(
        !absent.exists(),
        "environment override does not create a password file"
    );
    for given in [
        b"".as_slice(),
        b" env secret",
        b" env secret  ",
        b" other pass ",
        &[0xff, 0xfe],
    ] {
        assert!(!env.authorized(given));
    }
}

// The expected document spells out server.js:106-126 independently of the
// implementation's projection. Unknown fields exercise every nested boundary.
fn complete_whitelist_map() -> (Value, Value) {
    let mut ground = vec![".".repeat(24); 24];
    ground[5].replace_range(11..12, "W");
    let mut objects = vec![".".repeat(24); 24];
    objects[0].replace_range(1..2, "N");
    objects[5].replace_range(10..12, "B=");
    let mut rows = objects.clone();
    let mines = vec![".".repeat(24); 24];
    rows[5].replace_range(11..12, "=");
    let clean = json!({"name":"Full whitelist","w":24,"h":24,"rows":rows,
        "heights":vec!["0".repeat(24);24],
        "spawns":[{"x":3,"y":3},{"x":20,"y":20},{"x":3,"y":20,"assault":true}],
        "points":[{"id":"point:0","x":12,"y":12,"vp":0,"mp":1,"kind":"radio"},
            {"id":"point:1","x":15,"y":15,"vp":1,"mp":2,"kind":"depot","needs":0}],
        "pointSequence":2,"worldVersion":2,
        "layers":{"ground":ground,"objects":objects,"mines":mines,
            "materials":[{"c":130,"material":"stone"}],
            "groundMaterials":[{"c":130,"material":"soil"}],
            "objectMaterials":[{"c":130,"material":"steel"}],
            "mineMaterials":[{"c":1,"material":"wood"}]},
        "structures":[{"id":"house","kind":"house","sections":[
            {"id":"base","c":130,"hp":321,"material":"steel","anchor":true,"supports":[]}]}],
        "scenario":{"version":1,"areas":[],"triggers":[]},
        "defend":[0],"assaultTime":600,"naval":true,"trenchFacing":true,
        "buildings":[{"x":11,"y":5,"kind":"stone bridge"}],
        "triggers":[{"at":6000,"say":"Private future","blow":[8,8,9,9]}]});
    let mut raw = clean.clone();
    raw["extra"] = json!("discard");
    raw["spawns"][0]["assault"] = json!(false);
    raw["points"][0].as_object_mut().unwrap().remove("mp");
    raw["points"][1]["vp"] = Value::Null;
    for pointer in [
        "/spawns/0",
        "/points/0",
        "/layers",
        "/layers/materials/0",
        "/layers/groundMaterials/0",
        "/layers/objectMaterials/0",
        "/layers/mineMaterials/0",
        "/structures/0",
        "/structures/0/sections/0",
        "/buildings/0",
        "/triggers/0",
    ] {
        raw.pointer_mut(pointer).unwrap()["extra"] = json!("discard");
    }
    (raw, clean)
}

#[tokio::test]
async fn map23_net12_http_save_preserves_the_complete_literal_field_whitelist() {
    let dir = Dir::new();
    let editor = MapEditor::open(dir.0.clone(), "secret").unwrap();
    let (raw, expected) = complete_whitelist_map();
    let (status, response) = request(
        &editor,
        Method::POST,
        "/maps/all-fields",
        Some("secret"),
        raw.to_string().into_bytes(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&response)
    );
    let disk: Value =
        serde_json::from_slice(&fs::read(dir.0.join("all-fields.json")).unwrap()).unwrap();
    assert_eq!(disk, expected);
    assert_eq!(editor.raw("all-fields"), Some(expected.clone()));
    assert_eq!(
        MapEditor::open(dir.0.clone(), "secret")
            .unwrap()
            .raw("all-fields"),
        Some(expected)
    );
    let mut sparse = raw_map();
    sparse["naval"] = json!(false);
    sparse["trenchFacing"] = json!(false);
    sparse["buildings"] = json!([]);
    sparse["triggers"] = json!([]);
    let mut rows = vec![".".repeat(24); 24];
    rows[0].replace_range(1..2, "N");
    let expected = json!({"name":"Editor map","w":24,"h":24,"rows":rows,
        "spawns":[{"x":3,"y":3},{"x":20,"y":20}],
        "points":[{"id":"front","x":12,"y":12,"vp":1,"mp":1}],"defend":[0]});
    assert_eq!(
        request(
            &editor,
            Method::POST,
            "/maps/sparse",
            Some("secret"),
            sparse.to_string().into_bytes()
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(editor.raw("sparse"), Some(expected));
}

#[tokio::test]
async fn map23_all_33_shipped_maps_survive_http_save_disk_reopen_and_validation() {
    let dir = Dir::new();
    let editor = MapEditor::open(dir.0.clone(), "secret").unwrap();
    let names: Value = serde_json::from_str(include_str!("map-catalog.json")).unwrap();
    assert_eq!(names.as_object().unwrap().len(), 33);
    for name in names.as_object().unwrap().keys() {
        let source = crate::authoritative_maps::load(name).unwrap().source();
        let (status, response) = request(
            &editor,
            Method::POST,
            &format!("/maps/{name}"),
            Some("secret"),
            source.to_string().into_bytes(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{name}: {}",
            String::from_utf8_lossy(&response)
        );
        let disk = fs::read(dir.0.join(format!("{name}.json"))).unwrap();
        let value: Value = serde_json::from_slice(&disk).unwrap();
        assert_eq!(
            ww2_core::map_save::validate_editor_map(&value),
            Ok(()),
            "{name}"
        );
        ww2_core::map::AuthoritativeMap::from_json(&disk).unwrap();
        assert_eq!(editor.raw(name), Some(value));
    }
    let reopened = MapEditor::open(dir.0.clone(), "secret").unwrap();
    for name in names.as_object().unwrap().keys() {
        assert_eq!(editor.raw(name), reopened.raw(name), "{name}");
    }
}

#[tokio::test]
async fn net11_raw_and_public_get_preserve_projection_headers_and_route_boundaries() {
    let dir = Dir::new();
    let (raw, _) = complete_whitelist_map();
    let literal = format!(" {}\n", serde_json::to_string_pretty(&raw).unwrap());
    fs::write(dir.0.join("private.json"), &literal).unwrap();
    let editor = MapEditor::open(dir.0.clone(), "secret").unwrap();
    for suffix in ["", ".json"] {
        for credential in [None, Some("secret")] {
            let mut headers = HeaderMap::new();
            if let Some(password) = credential {
                headers.insert("x-edit-password", password.parse().unwrap());
            }
            let response = serve(
                OriginalUri(format!("/maps/private{suffix}").parse().unwrap()),
                Extension(Some(editor.clone())),
                Method::GET,
                headers,
                Body::empty(),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["cache-control"], "no-store");
            assert_eq!(response.headers()["content-type"], "application/json");
            let body = to_bytes(response.into_body(), 16 * 1024 * 1024)
                .await
                .unwrap();
            if credential.is_some() {
                assert_eq!(body.as_ref(), literal.as_bytes());
                continue;
            }
            let public: Value = serde_json::from_slice(&body).unwrap();
            let mut expected = raw.clone();
            expected.as_object_mut().unwrap().remove("scenario");
            expected.as_object_mut().unwrap().remove("triggers");
            expected["rows"][0] = json!(".".repeat(24));
            expected["layers"]["objects"][0] = json!(".".repeat(24));
            expected["layers"]["mines"] = json!(vec![".".repeat(24); 24]);
            expected["layers"]["mineMaterials"] = json!([]);
            assert_eq!(public, expected);
        }
    }
    for (method, path, password, status, message) in [
        (
            Method::GET,
            "/maps/private",
            Some("wrong"),
            StatusCode::FORBIDDEN,
            "wrong password",
        ),
        (
            Method::GET,
            "/maps/missing",
            None,
            StatusCode::NOT_FOUND,
            "no such map",
        ),
        (
            Method::GET,
            "/maps/missing",
            Some("secret"),
            StatusCode::NOT_FOUND,
            "no such map",
        ),
        (
            Method::GET,
            "/maps/missing",
            Some("wrong"),
            StatusCode::FORBIDDEN,
            "wrong password",
        ),
        (
            Method::GET,
            "/maps/Upper",
            None,
            StatusCode::BAD_REQUEST,
            "bad map name",
        ),
        (
            Method::GET,
            "/maps/%2fsecret",
            Some("secret"),
            StatusCode::BAD_REQUEST,
            "bad map name",
        ),
        (
            Method::PUT,
            "/maps/private",
            None,
            StatusCode::METHOD_NOT_ALLOWED,
            "method not allowed",
        ),
        (
            Method::DELETE,
            "/maps/private",
            Some("wrong"),
            StatusCode::METHOD_NOT_ALLOWED,
            "method not allowed",
        ),
        (
            Method::HEAD,
            "/maps/missing",
            Some("secret"),
            StatusCode::METHOD_NOT_ALLOWED,
            "method not allowed",
        ),
    ] {
        assert_eq!(
            request(&editor, method, path, password, vec![]).await,
            (status, message.as_bytes().to_vec())
        );
    }
    let (status, body) = request(&editor, Method::DELETE, "/maps", Some("wrong"), vec![]).await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<String> = serde_json::from_slice(&body).unwrap();
    assert!(names.contains(&"private".to_owned()));
    assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
}

#[tokio::test]
async fn net12_corrupt_overrides_preserve_raw_get_default_catalogue_and_strict_refusal() {
    let dir = Dir::new();
    let mut invalid = raw_map();
    invalid["w"] = json!(1);
    for (name, body) in [
        ("broken", b"{malformed\n".to_vec()),
        ("bastogne", invalid.to_string().into_bytes()),
    ] {
        fs::write(dir.0.join(format!("{name}.json")), body).unwrap();
    }
    fs::create_dir(dir.0.join("unreadable.json")).unwrap();
    // A directory with a .json name produces a deterministic read failure even
    // when the test runner has permission to read every regular file.
    fs::create_dir(dir.0.join("default.json")).unwrap();
    let editor = MapEditor::open(dir.0.clone(), "secret").unwrap();
    let (_, catalog) = editor.catalog();
    assert_eq!(catalog["broken"], catalog["default"]);
    assert_eq!(catalog["bastogne"], catalog["default"]);
    assert_eq!(catalog["bastogne"]["horde"], false);
    for name in ["unreadable", "default"] {
        assert_eq!(catalog[name], catalog["broken"]);
        assert_eq!(editor.raw(name), Some(Value::Null));
        for password in [None, Some("secret")] {
            assert_eq!(
                request(
                    &editor,
                    Method::GET,
                    &format!("/maps/{name}"),
                    password,
                    vec![]
                )
                .await,
                (StatusCode::NOT_FOUND, b"no such map".to_vec())
            );
        }
    }
    let revision = editor.catalog().0;
    assert_eq!(
        request(
            &editor,
            Method::POST,
            "/maps/unreadable",
            Some("secret"),
            raw_map().to_string().into_bytes()
        )
        .await
        .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(editor.catalog().0, revision);
    fs::remove_dir(dir.0.join("unreadable.json")).unwrap();
    assert_eq!(
        request(
            &editor,
            Method::POST,
            "/maps/unreadable",
            Some("secret"),
            raw_map().to_string().into_bytes()
        )
        .await
        .0,
        StatusCode::OK
    );
    assert!(!editor.unavailable("unreadable"));
    assert_eq!(editor.catalog().1["unreadable"]["horde"], true);
    for name in ["broken", "bastogne"] {
        assert!(
            ww2_core::map::AuthoritativeMap::from_json(
                &editor.raw(name).unwrap().to_string().into_bytes()
            )
            .is_err()
        );
        assert_eq!(
            request(
                &editor,
                Method::GET,
                &format!("/maps/{name}"),
                Some("secret"),
                vec![]
            )
            .await,
            (
                StatusCode::OK,
                fs::read(dir.0.join(format!("{name}.json"))).unwrap()
            )
        );
    }
    assert_eq!(
        request(&editor, Method::GET, "/maps/broken", None, vec![]).await,
        (StatusCode::NOT_FOUND, b"no such map".to_vec())
    );
    let before = editor.catalog().0;
    assert_eq!(
        request(
            &editor,
            Method::POST,
            "/maps/broken",
            Some("secret"),
            raw_map().to_string().into_bytes()
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(editor.catalog().0, before + 1);
    assert_eq!(editor.catalog().1["broken"]["horde"], true);
    assert!(
        ww2_core::map::AuthoritativeMap::from_json(&fs::read(dir.0.join("broken.json")).unwrap())
            .is_ok()
    );
}

fn editor_world_map() -> Value {
    let mut map = raw_map();
    map.as_object_mut().unwrap().remove("defend");
    map["points"] = json!([]);
    map["world"] = json!({"size":"huge","total":64,"regionCells":64,
        "regions":(0..64).map(|id| {
            let mut region = json!({"id":1_000_000_000_000_u64+id,"x":id%8,"y":id/8,
                "bounds":[null,"untyped",-1000,{}],"kind":false,"team":"untyped"});
            if id<2 {region["home"]=json!(id);}
            region
        }).collect::<Vec<_>>(),"seed":"untyped","biomes":false,"generatorVersion":[]});
    map
}

#[tokio::test]
async fn net12_world_editor_acceptance_matches_prototype_without_relaxing_runtime() {
    let dir = Dir::new();
    let editor = MapEditor::open(dir.0.clone(), "secret").unwrap();
    let source = editor_world_map();
    assert!(
        ww2_core::map::AuthoritativeMap::from_json(source.to_string().as_bytes()).is_err(),
        "typed runtime keeps its bounded geography requirements"
    );
    assert!(ww2_core::map_save::validate_editor_map(&source).is_ok());
    assert_eq!(
        request(
            &editor,
            Method::POST,
            "/maps/world-authoring",
            Some("secret"),
            source.to_string().into_bytes()
        )
        .await,
        (StatusCode::OK, b"{\"ok\":true}".to_vec())
    );
    let clean: Value =
        serde_json::from_slice(&fs::read(dir.0.join("world-authoring.json")).unwrap()).unwrap();
    assert!(
        clean.get("world").is_none(),
        "prototype whitelist omits World geography"
    );
    assert_eq!(clean["points"], json!([]));
    assert!(ww2_core::map_save::validate_editor_map(&clean).is_err());
    assert!(ww2_core::map::AuthoritativeMap::from_json(clean.to_string().as_bytes()).is_err());
    assert_eq!(
        editor.catalog().1["world-authoring"],
        editor.catalog().1["default"]
    );
    let baseline = fs::read(dir.0.join("world-authoring.json")).unwrap();
    for (pointer, value, message) in [
        ("/world/total", json!(63), "invalid world regions"),
        (
            "/world/regions/2/bounds",
            json!([0, 0, 1]),
            "invalid world region",
        ),
        (
            "/world/regions/2/id",
            json!(1_000_000_000_000_u64),
            "scenario: capture targets need unique IDs",
        ),
        ("/world/regions/2/id", json!(3.5), "invalid world region"),
        ("/world/regions/2/x", json!(24), "invalid world region"),
        ("/world/regions/1/home", json!(0), "invalid world home"),
        ("/world/regions/1/home", json!(2), "invalid world home"),
    ] {
        let mut bad = source.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        let (status, body) = request(
            &editor,
            Method::POST,
            "/maps/world-authoring",
            Some("secret"),
            bad.to_string().into_bytes(),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{pointer}");
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            json!({"error":message}),
            "{pointer}"
        );
        assert_eq!(
            fs::read(dir.0.join("world-authoring.json")).unwrap(),
            baseline
        );
    }
    let mut disconnected = source.clone();
    disconnected["rows"][4] = json!("W".repeat(24));
    // Every objective must remain open before connectivity is considered.
    for region in disconnected["world"]["regions"].as_array_mut().unwrap() {
        if region["y"] == 4 {
            region["y"] = json!(8);
        }
    }
    let (status, body) = request(
        &editor,
        Method::POST,
        "/maps/world-authoring",
        Some("secret"),
        disconnected.to_string().into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"error":"world regions need connected ground routes"})
    );
    let mut missing_home = source.clone();
    missing_home["world"]["regions"][1]
        .as_object_mut()
        .unwrap()
        .remove("home");
    let (status, body) = request(
        &editor,
        Method::POST,
        "/maps/world-authoring",
        Some("secret"),
        missing_home.to_string().into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"error":"each player needs one home"})
    );
    let mut membership = source.clone();
    membership["world"]["regionMap"] = json!([]);
    let (status, body) = request(
        &editor,
        Method::POST,
        "/maps/world-authoring",
        Some("secret"),
        membership.to_string().into_bytes(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"error":"invalid region membership"})
    );
}

#[tokio::test]
async fn net11_public_world_projection_does_not_require_runtime_map_admission() {
    let dir = Dir::new();
    let source = editor_world_map();
    fs::write(dir.0.join("world-source.json"), source.to_string()).unwrap();
    let mut invalid_ordinary = raw_map();
    invalid_ordinary["points"] = json!([]);
    fs::write(
        dir.0.join("ordinary-source.json"),
        invalid_ordinary.to_string(),
    )
    .unwrap();
    let editor = MapEditor::open(dir.0.clone(), "secret").unwrap();
    assert_eq!(
        request(
            &editor,
            Method::POST,
            "/maps/world-saved",
            Some("secret"),
            source.to_string().into_bytes()
        )
        .await
        .0,
        StatusCode::OK
    );
    for name in ["world-source", "world-saved", "ordinary-source"] {
        let raw = editor.raw(name).unwrap();
        assert!(ww2_core::map::AuthoritativeMap::from_json(raw.to_string().as_bytes()).is_err());
        let mut expected = raw.clone();
        expected.as_object_mut().unwrap().remove("triggers");
        expected.as_object_mut().unwrap().remove("scenario");
        expected["rows"] = json!(vec![".".repeat(24); 24]);
        expected["worldVersion"] = json!(2);
        expected["layers"] = json!({"ground":vec![".".repeat(24);24],
            "objects":vec![".".repeat(24);24],"mines":vec![".".repeat(24);24]});
        let (status, body) =
            request(&editor, Method::GET, &format!("/maps/{name}"), None, vec![]).await;
        assert_eq!(status, StatusCode::OK, "{name}");
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            expected,
            "{name}"
        );
    }
}
