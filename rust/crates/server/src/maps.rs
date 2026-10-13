use axum::{
    body::Body,
    extract::OriginalUri,
    http::{HeaderMap, Method, Response, StatusCode, header},
};

#[path = "map-data.rs"]
mod data;

/// Read-only map delivery. No request can select an authoring file or raw projection.
pub(crate) async fn serve(
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
) -> Response<Body> {
    response(uri.path(), &method, headers.contains_key("x-edit-password"))
}

fn response(path: &str, method: &Method, editor_header: bool) -> Response<Body> {
    let path = normalized_path(path);
    if path == "/maps" {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&data::MAPS.iter().map(|(name, _)| *name).collect::<Vec<_>>())
                    .unwrap(),
            ))
            .unwrap();
    }
    let Some(raw_name) = path
        .strip_prefix("/maps/")
        .filter(|s| !s.is_empty() && !s.contains('/'))
    else {
        return text(StatusCode::NOT_FOUND, "not found");
    };
    let name = raw_name.strip_suffix(".json").unwrap_or(raw_name);
    if !(1..=32).contains(&name.len())
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return text(StatusCode::BAD_REQUEST, "bad map name");
    }
    if method != Method::GET {
        // Editor writes are not part of this server slice.
        return text(StatusCode::METHOD_NOT_ALLOWED, "method not allowed");
    }
    if editor_header {
        // There is no editor password in this process and no raw map data to unlock.
        return text(StatusCode::FORBIDDEN, "wrong password");
    }
    match data::MAPS.iter().find(|(key, _)| *key == name) {
        Some((_, json)) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(*json))
            .unwrap(),
        None => text(StatusCode::NOT_FOUND, "no such map"),
    }
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
    fn raw_names_and_methods_cannot_reach_authoring_data() {
        for name in ["default", "default.json"] {
            assert_eq!(
                response(&format!("/maps/{name}"), &Method::GET, false).status(),
                StatusCode::OK
            );
            assert_eq!(
                response(&format!("/maps/{name}"), &Method::GET, true).status(),
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                response(&format!("/maps/{name}"), &Method::POST, false).status(),
                StatusCode::METHOD_NOT_ALLOWED
            );
        }
        for path in [
            "/maps/%64efault",
            "/maps/../secret",
            "/maps/default/extra",
            "/maps/default.json.json",
            "/maps/../../Cargo.toml",
        ] {
            assert_ne!(response(path, &Method::GET, false).status(), StatusCode::OK);
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
