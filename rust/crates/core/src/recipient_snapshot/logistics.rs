use super::*;
use crate::{logistics::Stock, supply_convoys::GRACE};

pub(super) fn validate(state: &Match) -> Result<(), SnapshotError> {
    state
        .validate_logistics_runtime()
        .map_err(|_| SnapshotError::Unsupported {
            unit: 0,
            capability: SnapshotCapability::Logistics,
        })
}
fn number(n: f64) -> Result<Value, SnapshotError> {
    if !n.is_finite() {
        return Err(SnapshotError::InvalidNumber);
    }
    Ok(json!(n))
}
fn rounded(n: f64) -> Result<Value, SnapshotError> {
    number(math::round(n * 100.0) / 100.0)
}
fn optional(n: Option<f64>) -> Result<Value, SnapshotError> {
    n.map_or(Ok(Value::Null), rounded)
}
fn stock(s: Stock, round: bool) -> Result<Value, SnapshotError> {
    let encode = if round { rounded } else { number };
    Ok(json!({
        "ammo": encode(s.ammo)?,
        "provisions": encode(s.provisions)?,
        "fuel": encode(s.fuel)?
    }))
}
fn point(p: crate::game::Point, round: bool) -> Result<Value, SnapshotError> {
    let encode = if round { rounded } else { number };
    Ok(json!({"x": encode(p.x)?, "z": encode(p.z)?}))
}

/// Prototype supply-convoys.js:646-659. This reads only this seat's private
/// rows and usable allied inventories. Visibility and reveal never widen it.
pub(super) fn project(state: &Match, slot: usize) -> Result<Option<Value>, SnapshotError> {
    let Some(convoys) = &state.convoys else {
        return if state.options.logistics {
            Err(SnapshotError::Rules)
        } else {
            Ok(None)
        };
    };
    let g = &state.world.game;
    let mut units = Vec::new();
    let mut trucks = Vec::new();
    for u in g.units.values().filter(|u| u.owner == slot) {
        if let Some(r) = state.logistics.get(u.id) {
            finite(&[r.ammo, r.ammo_max])?;
            if let Some(cut_for) = r.cut_for {
                finite(&[cut_for])?;
            }
            let cut_for = r.cut_for.unwrap_or(0.0);
            finite(&[r.ammo, r.ammo_max, cut_for])?;
            let cut = cut_for >= GRACE;
            let grace = (cut_for > 0.0 && cut_for < GRACE).then(|| (GRACE - cut_for).ceil());
            units.push(json!({
                "id": u.id,
                "ammo": optional((r.ammo_max != 0.0).then(|| r.ammo / r.ammo_max))?,
                "provisions": rounded(r.provisions)?,
                "fuel": optional(r.fuel)?,
                "emergency": optional(r.emergency)?,
                "warning": optional(r.warning)?,
                "forced": r.forced,
                "stranded": r.stranded,
                "supply": rounded(if cut { 0.0 } else { r.supply_rate.unwrap_or(0.0) })?,
                "cut": cut,
                "grace": grace.map_or(Ok(Value::Null), number)?
            }));
        }
        if let Some(c) = convoys.trucks.get(&u.id) {
            trucks.push(json!({
                "id": u.id,
                "state": c.state.name(),
                "manual": c.manual,
                "hold": c.hold,
                "cargo": stock(c.cargo, true)?,
                "destination": c.destination.map_or(Ok(Value::Null), |p| point(p, false))?,
                "route": u.path.iter().map(|&p| point(p, true)).collect::<Result<Vec<_>, _>>()?
            }));
        }
    }
    let mut stores = Vec::new();
    for store in convoys
        .stores
        .iter()
        .filter(|s| s.active && g.allied(s.owner, slot))
    {
        // available lazily inserts empty buckets. A local copy preserves the
        // immutable send pass without changing stock or donation authorization.
        let mut inventory = store.clone();
        stores.push(json!({
            "id": store.id.key(), "owner": store.owner,
            "x": rounded(store.at.x)?, "z": rounded(store.at.z)?,
            "source": store.source,
            "stock": stock(inventory.available(slot, true), false)?,
            "capacity": stock(store.capacity, false)?
        }));
    }
    let mut events = Vec::new();
    for e in convoys.events.iter().filter(|e| e.owner == slot) {
        finite(&[e.seconds])?;
        let mut event = json!({
            "id": e.id, "type": e.kind, "count": e.count, "message": e.message
        });
        if let Some(at) = e.at {
            event["x"] = number(at.x)?;
            event["z"] = number(at.z)?;
        }
        events.push(event);
    }
    Ok(Some(json!({
        "enabled": true, "units": units, "trucks": trucks,
        "stores": stores, "events": events
    })))
}

#[cfg(test)]
#[path = "logistics/tests.rs"]
mod tests;
