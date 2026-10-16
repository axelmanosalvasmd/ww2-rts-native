//! Detached ordinary commander observations and owner-filtered alert frames.
//! Strategic sighting memory stays private to the seat, separately from camera attention.

use crate::{
    catalog::{Catalog, Value as RuleValue},
    commander_ai::{Input, Observation},
    environment,
    game::Point,
    human_commander::{
        HumanSighting, HumanUnit, HumanView, Hurt, OwnerHurtBatch, ScreenView, TickControl,
    },
    human_commander_inputs,
    match_firing::LaunchEvent,
    match_routing::{RecipientContext, prepare_recipient_routing},
    match_runtime::Match,
    match_support::SupportEvent,
    math,
};
use serde_json::json;
use std::collections::BTreeMap;

mod recipient_packet;

const SAFE_TICK: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, PartialEq)]
struct Sighting {
    id: u32,
    kind: String,
    owner: usize,
    at: Point,
    time: f64,
    worth: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct Stationary {
    id: u32,
    at: Point,
    since: f64,
}

/// The source AI view memory, not human camera attention or policy intent.
/// Reset by replacing this value when a person resumes the seat or the match changes.
#[derive(Clone, Debug, PartialEq)]
pub struct HumanObservationMemory {
    owner: usize,
    binding: Option<(usize, usize, usize, u64)>,
    tick: Option<i64>,
    sightings: Vec<Sighting>,
    still: Vec<Stationary>,
}
impl HumanObservationMemory {
    pub fn new(owner: usize) -> Self {
        Self {
            owner,
            binding: None,
            tick: None,
            sightings: Vec::new(),
            still: Vec::new(),
        }
    }
    pub fn owner(&self) -> usize {
        self.owner
    }
}

/// A borrowed event projection from the caller's complete, ordered shot frame.
/// Each non-hurt row still occupies its original ordinal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HumanShot {
    Other,
    Hurt { owner: usize, unit: u32, at: Point },
}
impl HumanShot {
    /// Project each row of the caller's one chronological frame without filtering ordinals.
    /// Frame replacement generation is supplied separately by the host.
    pub fn tick(event: &crate::match_tick::TickEvent) -> Self {
        match event {
            crate::match_tick::TickEvent::Firing(event) => Self::firing(event),
            crate::match_tick::TickEvent::Support(event) => Self::support(event),
            crate::match_tick::TickEvent::Environment(event) => Self::environment(event),
            crate::match_tick::TickEvent::Aviation(_)
            | crate::match_tick::TickEvent::Scenario(_)
            | crate::match_tick::TickEvent::Horde(_) => Self::Other,
        }
    }

    pub fn environment(event: &environment::Effect) -> Self {
        match event {
            environment::Effect::Hurt(h) => Self::Hurt {
                owner: h.owner,
                unit: h.unit,
                at: h.at,
            },
            _ => Self::Other,
        }
    }
    pub fn firing(event: &LaunchEvent) -> Self {
        match event {
            LaunchEvent::Environment(e) => Self::environment(e),
            _ => Self::Other,
        }
    }
    pub fn support(event: &SupportEvent) -> Self {
        match event {
            SupportEvent::Environment(e) => Self::environment(e),
            _ => Self::Other,
        }
    }
}

fn finite(at: Point) -> bool {
    at.x.is_finite() && at.z.is_finite()
}

/// Rebuild the current strategic packet only from the camera-filtered detached rows.
/// No policy callback needs to consult Match or recover a filtered-out unit.
pub fn human_screen_observation(
    screen: &ScreenView<'_, Observation>,
) -> Result<Observation, String> {
    let mut input = screen.packet.input().clone();
    if screen.tick != input.tick as u64
        || screen
            .spawn
            .is_some_and(|spawn| spawn != input.players[input.slot].spawn)
        || screen
            .units
            .windows(2)
            .any(|rows| rows[0].motion.id >= rows[1].motion.id)
    {
        return Err("inconsistent human filtered frame".into());
    }
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let mut ids = std::collections::BTreeSet::new();
    for human in &screen.units {
        let index = input
            .units
            .binary_search_by_key(&human.motion.id, |u| u.id)
            .map_err(|_| "human filtered unit is absent from strategic packet")?;
        let row = &input.units[index];
        let definition = catalog.unit(&row.kind).map_err(|e| e.to_string())?;
        if human.owner as usize != row.owner
            || human.kind != row.kind
            || human.motion.at != row.at
            || human.hp != row.hp
            || human.motion.target_id != row.target_id
            || human.motion.retreating != row.retreating
            || human.build != row.build
            || human.dig != row.dig
            || human.entrench != row.entrench
            || human.garrison != row.garrison
            || human.air
                != definition
                    .get("air")
                    .and_then(RuleValue::boolean)
                    .unwrap_or(false)
            || human.motion.world_goal.is_some_and(|p| !finite(p))
            || human.motion.amove.is_some_and(|p| !finite(p))
            || (row.owner != input.slot
                && (human.motion.world_goal.is_some() || human.motion.amove.is_some()))
            || (human.motion.world_goal.is_some() && human.motion.amove.is_some())
            || ((row.retreating
                || row.attack_id
                || row.enter
                || row.fire_at
                || row.nade
                || row.dig
                || definition
                    .get("structure")
                    .and_then(RuleValue::boolean)
                    .unwrap_or(false))
                && (human.motion.world_goal.is_some() || human.motion.amove.is_some()))
        {
            return Err("human filtered unit differs from delivered strategic facts".into());
        }
        ids.insert(row.id);
    }
    let expected_visible: Vec<_> = input
        .units
        .iter()
        .filter(|u| {
            ids.contains(&u.id) && input.players[u.owner].team != input.players[input.slot].team
        })
        .map(|u| u.id)
        .collect();
    if screen.visible != expected_visible {
        return Err("human filtered visibility differs from delivered unit facts".into());
    }
    input.units.retain(|u| ids.contains(&u.id));
    Observation::new(input).map_err(|e| format!("human filtered strategic observation: {e:?}"))
}

impl Match {
    /// Live stop controls are independent of the age of a delivered observation.
    pub fn human_commander_control<'a>(
        &self,
        owner: usize,
        level: &'a str,
    ) -> Result<TickControl<'a>, String> {
        let g = &self.world.game;
        if !(0..=SAFE_TICK).contains(&g.tick)
            || owner >= g.players.len()
            || self.start.economy.players.len() != g.players.len()
        {
            return Err("invalid human commander live controls".into());
        }
        Ok(TickControl {
            now: g.tick as u64,
            live_owner_out: self.start.economy.players[owner].out,
            winner: self.finish.is_some(),
            level,
        })
    }

    /// The caller supplies the real shot-frame generation and original event order.
    /// This reads no unit position, visibility, private intent or other player's economy.
    pub fn human_commander_hurts(
        &self,
        owner: usize,
        generation: u64,
        shots: &[HumanShot],
    ) -> Result<OwnerHurtBatch, String> {
        if owner >= self.world.game.players.len() || shots.len() as u64 > SAFE_TICK as u64 {
            return Err("invalid human commander hurt frame".into());
        }
        let mut events = Vec::new();
        for (ordinal, shot) in shots.iter().enumerate() {
            let HumanShot::Hurt {
                owner: recipient,
                unit,
                at,
            } = *shot
            else {
                continue;
            };
            // Source readAlerts ignores a missing victim or a nonfinite x before considering z.
            if recipient != owner || unit == 0 || !at.x.is_finite() {
                continue;
            }
            if !at.z.is_finite() {
                return Err("invalid human commander hurt coordinates".into());
            }
            events.push(Hurt {
                ordinal: ordinal as u64,
                id: unit,
                at,
            });
        }
        Ok(OwnerHurtBatch {
            generation,
            through_ordinal: shots.len() as u64,
            events,
        })
    }

    /// Build one complete existing strategic packet plus human-facing delivered facts.
    /// The only publication is the canonical recipient receipt and this seat's view memory.
    pub fn human_commander_observation(
        &mut self,
        memory: &mut HumanObservationMemory,
    ) -> Result<HumanView<Observation>, String> {
        let owner = memory.owner;
        if self.options.logistics
            || self.logistics.iter().next().is_some()
            || self.convoys.is_some()
        {
            return Err("human commander requires delivered logistics support".into());
        }
        let g = &self.world.game;
        if g.units.iter().any(|(id, u)| {
            *id != u.id
                || u.kind >= self.rules.kinds.len()
                || u.owner >= g.players.len()
                || !u.hp.is_finite()
        }) {
            return Err("invalid canonical human observation unit identity or owner".into());
        }
        self.recipient_routing(owner)?;
        let g = &self.world.game;
        if self.options.mode == crate::economy::Mode::World
            || g.mode.is_some()
            || !(0..=SAFE_TICK).contains(&g.tick)
        {
            return Err("human commander requires an ordinary observation boundary".into());
        }
        let expected = (g.tick > 0).then(|| g.tick - (g.tick - 1) % 4);
        if g.vision_tick != expected {
            return Err("human commander requires the canonical vision phase".into());
        }
        if self
            .field_works
            .jobs()
            .keys()
            .any(|id| !g.units.contains_key(id))
            || g.units.values().any(|u| {
                u.dig != self.field_works.is_active(u.id)
                    || u.entrench != self.field_works.membership(u.id).is_some()
            })
        {
            return Err(
                "human observation requires canonical paid field and project parity".into(),
            );
        }
        let binding = (
            self.world.initial().chars.as_ptr() as usize,
            g.w,
            g.h,
            self.options.seed,
        );
        if memory.binding.is_some_and(|b| b != binding)
            || memory.tick.is_some_and(|t| t > g.tick)
            || memory.sightings.len() > 65_536
        {
            return Err("human observation memory binding or clock mismatch".into());
        }
        // Use the exact recipient producer for every dynamic fact, including own plans.
        // Its partial capability list stays separate from complete delivery admission.
        let snapshot = crate::recipient_snapshot::SnapshotCache::new(self)
            .and_then(|cache| cache.recipient(owner))
            .map_err(|error| format!("human recipient projection: {error:?}"))?;
        let lists = self.prepare_ordinary_recipient_lists(&[owner])?;
        let found_mines = lists.results()[0].1.found_mines.clone();
        let public = recipient_packet::PublicFacts::capture(self, owner)?;
        let mut snapshot = serde_json::Value::Object(snapshot.fields);
        snapshot["foundMines"] = json!(found_mines);
        let (mut input, units) = public.decode(&snapshot)?;
        let visible: Vec<_> = input
            .units
            .iter()
            .filter(|u| input.players[u.owner].team != input.players[owner].team)
            .map(|u| u.id)
            .collect();
        let now = g.tick as f64 / 20.0;
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let terrain = catalog.table("TERRAIN").map_err(|e| e.to_string())?;
        let mut staged = memory.clone();
        staged.binding = Some(binding);
        staged.tick = Some(g.tick);
        let visible_set: std::collections::BTreeSet<_> = visible.iter().copied().collect();
        staged.still.retain(|old| visible_set.contains(&old.id));
        let mut still_positions: BTreeMap<_, _> = staged
            .still
            .iter()
            .enumerate()
            .map(|(index, old)| (old.id, index))
            .collect();
        let mut positions: BTreeMap<_, _> = staged
            .sightings
            .iter()
            .enumerate()
            .map(|(index, sighting)| (sighting.id, index))
            .collect();
        for row in input
            .units
            .iter()
            .filter(|u| input.players[u.owner].team != input.players[owner].team)
        {
            if let Some(&index) = still_positions.get(&row.id) {
                let old = &staged.still[index];
                if math::hypot(old.at.x - row.at.x, old.at.z - row.at.z) > 0.1 + 1e-9 {
                    staged.still[index] = Stationary {
                        id: row.id,
                        at: row.at,
                        since: now,
                    };
                }
            } else {
                still_positions.insert(row.id, staged.still.len());
                staged.still.push(Stationary {
                    id: row.id,
                    at: row.at,
                    since: now,
                });
            }
            let definition = catalog.unit(&row.kind).map_err(|e| e.to_string())?;
            if definition
                .get("structure")
                .and_then(RuleValue::boolean)
                .unwrap_or(false)
            {
                continue;
            }
            let number = |name| {
                definition
                    .get(name)
                    .and_then(RuleValue::number)
                    .filter(|v| v.is_finite())
                    .ok_or("invalid human sighting unit rule")
            };
            let worth = number("cost")? * row.hp / (number("models")? * number("hpPer")?);
            if !worth.is_finite() {
                return Err("invalid human sighting worth".into());
            }
            let sighting = Sighting {
                id: row.id,
                kind: row.kind.clone(),
                owner: row.owner,
                at: row.at,
                time: now,
                worth,
            };
            if let Some(&index) = positions.get(&row.id) {
                staged.sightings[index] = sighting;
            } else {
                positions.insert(row.id, staged.sightings.len());
                staged.sightings.push(sighting);
            }
        }
        staged.sightings.retain(|s| now - s.time <= 60.0);
        if staged.sightings.len() > 65_536 {
            return Err("human observation sighting capacity exhausted".into());
        }
        let sightings = staged.sightings.iter().map(|s| HumanSighting { id: s.id,
            payload: json!({"id":s.id,"type":s.kind,"owner":s.owner,"x":s.at.x,"z":s.at.z,"t":s.time,"val":s.worth}) }).collect();
        let spawn = Some(self.start.players[owner].spawn);
        let preview = prepare_recipient_routing(
            RecipientContext {
                world: &self.world,
                rules: &self.rules,
                building_cells: &self.building_cells,
                start: &self.start,
                options: &self.options,
                environment: &self.environment,
            },
            &mut self.recipients[owner],
            &mut self.routing_fog,
            owner,
            None,
        )?;
        snapshot["cells"] = json!(
            preview
                .remembered()
                .iter()
                .map(|observed| {
                    let row = &observed.row;
                    json!([row.cell, char::from(row.ch).to_string(), row.height, row.state,
                {"mineOwned": observed.mine_owned.unwrap_or(false)}])
                })
                .collect::<Vec<_>>()
        );
        public.populate_mines(&mut input, &snapshot)?;
        apply_view_rows(&mut input, preview.remembered(), terrain)?;
        let packet =
            Observation::new(input).map_err(|e| format!("human strategic observation: {e:?}"))?;
        let view = HumanView {
            tick: packet.input().tick as u64,
            spawn,
            units,
            visible,
            sightings,
            packet,
        };
        let (_, commit) = preview.prepare_publication(&mut self.world, &self.environment)?;
        commit.commit_with_paths(&mut self.commands.paths);
        *memory = staged;
        Ok(view)
    }
}

fn apply_view_rows(
    input: &mut Input,
    rows: &[crate::observed_memory::ObservedRow],
    terrain: &RuleValue,
) -> Result<(), String> {
    for observed in rows {
        let row = &observed.row;
        let surface = if row.data.object != b'.' {
            row.data.object
        } else {
            row.data.ground
        };
        let flag = terrain
            .get(&char::from(if row.ch == b'N' { surface } else { row.ch }).to_string())
            .and_then(RuleValue::number)
            .unwrap_or(0.0);
        if row.cell >= input.chars.len()
            || !flag.is_finite()
            || flag < 0.0
            || flag > u16::MAX as f64
        {
            return Err("invalid human observed terrain row".into());
        }
        // ai-view retains the received N character while decoding its delivered surface mask.
        input.chars[row.cell] = row.ch;
        input.flags[row.cell] = flag as u16;
        if let Some(runtime) = &mut input.runtime
            && let Some(height) = &mut runtime.height
        {
            height[row.cell] = row.height;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
