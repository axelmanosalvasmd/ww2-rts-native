use axum::{
    body::Body,
    extract::OriginalUri,
    http::{HeaderMap, Method, Response, StatusCode, header},
};

#[path = "map-data.rs"]
mod data;

/// Sanitize the immutable authoring map for catalogue regression checks.
#[cfg(test)]
pub(crate) fn public_map(name: &str) -> Option<serde_json::Value> {
    let map = crate::authoritative_maps::load(name).ok()?;
    ww2_core::map::map_for_client(map.source()).ok()
}

/// Public maps are sanitized; only a valid editor password selects raw maps or writes.
pub(crate) async fn serve(
    OriginalUri(uri): OriginalUri,
    axum::Extension(editor): axum::Extension<Option<crate::MapEditor>>,
    method: Method,
    headers: HeaderMap,
    body: Body,
) -> Response<Body> {
    let path = normalized_path(uri.path());
    let credential = headers.get("x-edit-password");
    if path == "/maps" {
        let names = editor
            .as_ref()
            .map(|editor| {
                editor
                    .catalog()
                    .1
                    .as_object()
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| {
                data::MAPS
                    .iter()
                    .map(|(name, _)| (*name).to_owned())
                    .collect()
            });
        return json(StatusCode::OK, serde_json::json!(names));
    }
    let Some(raw_name) = path
        .strip_prefix("/maps/")
        .filter(|s| !s.is_empty() && !s.contains('/'))
    else {
        return text(StatusCode::NOT_FOUND, "not found");
    };
    let name = raw_name.strip_suffix(".json").unwrap_or(raw_name);
    if !crate::map_editor::valid_name(name) {
        return text(StatusCode::BAD_REQUEST, "bad map name");
    }
    let authorized = editor
        .as_ref()
        .is_some_and(|editor| editor.authorized(credential.map_or(b"", |v| v.as_bytes())));
    if method == Method::POST {
        if !authorized {
            return json(
                StatusCode::FORBIDDEN,
                serde_json::json!({"error":"wrong password"}),
            );
        }
        let bytes = match axum::body::to_bytes(body, 8 * 1024 * 1024).await {
            Ok(bytes) => bytes,
            Err(_) => {
                return json(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    serde_json::json!({"error":"map too large"}),
                );
            }
        };
        let raw = match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(raw) => raw,
            Err(_) => {
                return json(
                    StatusCode::BAD_REQUEST,
                    serde_json::json!({"error":"bad JSON"}),
                );
            }
        };
        return match editor.unwrap().save(name, &bytes) {
            Ok(()) => json(StatusCode::OK, serde_json::json!({"ok":true})),
            Err(crate::map_editor::SaveError::Invalid(error)) => json(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error":crate::editor_errors::message(&error, &raw)}),
            ),
            Err(crate::map_editor::SaveError::Io(error)) => {
                eprintln!("map not saved: {error}");
                json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    serde_json::json!({"error":"map not saved"}),
                )
            }
        };
    }
    if method != Method::GET {
        return text(StatusCode::METHOD_NOT_ALLOWED, "method not allowed");
    }
    if credential.is_some() && !authorized {
        return text(StatusCode::FORBIDDEN, "wrong password");
    }
    let raw = editor.as_ref().and_then(|e| e.raw(name)).or_else(|| {
        crate::authoritative_maps::load(name)
            .ok()
            .map(|m| m.source().clone())
    });
    let value = raw.and_then(|raw| {
        if credential.is_some() {
            Some(raw)
        } else {
            ww2_core::map::map_for_client(&raw).ok()
        }
    });
    match value {
        Some(value) => {
            let mut response = json(StatusCode::OK, value);
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                header::HeaderValue::from_static("no-store"),
            );
            response
        }
        None => text(StatusCode::NOT_FOUND, "no such map"),
    }
}
fn json(status: StatusCode, value: serde_json::Value) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&value).unwrap()))
        .unwrap()
}

fn text(status: StatusCode, body: &'static str) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Body::from(body))
        .unwrap()
}

// The reference constructs an HTTP URL before routing. Match its dot-segment and
// backslash normalization without turning percent-encoded names into file paths.
fn normalized_path(path: &str) -> String {
    let path = path.replace('\\', "/");
    let segments: Vec<_> = path.split('/').collect();
    let mut result = Vec::new();
    for (i, segment) in segments.iter().enumerate() {
        let lower = segment.to_ascii_lowercase();
        if matches!(lower.as_str(), "." | "%2e") {
            if i + 1 == segments.len() {
                result.push("");
            }
        } else if matches!(lower.as_str(), ".." | ".%2e" | "%2e." | "%2e%2e") {
            if result.len() > 1 {
                result.pop();
            }
            if i + 1 == segments.len() {
                result.push("");
            }
        } else {
            result.push(segment);
        }
    }
    result.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn map24_catalogue_public_projection_matches_every_committed_public_asset() {
        for (key, text) in data::MAPS {
            let expected: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(public_map(key).unwrap(), expected, "{key}");
        }
    }
    #[tokio::test]
    async fn raw_names_and_methods_cannot_reach_authoring_data() {
        async fn response(path: &str, method: &Method, editor: bool) -> Response<Body> {
            let mut headers = HeaderMap::new();
            if editor {
                headers.insert("x-edit-password", "wrong".parse().unwrap());
            }
            serve(
                OriginalUri(path.parse().unwrap()),
                axum::Extension(None),
                method.clone(),
                headers,
                Body::empty(),
            )
            .await
        }
        for name in ["default", "default.json"] {
            assert_eq!(
                response(&format!("/maps/{name}"), &Method::GET, false)
                    .await
                    .status(),
                StatusCode::OK
            );
            assert_eq!(
                response(&format!("/maps/{name}"), &Method::GET, true)
                    .await
                    .status(),
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                response(&format!("/maps/{name}"), &Method::POST, false)
                    .await
                    .status(),
                StatusCode::FORBIDDEN
            );
        }
        for path in [
            "/maps/%64efault",
            "/maps/../secret",
            "/maps/default/extra",
            "/maps/default.json.json",
            "/maps/../../Cargo.toml",
        ] {
            assert_ne!(
                response(path, &Method::GET, false).await.status(),
                StatusCode::OK
            );
        }
        assert_eq!(
            normalized_path("/maps/%2e%2e/maps/default"),
            "/maps/default"
        );
    }
    #[test]
    fn embedded_maps_match_the_lobby_catalogue_and_expose_no_mines_or_scripts() {
        let catalog: serde_json::Value =
            serde_json::from_str(include_str!("map-catalog.json")).unwrap();
        assert_eq!(data::MAPS.len(), catalog.as_object().unwrap().len());
        for (name, text) in data::MAPS {
            let private = crate::authoritative_maps::load(name).unwrap();
            assert_eq!(
                Some(private.name()),
                serde_json::from_str::<serde_json::Value>(text).unwrap()["name"].as_str()
            );
            assert!(catalog.get(name).is_some());
            let map: serde_json::Value = serde_json::from_str(text).unwrap();
            assert!(map.get("triggers").is_none() && map.get("scenario").is_none());
            for key in ["rows", "layers"] {
                let rows = if key == "rows" {
                    &map["rows"]
                } else {
                    &map["layers"]["objects"]
                };
                assert!(
                    rows.as_array()
                        .unwrap()
                        .iter()
                        .all(|r| !r.as_str().unwrap().contains('N'))
                );
            }
            assert!(
                map["layers"]["mines"].as_array().unwrap().iter().all(|r| r
                    .as_str()
                    .unwrap()
                    .chars()
                    .all(|c| c == '.'))
            );
        }
    }
}
