//! Stateless presentation queries from the compiled unit catalog and projectile providers.
//! Source: `shared/vehicle-motion.js:15-17`, `shared/projectiles.js:22-28` at `e4e579a`.

use crate::{
    projectiles::{self, Flight, Point3},
    rules::{Rules, UnitKind},
};
use serde_json::{Value, json};
use std::sync::OnceLock;

fn kind(input: &Value) -> Result<&'static UnitKind, String> {
    // Immutable compiled rules are shared by presentation calls, with no match state retained.
    static RULES: OnceLock<Result<Rules, String>> = OnceLock::new();
    let name = input["type"].as_str().ok_or("type must be a unit name")?;
    RULES
        .get_or_init(|| Rules::compiled().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(Clone::clone)?
        .kinds
        .iter()
        .find(|kind| kind.name == name)
        .ok_or_else(|| format!("unknown unit type: {name}"))
}

pub fn movement_profile(input: &Value) -> Result<Value, String> {
    Ok(match kind(input)?.vehicle.as_ref() {
        None => Value::Null,
        Some(p) => json!({
            "acceleration": p.acceleration,
            "reverseAcceleration": p.reverse_acceleration,
            "braking": p.braking,
            "reverseSpeed": p.reverse_speed,
            "hullTurn": p.hull_turn,
            "tracked": p.tracked,
        }),
    })
}

pub fn is_ground_vehicle(input: &Value) -> Result<Value, String> {
    Ok(json!(kind(input)?.ground_vehicle()))
}

pub fn body_radius(input: &Value) -> Result<Value, String> {
    Ok(json!(kind(input)?.body_radius()))
}

fn number(value: &Value, key: &str) -> Result<f64, String> {
    value[key]
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("{key} must be a finite number"))
}

fn flight(input: &Value) -> Result<(Flight, f64), String> {
    let p = &input["flight"];
    Ok((
        Flight {
            origin: Point3::new(number(p, "x")?, number(p, "y")?, number(p, "z")?),
            velocity: Point3::new(number(p, "vx")?, number(p, "vy")?, number(p, "vz")?),
            gravity: number(p, "gravity")?,
            stamp: number(p, "stamp")?,
        },
        number(input, "time")?,
    ))
}

pub fn flight_at(input: &Value) -> Result<Value, String> {
    let (p, time) = flight(input)?;
    let at = projectiles::flight_at(p, time).map_err(|e| format!("invalid flight: {e:?}"))?;
    serde_json::to_value(at).map_err(|e| e.to_string())
}

pub fn velocity_at(input: &Value) -> Result<Value, String> {
    let (p, time) = flight(input)?;
    let at = projectiles::velocity_at(p, time).map_err(|e| format!("invalid flight: {e:?}"))?;
    serde_json::to_value(at).map_err(|e| e.to_string())
}
