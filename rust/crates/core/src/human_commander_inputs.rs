//! Detached camera, selection and command-input helpers from the human commander.
//! Commands retain their raw shape until the canonical host normalizes them.

use crate::{game::Point, math};
use serde_json::Value;
use std::collections::BTreeSet;

const SCREEN_X: f64 = 55.0;
const SCREEN_Z: f64 = 40.0;
const SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    pub id: u32,
    pub at: Point,
    pub world_goal: Option<Point>,
    pub amove: Option<Point>,
    pub target_id: u32,
    pub retreating: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    pub units: Vec<Unit>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cursor {
    pub camera: Point,
    /// The source selection key is sorted, with repeated IDs retained.
    pub selection: Vec<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    Camera,
    Select,
    Key,
    Click,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    pub kind: InputKind,
    pub at: Point,
    pub ids: Option<Vec<i64>>,
    pub minimap: Option<bool>,
    pub command: Option<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidView,
    InvalidCommand,
    InvalidIds,
    InvalidOrders,
    InvalidPoint,
    InvalidCursor,
    InputCapacity,
}

fn finite(at: Point) -> bool {
    at.x.is_finite() && at.z.is_finite()
}

fn id(value: &Value) -> Result<i64, Error> {
    value
        .as_f64()
        .filter(|n| n.is_finite() && n.fract() == 0.0 && n.abs() <= SAFE_INTEGER)
        .map(|n| n as i64)
        .ok_or(Error::InvalidIds)
}

fn order_rows(command: &Value) -> Result<Option<&Vec<Value>>, Error> {
    let Some(rows) = command.get("orders") else {
        return Ok(None);
    };
    let rows = rows.as_array().ok_or(Error::InvalidOrders)?;
    for row in rows {
        let row = row
            .as_array()
            .filter(|r| r.len() >= 3)
            .ok_or(Error::InvalidOrders)?;
        id(&row[0]).map_err(|_| Error::InvalidOrders)?;
        if !row[1].as_f64().is_some_and(f64::is_finite)
            || !row[2].as_f64().is_some_and(f64::is_finite)
        {
            return Err(Error::InvalidOrders);
        }
    }
    Ok(Some(rows))
}

fn validate_command(command: &Value) -> Result<&str, Error> {
    let object = command.as_object().ok_or(Error::InvalidCommand)?;
    let kind = object
        .get("t")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .ok_or(Error::InvalidCommand)?;
    // `formation` is the source helper's private scratch marker, not an intent field.
    if object.contains_key("formation") {
        return Err(Error::InvalidCommand);
    }
    if let Some(ids) = object.get("ids").filter(|v| !v.is_null()) {
        for value in ids.as_array().ok_or(Error::InvalidIds)? {
            id(value)?;
        }
    }
    order_rows(command)?;
    for key in ["x", "z"] {
        if let Some(value) = object.get(key)
            && !value.as_f64().is_some_and(f64::is_finite)
        {
            return Err(Error::InvalidPoint);
        }
    }
    Ok(kind)
}

fn validate_view(view: &View) -> Result<(), Error> {
    let mut ids = BTreeSet::new();
    if view.units.iter().any(|u| {
        !ids.insert(u.id)
            || !finite(u.at)
            || u.world_goal.is_some_and(|p| !finite(p))
            || u.amove.is_some_and(|p| !finite(p))
    }) {
        return Err(Error::InvalidView);
    }
    Ok(())
}

fn unit(view: &View, selected: i64) -> Option<&Unit> {
    u32::try_from(selected)
        .ok()
        .and_then(|id| view.units.iter().find(|u| u.id == id))
}

pub fn on_screen(camera: Point, at: Point) -> bool {
    (at.x - camera.x).abs() <= SCREEN_X && (at.z - camera.z).abs() <= SCREEN_Z
}

fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}

fn centroid(points: impl IntoIterator<Item = Point>) -> Result<Option<Point>, Error> {
    let mut sum = Point { x: 0.0, z: 0.0 };
    let mut count = 0usize;
    for at in points {
        sum.x += at.x;
        sum.z += at.z;
        count = count.checked_add(1).ok_or(Error::InputCapacity)?;
    }
    if count == 0 {
        return Ok(None);
    }
    let at = Point {
        x: sum.x / count as f64,
        z: sum.z / count as f64,
    };
    finite(at).then_some(Some(at)).ok_or(Error::InvalidPoint)
}

/// `ids` takes precedence, including an empty array; null falls back to order rows.
pub fn units_of(command: &Value) -> Result<Vec<i64>, Error> {
    validate_command(command)?;
    if let Some(ids) = command.get("ids").filter(|v| !v.is_null()) {
        return ids
            .as_array()
            .ok_or(Error::InvalidIds)?
            .iter()
            .map(id)
            .collect();
    }
    order_rows(command)?
        .into_iter()
        .flatten()
        .map(|row| id(&row[0]))
        .collect()
}

/// Unknown targets have no place. Unknown selected IDs never supply coordinates.
pub fn place_of(command: &Value, view: &View) -> Result<Option<Point>, Error> {
    validate_command(command)?;
    validate_view(view)?;
    if let (Some(x), Some(z)) = (
        command.get("x").and_then(Value::as_f64),
        command.get("z").and_then(Value::as_f64),
    ) {
        return Ok(Some(Point { x, z }));
    }
    if let Some(rows) = order_rows(command)? {
        return centroid(rows.iter().map(|row| Point {
            x: row[1].as_f64().expect("validated order"),
            z: row[2].as_f64().expect("validated order"),
        }));
    }
    let target = command
        .get("target")
        .filter(|v| !v.is_null())
        .or_else(|| command.get("id"));
    Ok(target
        .and_then(|value| id(value).ok())
        .and_then(|target| unit(view, target))
        .map(|u| u.at))
}

fn plain_move(command: &Value) -> bool {
    matches!(command["t"].as_str(), Some("move" | "amove"))
        && command.get("orders").is_some_and(Value::is_array)
        && command
            .as_object()
            .is_some_and(|o| o.keys().all(|k| k == "t" || k == "orders"))
}

/// Filter only the source's plain moves, attacks and retreats. Keep raw extensions.
pub fn news(command: &Value, view: &View) -> Result<Option<Value>, Error> {
    let kind = validate_command(command)?;
    validate_view(view)?;
    let mut filtered = command.clone();
    if plain_move(command) {
        let rows = command["orders"].as_array().expect("validated orders");
        let left: Vec<_> = rows
            .iter()
            .filter(|row| {
                let goal = unit(view, id(&row[0]).expect("validated ID")).and_then(|u| {
                    if kind == "move" {
                        u.world_goal
                    } else {
                        u.amove
                    }
                });
                let at = Point {
                    x: row[1].as_f64().expect("validated order"),
                    z: row[2].as_f64().expect("validated order"),
                };
                !goal.is_some_and(|goal| distance(goal, at) <= 3.0)
            })
            .cloned()
            .collect();
        if left.is_empty() {
            return Ok(None);
        }
        filtered["orders"] = Value::Array(left);
    } else if matches!(kind, "attack" | "retreat")
        && let Some(ids) = command.get("ids").and_then(Value::as_array)
    {
        let left: Vec<_> = ids
            .iter()
            .filter(|selected| {
                !unit(view, id(selected).expect("validated ID")).is_some_and(|u| {
                    if kind == "retreat" {
                        u.retreating
                    } else {
                        command.get("target").and_then(Value::as_f64)
                            == Some(f64::from(u.target_id))
                    }
                })
            })
            .cloned()
            .collect();
        if left.is_empty() {
            return Ok(None);
        }
        filtered["ids"] = Value::Array(left);
    }
    Ok(Some(filtered))
}

pub fn fits_screen(units: &[&Unit]) -> Result<bool, Error> {
    let Some(at) = centroid(units.iter().map(|u| u.at))? else {
        return Ok(true);
    };
    Ok(units.iter().all(|u| on_screen(at, u.at)))
}

/// Merge into the first earlier plain order, retaining source ID and row order.
pub fn formation_moves(commands: Vec<Value>, view: &View) -> Result<Vec<Value>, Error> {
    validate_view(view)?;
    for command in &commands {
        validate_command(command)?;
        place_of(command, view)?;
    }
    let mut out: Vec<Value> = Vec::with_capacity(commands.len());
    for command in commands {
        let mut into = None;
        if plain_move(&command) {
            for (i, previous) in out.iter().enumerate() {
                if !plain_move(previous) || previous["t"] != command["t"] {
                    continue;
                }
                let (Some(a), Some(b)) = (place_of(previous, view)?, place_of(&command, view)?)
                else {
                    return Err(Error::InvalidOrders);
                };
                if distance(a, b) > 15.0 {
                    continue;
                }
                let mut ids = units_of(previous)?;
                ids.extend(units_of(&command)?);
                let units: Vec<_> = ids.into_iter().filter_map(|id| unit(view, id)).collect();
                if fits_screen(&units)? {
                    into = Some(i);
                    break;
                }
            }
        }
        if let Some(i) = into {
            out[i]["orders"].as_array_mut().expect("plain move").extend(
                command["orders"]
                    .as_array()
                    .expect("plain move")
                    .iter()
                    .cloned(),
            );
        } else {
            out.push(command);
        }
    }
    Ok(out)
}

fn input(kind: InputKind, at: Point) -> Input {
    Input {
        kind,
        at,
        ids: None,
        minimap: None,
        command: None,
    }
}

fn look(cursor: &mut Cursor, at: Option<Point>, out: &mut Vec<Input>) {
    if let Some(at) = at
        && !on_screen(cursor.camera, at)
    {
        out.push(input(InputKind::Camera, at));
        cursor.camera = at;
    }
}

/// Commit the predicted cursor only after every command and detached coordinate passes.
pub fn inputs_for(cursor: &mut Cursor, command: &Value, view: &View) -> Result<Vec<Input>, Error> {
    let kind = validate_command(command)?;
    validate_view(view)?;
    if !finite(cursor.camera)
        || cursor.selection.windows(2).any(|w| w[0] > w[1])
        || cursor
            .selection
            .iter()
            .any(|id| id.unsigned_abs() > SAFE_INTEGER as u64)
    {
        return Err(Error::InvalidCursor);
    }
    let ids = units_of(command)?;
    let place = place_of(command, view)?;
    let mut staged = cursor.clone();
    let mut out = Vec::new();
    if !ids.is_empty() {
        let mut key = ids.clone();
        key.sort_unstable();
        let at = centroid(ids.iter().filter_map(|id| unit(view, *id)).map(|u| u.at))?;
        if key != staged.selection {
            look(&mut staged, at, &mut out);
            let mut select = input(InputKind::Select, at.unwrap_or(staged.camera));
            select.ids = Some(ids);
            out.push(select);
            staged.selection = key;
        }
    }
    if !matches!(kind, "move" | "amove") {
        look(&mut staged, place, &mut out);
    }
    if matches!(
        kind,
        "amove"
            | "ability"
            | "dig"
            | "entrench"
            | "build"
            | "support"
            | "fireat"
            | "garrison"
            | "assist"
            | "stance"
            | "retreat"
            | "stop"
            | "cover"
    ) {
        out.push(input(InputKind::Key, place.unwrap_or(staged.camera)));
    }
    if !matches!(kind, "stance" | "retreat" | "stop" | "cover") {
        let mut click = input(InputKind::Click, place.unwrap_or(staged.camera));
        click.minimap = Some(place.is_some_and(|p| !on_screen(staged.camera, p)));
        out.push(click);
    }
    out.last_mut().ok_or(Error::InvalidCommand)?.command = Some(command.clone());
    *cursor = staged;
    Ok(out)
}
