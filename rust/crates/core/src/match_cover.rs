//! Ordinary Take Cover with canonical paid-job cancellation and house-preserving cleanup.
use crate::{
    catalog::Catalog,
    command_destinations::{self, Context},
    game::Point,
    match_commands::{Command, Denial, MoveOutcome},
    match_runtime::Match,
    terrain,
};
use std::collections::BTreeSet;

impl Match {
    pub(crate) fn execute_cover_command(
        &mut self,
        owner: usize,
        ids: &[i64],
        queued: bool,
    ) -> Result<Option<Denial>, String> {
        let command = Command::Cover {
            ids: ids.to_vec(),
            queue: queued,
            at: None,
        };
        let controlled: Vec<_> = ids
            .iter()
            .take(50.max(self.world.game.units.len()))
            .filter_map(|id| self.eligible_command_unit(*id, owner, &command))
            .collect();
        let selected: Vec<_> = controlled
            .iter()
            .copied()
            .filter(|id| self.rules.kinds[self.world.game.units[id].kind].infantry)
            .collect();
        if selected.is_empty() {
            for id in controlled {
                self.commands
                    .units
                    .get_mut(&id)
                    .expect("controlled unit")
                    .follow = 0;
            }
            return Ok(Some(Denial::Needs));
        }
        // Do not clear a job flag while leaving its owning descriptor alive.
        if selected.iter().any(|id| {
            let u = &self.world.game.units[id];
            !u.retreating && (u.entrench || u.build)
        }) {
            return Ok(Some(Denial::Unsupported));
        }
        // Validate the sole job owner before any queue, path or observation mutation.
        for id in &selected {
            let unit = &self.world.game.units[id];
            if !unit.retreating && (unit.id != *id || unit.dig != self.field_works.is_active(*id)) {
                return Err("unrepresented or inconsistent cover field work".into());
            }
        }
        if queued {
            let mut queues = std::collections::BTreeMap::new();
            let mut accepted = false;
            let mut reason = Denial::NoCover;
            for id in selected {
                let u = &self.world.game.units[&id];
                if u.retreating {
                    reason = Denial::Retreating;
                    continue;
                }
                let orders = queues
                    .entry(id)
                    .or_insert_with(|| self.commands.units[&id].orders.clone());
                if orders.len() >= 8 {
                    reason = Denial::QueueFull;
                    continue;
                }
                let at = if let Some(last) = orders.back() {
                    match last {
                        Command::Move { orders, .. } => orders
                            .first()
                            .map(|r| r.at)
                            .ok_or("missing queued move anchor")?,
                        Command::Cover { at: Some(at), .. }
                        | Command::Garrison { at: Some(at), .. }
                        | Command::Ability { at: Some(at), .. }
                        | Command::Attack { at: Some(at), .. }
                        | Command::Assist { at: Some(at), .. }
                        | Command::Entrench {
                            order: crate::field_works::ProjectOrder::Join { at: Some(at), .. },
                            ..
                        } => *at,
                        _ => return Err("unrepresented queued cover anchor".into()),
                    }
                } else {
                    u.path.last().copied().unwrap_or(u.at())
                };
                if !at.x.is_finite() || !at.z.is_finite() {
                    return Err("nonfinite queued cover anchor".into());
                }
                orders.push_back(Command::Cover {
                    ids: vec![i64::from(id)],
                    queue: false,
                    at: Some(at),
                });
                accepted = true;
            }
            for id in controlled {
                self.commands
                    .units
                    .get_mut(&id)
                    .expect("controlled unit")
                    .follow = 0;
            }
            for (id, orders) in queues {
                self.commands
                    .units
                    .get_mut(&id)
                    .expect("queued control")
                    .orders = orders;
            }
            return Ok((!accepted).then_some(reason));
        }
        let radius = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("coverSeek")
            .and_then(|v| v.number())
            .ok_or("missing cover seek")?;
        let points: Vec<Point> = self.start.points.iter().map(|p| p.at).collect();
        let hit_at = self.combat.iter().map(|(id, c)| (id, c.hit_at)).collect();
        let path_rules = crate::pathfinding::PathRules::compiled().map_err(|e| e.to_string())?;
        let preview = crate::match_routing::prepare_recipient_routing(
            crate::match_routing::RecipientContext {
                world: &self.world,
                environment: &self.environment,
                rules: &self.rules,
                building_cells: &self.building_cells,
                start: &self.start,
                options: &self.options,
            },
            &mut self.recipients[owner],
            &mut self.routing_fog,
            owner,
            None,
        )?;
        // Fork existing derived history. Abort drops only this preview and its cache copy.
        let mut paths = preview.fork_paths(&self.commands.paths);
        let mut searched = false;
        // Later squads see earlier prepared routes when choosing occupied spots.
        let mut staged = self.world.game.clone();
        let mut prepared = Vec::new();
        let mut reason = Denial::NoCover;
        for id in selected {
            let mut u = staged.units[&id].clone();
            if u.retreating {
                reason = Denial::Retreating;
                continue;
            }
            if u.garrison.is_some() {
                // The source keeps the squad in its house and retains its stuck counter.
                let next_identity = u
                    .path_gen
                    .checked_add(1)
                    .ok_or("cover path identity exhausted")?;
                cleanup_unit(&mut u, false);
                u.path.clear();
                u.path_gen = next_identity;
                staged.units.insert(id, u.clone());
                prepared.push((id, u, None));
                continue;
            }
            let context = Context {
                game: &staged,
                rules: &self.rules,
                grid: &self.grid,
                points: &points,
                hit_at: &hit_at,
            };
            let from = command_destinations::threat_for(&context, id, u.at())
                .map_err(|e| format!("cover threat: {e:?}"))?;
            let rank = terrain::cover_rank(&staged, &self.rules, u.at(), from);
            let spot = if rank == 0 {
                None
            } else {
                let held = command_destinations::held_destinations(
                    &context,
                    &BTreeSet::from([id]),
                    u.at(),
                    radius,
                    owner,
                )
                .map_err(|e| format!("cover occupied spots: {e:?}"))?;
                let first = command_destinations::spot_near(
                    &context,
                    id,
                    u.at(),
                    radius,
                    &held,
                    from,
                    rank,
                    None,
                )
                .map_err(|e| format!("cover destination: {e:?}"))?;
                match first {
                    Some(p) => Some(p),
                    None => command_destinations::spot_near(
                        &context,
                        id,
                        u.at(),
                        radius,
                        &held,
                        from,
                        rank,
                        Some(0.0),
                    )
                    .map_err(|e| format!("cover packed destination: {e:?}"))?,
                }
            };
            if spot.is_none() && rank == 3 {
                continue;
            }
            let next_identity = u
                .path_gen
                .checked_add(1)
                .ok_or("cover path identity exhausted")?;
            let route = if let Some(at) = spot {
                searched = true;
                let query = preview.query();
                let terrain = query
                    .path_terrain(preview.live())
                    .map_err(|e| format!("cover recipient path: {e:?}"))?;
                Some(
                    crate::match_commands::search_route(
                        &mut paths,
                        terrain,
                        path_rules,
                        &self.rules.kinds[u.kind],
                        &u,
                        at,
                    )?
                    .result,
                )
            } else {
                None
            };
            cleanup_unit(&mut u, true);
            u.path = route
                .as_ref()
                .map_or_else(Vec::new, |r| r.waypoints.clone());
            u.path_gen = next_identity;
            staged.units.insert(id, u.clone());
            prepared.push((id, u, route.map(|r| r.route_end)));
        }
        if prepared.is_empty() {
            for id in controlled {
                self.commands
                    .units
                    .get_mut(&id)
                    .expect("controlled unit")
                    .follow = 0;
            }
            return Ok(Some(reason));
        }
        if searched {
            let (game, receipt) =
                preview.prepare_publication(&mut self.world, &self.environment)?;
            publish(
                game,
                &mut self.commands,
                &mut self.firing,
                &mut self.field_works,
                &mut self.abilities,
                controlled,
                prepared,
            );
            self.commands.paths = paths;
            receipt.commit();
        } else {
            // No source findPath call means no observation acknowledgments.
            drop(preview);
            publish(
                &mut self.world.game,
                &mut self.commands,
                &mut self.firing,
                &mut self.field_works,
                &mut self.abilities,
                controlled,
                prepared,
            );
        }
        Ok(None)
    }
}

fn publish(
    game: &mut crate::game::Game,
    commands: &mut crate::match_commands::MatchCommands,
    firing: &mut crate::match_firing::FiringState,
    field_works: &mut crate::field_works::FieldWorksRuntime,
    abilities: &mut crate::match_abilities::AbilityState,
    controlled: Vec<u32>,
    prepared: Vec<(u32, crate::game::Unit, Option<Point>)>,
) {
    for id in controlled {
        commands.units.get_mut(&id).expect("controlled unit").follow = 0;
    }
    for (id, u, end) in prepared {
        // All activity/identity checks preceded route preparation. Cancellation is
        // now infallible and removes the real descriptor before its derived flag.
        crate::match_runtime::cancel_field_work_state(
            field_works,
            game.units.get_mut(&id).expect("prepared cover unit"),
        )
        .expect("prevalidated cover field work");
        let c = commands.units.get_mut(&id).expect("prepared cover control");
        c.follow = 0;
        c.orders.clear();
        c.face = None;
        c.attack_id = 0;
        c.nade = None;
        if let Some(ability) = abilities.get_mut(id) {
            ability.aimed_auto = false;
            ability.aimed_walked = false;
        }
        c.enter = None;
        c.fire_at = None;
        c.rally_drift = false;
        c.outcome = Some(MoveOutcome::Interrupted);
        c.outcome_tick = Some(game.tick);
        if let Some(end) = end {
            c.route_end = Some(end);
            commands.requests.clear_failures(id);
        }
        if let Some(f) = firing.units.get_mut(&id) {
            f.target_id = 0;
        }
        game.units.insert(id, u);
    }
}

fn cleanup_unit(u: &mut crate::game::Unit, reset_stuck: bool) {
    u.traffic = None;
    u.traffic_wait = 0.0;
    u.world_goal = None;
    u.entrench = false;
    u.attack = false;
    if reset_stuck {
        u.stuck = 0.0;
    }
    u.dig = false;
    u.board = 0;
    u.amove = None;
    u.build = false;
    u.drift = false;
}
