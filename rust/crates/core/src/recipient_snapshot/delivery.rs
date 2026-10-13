use super::*;
use crate::observed_memory::ObservedRow;

#[derive(Clone, Debug, PartialEq)]
struct SeatFog {
    version: u32,
    base: Vec<u8>,
}
/// Delivery acknowledgments for one Match. Create a fresh state for each match;
/// reconnect uses a full read for that seat without draining remembered terrain.
#[derive(Clone, Debug, PartialEq)]
pub struct DeliveryState {
    dimensions: (usize, usize),
    seats: Vec<SeatFog>,
}
pub struct DeliveryFrame<'a> {
    cache: SnapshotCache<'a>,
    updates: Vec<Map<String, Value>>,
}
impl DeliveryState {
    pub fn new(state: &Match) -> Result<Self, SnapshotError> {
        let cache = SnapshotCache::new(state)?;
        let g = &cache.state.world.game;
        Ok(Self {
            dimensions: (g.w, g.h),
            seats: vec![
                SeatFog {
                    version: 0,
                    base: vec![0; g.chars.len()]
                };
                g.players.len()
            ],
        })
    }
    /// Collect mutable recipient memories before borrowing the immutable cache.
    /// Full is the source match-start/reconnect read. Omit only suppresses the
    /// browser mask for an AI seat; it never changes authoritative visibility.
    pub fn capture<'a>(
        &mut self,
        state: &'a mut Match,
        full: &[bool],
        omit: &[bool],
    ) -> Result<DeliveryFrame<'a>, SnapshotError> {
        let count = state.world.game.players.len();
        if self.dimensions != (state.world.game.w, state.world.game.h)
            || self.seats.len() != count
            || full.len() != count
            || omit.len() != count
        {
            return Err(SnapshotError::InvalidRecipient);
        }
        // All represented rows and routing contexts must pass before an outbox
        // is drained or a delivery acknowledgment is advanced.
        {
            let cache = SnapshotCache::new(state)?;
            for slot in 0..count {
                cache.recipient(slot)?;
                state
                    .recipient_routing(slot)
                    .map_err(|_| SnapshotError::Rules)?;
            }
        }
        let remembered = (0..count)
            .map(|slot| {
                state
                    .observed_terrain(slot, true)
                    .map_err(|_| SnapshotError::Rules)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut seats = self.seats.clone();
        let mut updates = vec![Map::new(); count];
        for slot in 0..count {
            let rows = if full[slot] {
                remembered[slot].clone()
            } else {
                state
                    .observed_terrain(slot, false)
                    .map_err(|_| SnapshotError::Rules)?
            };
            updates[slot].insert(
                "cells".into(),
                Value::Array(rows.iter().map(cell_json).collect()),
            );
            if omit[slot] {
                continue;
            }
            let g = &state.world.game;
            let f = state
                .routing_fog
                .team(g, &state.rules, g.players[slot].team);
            let seat = &mut seats[slot];
            if full[slot] {
                updates[slot].insert(
                    "fog".into(),
                    json!({"v":pack(&f.vis,None),"e":pack(&f.explored,None)}),
                );
            } else if seat.version != f.version {
                let delta = if let Some(delta) = f.delta(seat.version) {
                    delta.to_owned()
                } else {
                    let delta = pack(&f.vis, Some(&seat.base));
                    // Match source fogFor: peers on the same base share packing.
                    state
                        .routing_fog
                        .teams
                        .get_mut(&g.players[slot].team)
                        .unwrap()
                        .deltas
                        .push((seat.version, delta.clone()));
                    delta
                };
                updates[slot].insert("fog".into(), json!(delta));
            } else {
                continue;
            }
            let f = &state.routing_fog.teams[&g.players[slot].team];
            seat.version = f.version;
            seat.base.clone_from(&f.vis);
        }
        self.seats = seats;
        Ok(DeliveryFrame {
            cache: SnapshotCache::new(state)?,
            updates,
        })
    }
}
impl DeliveryFrame<'_> {
    pub fn recipient(&self, slot: usize) -> Result<RecipientProjection, SnapshotError> {
        let mut result = self.cache.recipient(slot)?;
        result.fields.extend(self.updates[slot].clone());
        result
            .missing
            .retain(|c| !matches!(c, SnapshotCapability::Terrain | SnapshotCapability::Fog));
        Ok(result)
    }
}
fn cell_json(observed: &ObservedRow) -> Value {
    let row = &observed.row;
    let d = &row.data;
    let mut data = json!({"ground":char::from(d.ground).to_string(),"object":char::from(d.object).to_string(),
        "material":d.material,"groundMaterial":d.ground_material,"groundWear":f64::from(d.ground_wear)});
    if let Some(front) = d.trench_front {
        data["trenchFront"] = json!(f64::from(front));
    }
    if d.mine {
        data["mine"] = json!(true);
        if let Some(material) = &d.mine_material {
            data["mineMaterial"] = json!(material);
        }
    }
    if let Some(owned) = observed.mine_owned {
        data["mineOwned"] = json!(owned);
    }
    if let Some(s) = &d.section {
        data["section"] = json!({"id":s.id,"structureId":s.structure_id,"state":s.state,"hp":s.hp,"maxHp":s.max_hp,"material":s.material,"anchor":s.anchor});
    }
    json!([
        row.cell,
        char::from(row.ch).to_string(),
        row.height,
        row.state,
        data
    ])
}
fn pack(a: &[u8], base: Option<&[u8]>) -> String {
    const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    let mut last = 0;
    let mut on = false;
    let put = |out: &mut String, mut n: usize| {
        while n >= 32 {
            out.push(char::from(DIGITS[32 | (n & 31)]));
            n /= 32;
        }
        out.push(char::from(DIGITS[n]));
    };
    for (i, v) in a.iter().enumerate() {
        if (*v != base.map_or(0, |b| b[i])) != on {
            put(&mut out, i - last);
            last = i;
            on = !on;
        }
    }
    if on {
        put(&mut out, a.len() - last);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{game::Point, grid::SpatialGrid};
    #[test]
    fn terrain_and_fog_follow_actual_source_delivery_sequence() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/recipient-delivery.json"))
                .unwrap();
        let mut state = super::super::tests::initialize(&fixture["initial"]);
        let mut delivery = DeliveryState::new(&state).unwrap();
        for action in fixture["actions"].as_array().unwrap() {
            if let Some(tick) = action["tick"].as_i64() {
                state.world.game.tick = tick;
            }
            if let Some(cell) = action["cell"].as_u64() {
                state
                    .world
                    .set_cell(cell as usize, action["ch"].as_str().unwrap().as_bytes()[0])
                    .unwrap();
            }
            if let Some(movement) = action["move"].as_object() {
                let u = state
                    .world
                    .game
                    .units
                    .get_mut(&(movement["id"].as_u64().unwrap() as u32))
                    .unwrap();
                u.x = movement["x"].as_f64().unwrap();
                u.z = movement["z"].as_f64().unwrap();
            }
            if action["vision"] == true {
                let grid = SpatialGrid::new(&state.world.game.units, 16.);
                vision::update_vision(&mut state.world.game, &state.rules, &grid);
            }
            let flags = |name: &str| {
                (0..3)
                    .map(|slot| action[name][slot].as_bool().unwrap_or(false))
                    .collect::<Vec<_>>()
            };
            let frame = delivery
                .capture(&mut state, &flags("full"), &flags("omit"))
                .unwrap();
            for slot in 0..3 {
                let fields = frame.recipient(slot).unwrap().fields;
                let actual = Value::Object(
                    fields
                        .into_iter()
                        .filter(|(k, _)| k == "cells" || k == "fog")
                        .collect(),
                );
                super::super::tests::equal(
                    &actual,
                    &action["outputs"][slot],
                    action["name"].as_str().unwrap(),
                );
            }
        }
    }
    #[test]
    fn rejected_owner_plan_preserves_delivery_ack_and_terrain_outbox() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/recipient-delivery.json"))
                .unwrap();
        let mut state = super::super::tests::initialize(&fixture["initial"]);
        let mut delivery = DeliveryState::new(&state).unwrap();
        delivery
            .capture(&mut state, &[true; 3], &[false; 3])
            .unwrap();
        let before = delivery.clone();
        let id = state
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0 && state.rules.kinds[u.kind].infantry)
            .unwrap()
            .id;
        state.commands.units.get_mut(&id).unwrap().nade = Some(Point { x: f64::NAN, z: 0. });
        assert!(matches!(
            delivery.capture(&mut state, &[false; 3], &[false; 3]),
            Err(SnapshotError::InvalidNumber)
        ));
        assert_eq!(delivery, before);
        state.commands.units.get_mut(&id).unwrap().nade = None;
        let frame = delivery
            .capture(&mut state, &[false; 3], &[false; 3])
            .unwrap();
        assert!(
            frame.recipient(0).unwrap().fields["cells"]
                .as_array()
                .unwrap()
                .len()
                > 0
        );
    }
}
