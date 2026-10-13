//! Bounded World income and live region capture on the canonical Match.
//! The caller maintains the live grid and supplies the tick's ordered unit IDs.
//! This phase does not advance time or run the rest of a World tick.

use crate::{
    catalog::{Catalog, Value},
    economy::Mode,
    economy_state as economy,
    game::{CELL, LOCAL_DEFENDER_OWNER, ModeKind, Point},
    match_runtime::Match,
    match_story::{StoryChange, StoryKey, prepare_story_updates},
    math,
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldCapture {
    pub region_index: usize,
    pub owner: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ProductionLock {
    Guard,
    Team(i32),
}

fn n(value: &Value, key: &str) -> Result<f64, String> {
    value
        .get(key)
        .and_then(Value::number)
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("missing World phase rule {key}"))
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => *v != 0.0 && !v.is_nan(),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}

impl Match {
    /// Source income precedes region capture. Prepare every consequence before publishing.
    /// Unsupported fort transfer, army elimination and finish paths leave the phase intact.
    pub fn apply_world_capture_income(
        &mut self,
        unit_order: &[u32],
        dt: f64,
    ) -> Result<Vec<WorldCapture>, String> {
        if self.options.mode != Mode::World
            || self.world.game.mode != Some(ModeKind::World)
            || self.options.logistics
            || self.finish.is_some()
        {
            return Err(
                "World region phase requires undecided World with logistics disabled".into(),
            );
        }
        if !dt.is_finite() || dt < 0.0 {
            return Err("invalid World phase dt".into());
        }
        let economy_regions = self.world_economy_regions()?;
        let geography = self.world.geography().ok_or("missing World geography")?;
        let game = &self.world.game;
        let players = &self.start.economy.players;
        if !(1..=6).contains(&players.len())
            || players.len() != game.players.len()
            || players.len() != self.start.players.len()
            || players.len() != self.story.len()
            || players.len() != self.options.players.len()
            || game.tick < 0
            || game.tick > 9_007_199_254_740_991
        {
            return Err("invalid World phase shape".into());
        }
        for (slot, p) in players.iter().enumerate() {
            if !(0..6).contains(&p.team)
                || game.players[slot].slot != slot
                || p.team != game.players[slot].team
                || p.team != self.start.players[slot].config.team
                || p.team != self.options.players[slot].team
                || !self.start.players[slot].spawn.x.is_finite()
                || !self.start.players[slot].spawn.z.is_finite()
            {
                return Err("invalid World commander identity".into());
            }
        }
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        for (&id, u) in &game.units {
            if u.id != id
                || u.kind >= self.rules.kinds.len()
                || !u.hp.is_finite()
                || !u.x.is_finite()
                || !u.z.is_finite()
                || (u.owner != LOCAL_DEFENDER_OWNER && u.owner >= players.len())
            {
                return Err("invalid World phase unit".into());
            }
        }
        let mut seen = BTreeSet::new();
        let mut list = Vec::with_capacity(unit_order.len());
        for id in unit_order {
            if !seen.insert(*id) {
                return Err("duplicate World tick unit".into());
            }
            list.push(
                game.units
                    .get(id)
                    .ok_or("unsupported World income: removed tick unit facts")?,
            );
        }
        let queues: Vec<Vec<&str>> = list
            .iter()
            .map(|u| {
                self.production
                    .building(u.id)
                    .map_or_else(Vec::new, |p| p.queue().collect())
            })
            .collect();
        let income_units: Vec<_> = list
            .iter()
            .zip(&queues)
            .map(|(u, queue)| economy::Unit {
                id: u.id as u64,
                owner: (u.owner != LOCAL_DEFENDER_OWNER).then_some(u.owner),
                kind: &self.rules.kinds[u.kind].name,
                hp: u.hp,
                built: u.built.unwrap_or(0.0),
                queue,
            })
            .collect();
        let current_depots: Vec<_> = game
            .units
            .values()
            .map(|u| economy::DepotFacts {
                id: u.id as u64,
                owner: (u.owner != LOCAL_DEFENDER_OWNER).then_some(u.owner),
                hp: u.hp,
                built: u.built.unwrap_or(0.0),
            })
            .collect();
        let runtime = self.world_runtime.as_ref().ok_or("missing World runtime")?;
        let nodes = runtime
            .nodes
            .iter()
            .map(|node| {
                Ok(economy::Node {
                    depot: node.depot.map(u64::from),
                    rate: node.rate,
                    fuel: node.fuel,
                    region: self.world_region_index(node.at)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut wallets = self.start.economy.clone();
        wallets
            .advance_income_with_depots(
                &economy::Context {
                    mode: economy::ModeContext {
                        kind: Mode::World,
                        defender_team: None,
                        income_excluded_slot: None,
                    },
                    army: self.options.army,
                    units: &income_units,
                    drops: &[],
                    points: &[],
                    nodes: &nodes,
                    world: Some(economy::WorldContext {
                        regions: &economy_regions,
                    }),
                },
                &current_depots,
                dt,
                false,
            )
            .map_err(|e| e.to_string())?;
        if wallets.players.iter().any(|p| {
            [p.mp, p.mun, p.fuel, p.inc]
                .into_iter()
                .chain(p.fuel_inc)
                .chain(p.upkeep)
                .any(|v| !v.is_finite())
        }) {
            return Err("nonfinite World income result".into());
        }

        let cfg = catalog.table("CFG").map_err(|e| e.to_string())?;
        let radius = n(cfg, "pointRadius")?;
        let capture_time = n(cfg, "captureTime")?;
        if capture_time <= 0.0 || radius < 0.0 {
            return Err("invalid World capture rules".into());
        }
        let rate = dt / capture_time;
        let mut production = vec![BTreeSet::new(); runtime.regions.len()];
        for u in game.units.values().filter(|u| u.hp > 0.0) {
            let name = &self.rules.kinds[u.kind].name;
            let definition = catalog.unit(name).map_err(|e| e.to_string())?;
            if !definition.get("produces").is_some_and(truthy) {
                continue;
            }
            if let Some(index) = self.world_region_index(u.at())? {
                production[index].insert(if name == "worldbase" {
                    ProductionLock::Guard
                } else {
                    ProductionLock::Team(game.team_of(u))
                });
            }
        }
        let mut regions = runtime.regions.clone();
        let mut captured = Vec::new();
        for (index, (r, definition)) in regions.iter_mut().zip(&geography.regions).enumerate() {
            if !r.progress.is_finite()
                || !(0.0..=1.0).contains(&r.progress)
                || !(-1..6).contains(&r.team)
                || r.capper.is_some_and(|s| s >= players.len())
                || r.cap_tick
                    .is_some_and(|t| !(-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&t))
            {
                return Err("invalid World region state".into());
            }
            let at = Point {
                x: (definition.x as f64 + 0.5) * CELL,
                z: (definition.y as f64 + 0.5) * CELL,
            };
            let on = self
                .grid
                .in_order(&game.units, (at.x, at.z), radius, |_, u| {
                    u.hp > 0.0
                        && !u.retreating
                        && !u.riding
                        && self.rules.kinds[u.kind].infantry
                        && !self
                            .logistics
                            .get(u.id)
                            .is_some_and(|reserve| reserve.forced)
                        && math::hypot(u.x - at.x, u.z - at.z) <= radius
                });
            let team = on.first().map_or(-1, |id| game.team_of(&game.units[id]));
            r.contested = on.iter().any(|id| game.team_of(&game.units[id]) != team);
            let locked = production[index].contains(&ProductionLock::Guard)
                || production[index].contains(&ProductionLock::Team(r.team));
            r.locked = Some(locked);
            if team < 0 || on.is_empty() || r.contested || (r.team != team && locked) {
                continue;
            }
            let owner = game.units[&on[0]].owner;
            if r.team == team {
                r.progress = 1.0;
                r.capper = None;
            } else if r.team >= 0 {
                r.progress = math::max(0.0, r.progress - rate);
                if r.progress == 0.0 {
                    r.team = -1;
                    r.capper = Some(owner);
                }
            } else {
                if r.capper.is_none_or(|s| players[s].team != team) {
                    r.capper = Some(owner);
                    r.progress = 0.0;
                }
                r.progress = math::min(1.0, r.progress + rate);
                if r.progress >= 1.0 {
                    r.team = team;
                    r.capper = None;
                    r.cap_tick = Some(game.tick);
                    for fort in game.units.values().filter(|u| {
                        u.hp > 0.0
                            && ["pillbox", "tower", "wall", "gate"]
                                .contains(&self.rules.kinds[u.kind].name.as_str())
                    }) {
                        if !game.allied(fort.owner, owner)
                            && self.world_region_index(fort.at())? == Some(index)
                        {
                            return Err(
                                "unsupported World capture: fort ownership and gate state".into()
                            );
                        }
                    }
                    captured.push(WorldCapture {
                        region_index: index,
                        owner,
                    });
                }
            }
        }
        let mut spawns: Vec<_> = self.start.players.iter().map(|p| p.spawn).collect();
        for (slot, wallet) in wallets.players.iter_mut().enumerate() {
            let own: Vec<_> = regions
                .iter()
                .enumerate()
                .filter(|(_, r)| r.team == wallet.team)
                .collect();
            if own.is_empty() {
                if game.units.values().any(|u| u.owner == slot) {
                    return Err(
                        "unsupported World phase: army elimination and terrain wrecking".into(),
                    );
                }
                wallet.out = true;
            } else if self
                .world_region_index(spawns[slot])?
                .is_none_or(|i| regions[i].team != wallet.team)
            {
                let mut best: Option<(Point, f64)> = None;
                for (index, _) in own {
                    let r = &geography.regions[index];
                    let at = Point {
                        x: (r.x as f64 + 0.5) * CELL,
                        z: (r.y as f64 + 0.5) * CELL,
                    };
                    let distance = math::hypot(at.x - spawns[slot].x, at.z - spawns[slot].z);
                    if best.is_none_or(|(_, d)| distance < d) {
                        best = Some((at, distance));
                    }
                }
                spawns[slot] = best.expect("owned region").0;
            }
        }
        let mut all_teams = BTreeSet::new();
        let mut alive = Vec::new();
        for p in &wallets.players {
            all_teams.insert(p.team);
            if !p.out && !alive.contains(&p.team) {
                alive.push(p.team);
            }
        }
        let total = geography.size.region_count();
        let tuning = catalog.table("WORLD_TUNING").map_err(|e| e.to_string())?;
        let goal = (total as f64 * n(tuning, "winShare")?).ceil() as usize;
        let rivals = all_teams.len() > 1;
        if alive.is_empty()
            || (rivals && alive.len() == 1)
            || alive.iter().any(|team| {
                let owned = regions.iter().filter(|r| r.team == *team).count();
                owned == total || (rivals && owned >= goal)
            })
        {
            return Err("unsupported World phase: canonical World finish".into());
        }
        let changes: Vec<_> = captured
            .iter()
            .map(|event| StoryChange {
                slot: Some(event.owner),
                key: StoryKey::Captures,
                amount: 1.0,
            })
            .collect();
        let story = prepare_story_updates(&mut self.story, false, &changes)?;
        self.world_runtime
            .as_mut()
            .expect("checked World runtime")
            .regions = regions;
        self.start.economy = wallets;
        for (player, at) in self.start.players.iter_mut().zip(spawns) {
            player.spawn = at;
        }
        story.commit();
        Ok(captured)
    }
}
