//! Immediate ordinary Take Cover. Active field jobs and garrison cancellation
//! require their canonical owner callbacks and are refused before any mutation.
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
    ) -> Result<Option<Denial>, String> {
        let command = Command::Cover { ids: ids.to_vec() };
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
            !u.retreating && (u.dig || u.entrench || u.build || u.garrison.is_some())
        }) {
            return Ok(Some(Denial::Unsupported));
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
        let mut paths = self.commands.paths.clone();
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
            u.traffic = None;
            u.traffic_wait = 0.0;
            u.world_goal = None;
            u.entrench = false;
            u.attack = false;
            u.stuck = 0.0;
            u.dig = false;
            u.board = 0;
            u.amove = None;
            u.build = false;
            u.drift = false;
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
            let (game, receipt) = preview.prepare_publication(&mut self.world)?;
            publish(
                game,
                &mut self.commands,
                &mut self.firing,
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
    controlled: Vec<u32>,
    prepared: Vec<(u32, crate::game::Unit, Option<Point>)>,
) {
    for id in controlled {
        commands.units.get_mut(&id).expect("controlled unit").follow = 0;
    }
    for (id, u, end) in prepared {
        let c = commands.units.get_mut(&id).expect("prepared cover control");
        c.follow = 0;
        c.orders.clear();
        c.face = None;
        c.attack_id = 0;
        c.nade = None;
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
