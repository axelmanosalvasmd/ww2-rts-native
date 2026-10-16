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
fn net12_password_file_is_reused_and_override_does_not_expose_or_replace_it() {
    let dir = Dir::new();
    let file = dir.0.join(".edit-password");
    let first = MapEditor::from_password_file(dir.0.join("maps"), None, &file).unwrap();
    let password = fs::read_to_string(&file).unwrap();
    assert_eq!(password.len(), 12);
    assert!(first.authorized(password.as_bytes()));
    let second = MapEditor::from_password_file(dir.0.join("maps"), None, &file).unwrap();
    assert!(second.authorized(password.as_bytes()));
    let third = MapEditor::from_password_file(dir.0.join("maps"), Some("override"), &file).unwrap();
    assert!(third.authorized(b"override"));
    assert!(!third.authorized(password.as_bytes()));
    assert_eq!(fs::read_to_string(file).unwrap(), password);
    assert!(!format!("{third:?}").contains("override"));
}
