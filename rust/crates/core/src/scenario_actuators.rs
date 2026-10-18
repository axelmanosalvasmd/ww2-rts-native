//! Authored scenario effects through the ordinary spawn, command and terrain owners.
use std::collections::BTreeSet;

use serde_json::Value;

use crate::{
    catalog::Catalog,
    economy::{self, Mode},
    economy_state::{self, ModeContext, PendingDrop},
    environment::{Blast, DamageContext, Effect},
    game::{CELL, Point, StrikeKind},
    match_combat::DamageWorld,
    match_commands::{Command, MoveOrder},
    match_runtime::Match,
    unit_publication::StagedUnit,
};

fn cell_pair(value: &Value, width: usize, height: usize) -> Option<[usize; 2]> {
    let pair = value.as_array().filter(|a| a.len() == 2)?;
    let coordinate = |value: &Value, limit: usize| {
        value
            .as_f64()
            .filter(|n| n.is_finite() && *n >= 0.0 && n.fract() == 0.0 && *n < limit as f64)
            .map(|n| n as usize)
    };
    Some([coordinate(&pair[0], width)?, coordinate(&pair[1], height)?])
}

fn center([x, y]: [usize; 2]) -> Point {
    Point {
        x: (x as f64 + 0.5) * CELL,
        z: (y as f64 + 0.5) * CELL,
    }
}

impl Match {
    fn scenario_ordinary_mode(&self) -> bool {
        matches!(
            self.options.mode,
            Mode::Default
                | Mode::Conquest
                | Mode::Assault
                | Mode::Annihilation
                | Mode::Classic
                | Mode::Horde
        ) && crate::match_actor::ordinary_ground_mode(&self.world.game, self.options.mode)
            && self.world.geography().is_none()
            && self.world_runtime.is_none()
    }

    pub(crate) fn scenario_type_allowed(&self, side: usize, name: &str) -> bool {
        let Some(player) = self.start.players.get(side) else {
            return false;
        };
        let Some(profile) = self.rules.kind(name).map(|kind| &self.rules.kinds[kind]) else {
            return false;
        };
        let Ok(catalog) = Catalog::compiled() else {
            return false;
        };
        let Ok(definition) = catalog.unit(name) else {
            return false;
        };
        !profile.structure
            && !profile.building
            && !profile.air
            && economy::can_build(name, player.config.faction).unwrap_or(false)
            && (!definition
                .get("classic")
                .and_then(crate::catalog::Value::boolean)
                .unwrap_or(false)
                || self.options.mode.is_construction())
            && (!profile.naval || self.world.game.naval)
    }

    /// Initial authored units must fit the current terrain after ordinary startup edits.
    pub(crate) fn scenario_authored_unit_allowed(
        &self,
        side: usize,
        name: &str,
        at: [usize; 2],
    ) -> bool {
        let game = &self.world.game;
        self.scenario_ordinary_mode()
            && self.horde.as_ref().is_none_or(|mode| mode.slot != side)
            && side < game.players.len()
            && self.scenario_type_allowed(side, name)
            && at[0] < game.w
            && at[1] < game.h
            && self.rules.kind(name).is_some_and(|kind| {
                game.flags[at[1] * game.w + at[0]] & self.rules.kinds[kind].block() == 0
            })
    }

    /// Startup admission uses immutable terrain, before any mission damage or arrivals.
    pub(crate) fn scenario_action_allowed(&self, action: &Value, side: usize) -> bool {
        if !self.scenario_ordinary_mode() {
            return false;
        }
        if action["kind"] != "reinforce" {
            return true;
        }
        let game = &self.world.game;
        if self.horde.as_ref().is_some_and(|mode| mode.slot == side)
            || side >= game.players.len()
            || self.start.economy.players.get(side).is_none_or(|p| p.out)
        {
            return false;
        }
        let Some([x, y]) = cell_pair(&action["at"], game.w, game.h) else {
            return false;
        };
        let cell = y * game.w + x;
        let initial = self.world.initial();
        let Some((&ground, &object)) = initial.ground.get(cell).zip(initial.objects.get(cell))
        else {
            return false;
        };
        let flags = crate::map::flags(crate::map::compose_world_cell(ground, object, false));
        action["roster"].as_array().is_some_and(|roster| {
            roster.iter().all(|entry| {
                entry["type"].as_str().is_some_and(|name| {
                    self.scenario_type_allowed(side, name)
                        && self
                            .rules
                            .kind(name)
                            .is_some_and(|kind| flags & self.rules.kinds[kind].block() == 0)
                })
            })
        })
    }

    fn scenario_population(&self, side: usize) -> Result<(f64, f64), String> {
        let game = &self.world.game;
        let queues: Vec<Vec<&str>> = game
            .units
            .keys()
            .map(|id| {
                self.production
                    .building(*id)
                    .map_or_else(Vec::new, |building| building.queue().collect())
            })
            .collect();
        let units = game
            .units
            .values()
            .zip(&queues)
            .map(|(unit, queue)| {
                let profile = self
                    .rules
                    .kinds
                    .get(unit.kind)
                    .ok_or("scenario unit kind")?;
                Ok(economy_state::Unit {
                    id: u64::from(unit.id),
                    owner: Some(unit.owner),
                    kind: &profile.name,
                    hp: unit.hp,
                    built: unit.built.unwrap_or(0.0),
                    queue,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let drops = game
            .strikes
            .iter()
            .map(|strike| {
                if strike.kind == StrikeKind::Other && !strike.live && strike.runtime.is_none() {
                    return Err("scenario pending support needs exact kind metadata".to_owned());
                }
                Ok(PendingDrop {
                    owner: strike.owner,
                    kind: strike.runtime.as_ref().map_or("recon", |s| s.kind.name()),
                    live: strike.live,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let context = economy_state::Context {
            mode: ModeContext {
                kind: self.options.mode,
                ..ModeContext::default()
            },
            army: self.options.army,
            units: &units,
            drops: &drops,
            points: &[],
            nodes: &[],
            world: None,
        };
        Ok((
            self.start
                .economy
                .population(&context, side)
                .map_err(|e| e.to_string())?,
            self.start
                .economy
                .population_cap(&context, side)
                .map_err(|e| e.to_string())?,
        ))
    }

    /// A blocked or full arrival remains pending. Publication initializes every ordinary sidecar.
    pub(crate) fn scenario_arrive(
        &mut self,
        side: usize,
        name: &str,
        at: [usize; 2],
        order: &Value,
    ) -> Result<Option<u32>, String> {
        if !self.scenario_ordinary_mode() {
            return Err("scenario arrivals require an ordinary match".into());
        }
        let game = &self.world.game;
        if self.horde.as_ref().is_some_and(|mode| mode.slot == side)
            || !self.scenario_type_allowed(side, name)
            || side >= game.players.len()
            || self.start.economy.players.get(side).is_none_or(|p| p.out)
            || at[0] >= game.w
            || at[1] >= game.h
        {
            return Ok(None);
        }
        let kind = self.rules.kind(name).ok_or("scenario unit kind")?;
        let profile = &self.rules.kinds[kind];
        if game.flags[at[1] * game.w + at[0]] & profile.block() != 0 {
            return Ok(None);
        }
        let (population, cap) = self.scenario_population(side)?;
        let have = game
            .units
            .values()
            .filter(|u| u.owner == side)
            .map(|unit| {
                usize::from(unit.kind == kind)
                    + self.production.building(unit.id).map_or(0, |building| {
                        building.queue().filter(|queued| *queued == name).count()
                    })
            })
            .sum::<usize>();
        let definition = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .unit(name)
            .map_err(|e| e.to_string())?;
        if population + economy::pop_use(name).map_err(|e| e.to_string())? > cap
            || definition
                .get("max")
                .and_then(crate::catalog::Value::number)
                .is_some_and(|max| have as f64 >= max)
        {
            return Ok(None);
        }
        let mut position = None;
        'search: for radius in 0isize..=3 {
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let Some(x) = at[0].checked_add_signed(dx).filter(|x| *x < game.w) else {
                        continue;
                    };
                    let Some(y) = at[1].checked_add_signed(dy).filter(|y| *y < game.h) else {
                        continue;
                    };
                    if game.flags[y * game.w + x] & profile.block() != 0 {
                        continue;
                    }
                    let point = center([x, y]);
                    if game.units.values().any(|unit| {
                        unit.air.is_none()
                            && crate::math::hypot(unit.x - point.x, unit.z - point.z)
                                < (profile.radius * 0.4).max(0.6)
                    }) {
                        continue;
                    }
                    position = Some(point);
                    break 'search;
                }
            }
        }
        let Some(position) = position else {
            return Ok(None);
        };
        let id = self.next_unit_id;
        let command = match order["kind"].as_str() {
            Some("hold") => Command::Stop {
                ids: vec![i64::from(id)],
            },
            Some(kind @ ("move" | "attackMove")) => Command::Move {
                orders: vec![MoveOrder {
                    unit: i64::from(id),
                    at: center(
                        cell_pair(&order["at"], game.w, game.h)
                            .ok_or("scenario order destination is outside the map")?,
                    ),
                }],
                control_ids: None,
                attack_move: kind == "attackMove",
                queue: false,
                together: false,
                face: None,
            },
            _ => return Err("invalid scenario arrival order".into()),
        };
        let draft = StagedUnit::ordinary(
            game,
            &self.rules,
            side,
            kind,
            id,
            u32::try_from(game.units.len()).map_err(|_| "scenario unit count overflow")?,
            self.start.players[side].spawn,
            Some(position),
        )
        .map_err(|error| format!("scenario spawn: {error:?}"))?;
        let reserves = self.prepare_new_unit_reserves(&draft.unit)?;
        self.publish_starting_unit(draft)?;
        if let Some(reserves) = reserves {
            self.logistics.insert(id, reserves);
        }
        // Source command admission may ignore an order for an away commander.
        // The arrival still exists, and a later group order uses the same owner.
        self.execute_command(side, &command, self.finish.is_some())?;
        Ok(Some(id))
    }

    /// Damage each authored cell through the canonical terrain and structural damage path.
    pub(crate) fn scenario_damage_emitting(
        &mut self,
        action: &Value,
        execution: &str,
        emit: &mut dyn FnMut(Effect),
    ) -> Result<(), String> {
        if !self.scenario_ordinary_mode() {
            return Err("scenario damage requires an ordinary match".into());
        }
        let game = &self.world.game;
        let bounds = action["box"]
            .as_array()
            .filter(|bounds| bounds.len() == 4)
            .ok_or("invalid scenario damage bounds")?;
        let [x0, y0] = cell_pair(&Value::Array(bounds[..2].to_vec()), game.w, game.h)
            .ok_or("scenario damage bounds are outside the map")?;
        let [x1, y1] = cell_pair(&Value::Array(bounds[2..].to_vec()), game.w, game.h)
            .filter(|[x1, y1]| x0 <= *x1 && y0 <= *y1)
            .ok_or("invalid scenario damage bounds")?;
        let amount = match action["kind"].as_str() {
            Some("destroy") => 100_000.0,
            Some("damage") => action["damage"]
                .as_f64()
                .filter(|n| n.is_finite() && (0.01..=100_000.0).contains(n))
                .ok_or("invalid scenario damage amount")?,
            _ => return Err("invalid scenario damage action".into()),
        };
        let width = game.w;
        let unit_order: Vec<_> = game.units.keys().copied().collect();
        let skipped = BTreeSet::new();
        let mut damage = DamageWorld {
            world: &mut self.world,
            combat: &mut self.combat,
            grid: &mut self.grid,
            rules: &self.rules,
            logistics: &self.logistics,
            production: &self.production,
            mode: self.options.mode,
            tech_enabled: self.options.tech,
            environment: &mut self.environment,
            environment_seed: &mut self.start.environment_seed,
            next_id: &mut self.next_unit_id,
            unit_order: &unit_order,
            wind_angle: self.start.wind.0,
            wind_strength: self.start.wind.1,
        };
        for y in y0..=y1 {
            for x in x0..=x1 {
                let contact = format!("{execution}:{}", y * width + x);
                let mut hit = DamageContext::plain(&skipped);
                hit.contact_id = Some(&contact);
                damage
                    .terrain_blast_emitting(
                        Blast {
                            at: center([x, y]),
                            radius: CELL * 0.49,
                            damage: amount,
                            scar: 0.0,
                        },
                        &mut hit,
                        emit,
                    )
                    .map_err(|error| format!("scenario damage: {error}"))?;
            }
        }
        Ok(())
    }
}
