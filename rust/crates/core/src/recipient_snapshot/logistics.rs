//! Prototype supply-convoys.js:646 private reserves, convoys, stock and alerts.
use super::*;
use crate::logistics::Stock;

pub(super) fn validate(state: &Match) -> Result<(), SnapshotError> {
    state
        .validate_logistics_runtime()
        .map_err(|_| SnapshotError::Unsupported {
            unit: 0,
            capability: SnapshotCapability::Logistics,
        })
}
fn round(n: f64) -> Result<Value, SnapshotError> {
    finite(&[n])?;
    Ok(json!(math::round(n * 100.0) / 100.0))
}
fn optional(n: Option<f64>) -> Result<Value, SnapshotError> {
    n.map_or(Ok(Value::Null), round)
}
fn point(p: crate::game::Point, rounded: bool) -> Result<Value, SnapshotError> {
    finite(&[p.x, p.z])?;
    Ok(if rounded {
        json!({"x":round(p.x)?,"z":round(p.z)?})
    } else {
        json!({"x":p.x,"z":p.z})
    })
}
fn stock(s: Stock, rounded: bool) -> Result<Value, SnapshotError> {
    finite(&[s.ammo, s.provisions, s.fuel])?;
    Ok(if rounded {
        json!({"ammo":round(s.ammo)?,"provisions":round(s.provisions)?,"fuel":round(s.fuel)?})
    } else {
        json!({"ammo":s.ammo,"provisions":s.provisions,"fuel":s.fuel})
    })
}
pub(super) fn project(state: &Match, slot: usize) -> Result<Option<Value>, SnapshotError> {
    let Some(convoys) = &state.convoys else {
        return Ok(None);
    };
    let g = &state.world.game;
    let mut units = Vec::new();
    let mut trucks = Vec::new();
    for u in g.units.values().filter(|u| u.owner == slot) {
        if let Some(reserve) = state.logistics.get(u.id) {
            finite(&[reserve.ammo, reserve.ammo_max])?;
            if let Some(cut_for) = reserve.cut_for {
                finite(&[cut_for])?;
            }
            let cut_for = reserve.cut_for.unwrap_or(0.0);
            let cut = cut_for >= 10.0;
            units.push(json!({"id":u.id,
                "ammo":if reserve.ammo_max == 0.0 { Value::Null } else { round(reserve.ammo / reserve.ammo_max)? },
                "provisions":round(reserve.provisions)?,"fuel":optional(reserve.fuel)?,
                "emergency":optional(reserve.emergency)?,"warning":optional(reserve.warning)?,
                "forced":reserve.forced,"stranded":reserve.stranded,
                "supply":round(if cut { 0.0 } else { reserve.supply_rate.unwrap_or(0.0) })?,
                "cut":cut,"grace":if cut_for > 0.0 && cut_for < 10.0 { json!((10.0-cut_for).ceil()) } else { Value::Null }
            }));
        }
        if let Some(truck) = convoys.trucks.get(&u.id) {
            let route = u
                .path
                .iter()
                .map(|&p| point(p, true))
                .collect::<Result<Vec<_>, _>>()?;
            trucks.push(json!({"id":u.id,"state":truck.state.name(),"manual":truck.manual,
                "hold":truck.hold,"cargo":stock(truck.cargo,true)?,
                "destination":truck.destination.map_or(Ok(Value::Null), |p| point(p,false))?,"route":route}));
        }
    }
    let mut stores = Vec::new();
    for store in convoys
        .stores
        .iter()
        .filter(|s| s.active && g.allied(s.owner, slot))
    {
        finite(&[store.at.x, store.at.z])?;
        // available creates a zero bucket lazily. Clone the store so reads never
        // modify canonical inventory or depend on the order recipients are read.
        let available = store.clone().available(slot, true);
        stores.push(
            json!({"id":store.id.key(),"owner":store.owner,"x":round(store.at.x)?,
            "z":round(store.at.z)?,"source":store.source,"stock":stock(available,false)?,
            "capacity":stock(store.capacity,false)?}),
        );
    }
    let mut events = Vec::new();
    for event in convoys.events.iter().filter(|e| e.owner == slot) {
        finite(&[event.seconds])?;
        let mut row = json!({"id":event.id,"type":event.kind,"count":event.count,
            "message":event.message});
        if let Some(at) = event.at {
            finite(&[at.x, at.z])?;
            row["x"] = json!(at.x);
            row["z"] = json!(at.z);
        }
        // The prototype removes both private owner and its internal `at` clock.
        events.push(row);
    }
    Ok(Some(
        json!({"enabled":true,"units":units,"trucks":trucks,"stores":stores,"events":events}),
    ))
}
