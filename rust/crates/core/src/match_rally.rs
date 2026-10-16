//! Rally updates use the existing producer and commander state.
use crate::{
    catalog::Catalog,
    economy_state::{EconomyState, ModeContext},
    game::{CELL, MOVE, Point},
    match_commands::Denial,
    match_runtime::Match,
    math,
    navigation::TerrainView,
    production::UnitView,
};

struct RallyContext<'a> {
    mode: ModeContext,
    terrain: TerrainView<'a>,
    units: &'a [UnitView<'a>],
    wallets: &'a EconomyState,
}

#[derive(Debug, PartialEq)]
enum RallyPlan {
    Denied(Denial),
    Unchanged,
    Personal(Point),
    Buildings(Vec<u32>, Point),
}

fn plan_rally(
    c: &RallyContext<'_>,
    owner: usize,
    ids: &[i64],
    scalar: Option<i64>,
    at: Option<Point>,
) -> Result<RallyPlan, String> {
    let ids = &ids[..ids.len().min(50.max(c.units.len()))];
    let player = c
        .wallets
        .players
        .get(owner)
        .ok_or("missing rally commander")?;
    let width = c.terrain.width as f64 * CELL;
    let height = c.terrain.height as f64 * CELL;
    let building_branch = c.mode.kind.is_construction()
        || (c.mode.kind.is_skirmish() && (scalar.is_some() || !ids.is_empty()));
    // Tutorial and other non-base modes ignore a rally with selected IDs.
    if !building_branch && (scalar.is_some() || !ids.is_empty()) {
        return Ok(RallyPlan::Unchanged);
    }
    let Some(at) = at.filter(|p| p.x.is_finite() && p.z.is_finite()) else {
        return Ok(RallyPlan::Denied(Denial::Blocked));
    };
    if building_branch {
        let single = scalar.map(|id| [id]);
        let selected = single.as_ref().map_or(ids, |id| id.as_slice());
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let mut targets = Vec::new();
        let mut owns_selected = false;
        for id in selected {
            let Some(unit) = u32::try_from(*id)
                .ok()
                .and_then(|id| c.units.iter().find(|unit| unit.id == id))
            else {
                continue;
            };
            owns_selected |= unit.owner == Some(owner);
            let access = unit.owner == Some(owner)
                || (c.mode.kind == crate::economy::Mode::Horde
                    && c.mode.income_excluded_slot != Some(owner)
                    && unit.owner != c.mode.income_excluded_slot
                    && unit
                        .owner
                        .and_then(|slot| c.wallets.players.get(slot))
                        .is_some_and(|other| other.team == player.team));
            if !access || unit.hp <= 0.0 {
                continue;
            }
            let definition = catalog.unit(unit.kind).map_err(|e| e.to_string())?;
            if definition.get("makes").is_some() {
                targets.push(unit.id);
            }
        }
        if targets.is_empty() {
            return Ok(RallyPlan::Denied(if owns_selected {
                Denial::Needs
            } else {
                Denial::Unseen
            }));
        }
        Ok(RallyPlan::Buildings(
            targets,
            Point {
                x: math::min(width, math::max(0.0, at.x)),
                z: math::min(height, math::max(0.0, at.z)),
            },
        ))
    } else {
        if at.x < 0.0 || at.z < 0.0 || at.x >= width || at.z >= height {
            return Ok(RallyPlan::Denied(Denial::Blocked));
        }
        let cell =
            (at.z / CELL).floor() as usize * c.terrain.width + (at.x / CELL).floor() as usize;
        if c.terrain
            .flags
            .get(cell)
            .is_none_or(|flags| flags & MOVE != 0)
        {
            return Ok(RallyPlan::Denied(Denial::Blocked));
        }
        Ok(RallyPlan::Personal(at))
    }
}

impl Match {
    /// The dispatcher owns finish, away and input-rate admission.
    pub fn set_rally(
        &mut self,
        owner: usize,
        ids: &[i64],
        scalar: Option<i64>,
        at: Option<Point>,
    ) -> Result<Option<Denial>, String> {
        self.validate_actor_mode()?;
        if self.actor_policy(owner)?.economy_slot.is_none() {
            return Ok(Some(Denial::Blocked));
        }
        let units = self
            .world
            .game
            .units
            .values()
            .map(|unit| {
                let kind = self
                    .rules
                    .kinds
                    .get(unit.kind)
                    .ok_or("unknown rally target kind")?;
                Ok(UnitView {
                    id: unit.id,
                    owner: (unit.owner < self.start.economy.players.len()).then_some(unit.owner),
                    kind: kind.name.as_str(),
                    hp: unit.hp,
                    built: unit.built.unwrap_or(0.0),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let context = RallyContext {
            mode: ModeContext {
                kind: self.options.mode,
                income_excluded_slot: self.horde.as_ref().map(|horde| horde.slot),
                ..Default::default()
            },
            terrain: crate::match_runtime::terrain_view(&self.world.game),
            units: &units,
            wallets: &self.start.economy,
        };
        match plan_rally(&context, owner, ids, scalar, at)? {
            RallyPlan::Denied(denial) => return Ok(Some(denial)),
            RallyPlan::Unchanged => {}
            RallyPlan::Personal(at) => {
                *self
                    .player_rallies
                    .get_mut(owner)
                    .ok_or("canonical commander rally missing")? = Some(at);
            }
            RallyPlan::Buildings(targets, at) => {
                if targets
                    .iter()
                    .any(|id| !self.building_rallies.contains_key(id))
                {
                    return Err("canonical producer rally missing".into());
                }
                for id in targets {
                    self.building_rallies.insert(id, Some(at));
                }
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
#[path = "match_base24_tests.rs"]
mod base24_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{economy::Mode, economy_state::PlayerEconomy, navigation::Versions};

    #[test]
    fn construction_and_horde_rallies_follow_source_access_and_personal_policy() {
        let flags = [0; 16];
        let units = [
            UnitView {
                id: 1,
                owner: Some(0),
                kind: "hq",
                hp: 10.,
                built: 0.5,
            },
            UnitView {
                id: 2,
                owner: Some(1),
                kind: "barracks",
                hp: 10.,
                built: 1.,
            },
            UnitView {
                id: 3,
                owner: Some(2),
                kind: "hq",
                hp: 10.,
                built: 1.,
            },
            UnitView {
                id: 4,
                owner: Some(0),
                kind: "rifle",
                hp: 10.,
                built: 0.,
            },
        ];
        let wallets = EconomyState {
            players: (0..3)
                .map(|_| PlayerEconomy {
                    team: 0,
                    ..Default::default()
                })
                .collect(),
        };
        let mut c = RallyContext {
            mode: ModeContext::default(),
            terrain: TerrainView {
                width: 4,
                height: 4,
                flags: &flags,
                heights: None,
                world_known: false,
                knowledge_id: 0,
                versions: Versions::default(),
                changed: None,
                changes: &[],
            },
            units: &units,
            wallets: &wallets,
        };
        let at = Point { x: 3., z: 3. };
        for mode in [Mode::Classic, Mode::World] {
            c.mode.kind = mode;
            assert_eq!(
                plan_rally(&c, 0, &[], None, Some(at)).unwrap(),
                RallyPlan::Denied(Denial::Unseen)
            );
            assert_eq!(
                plan_rally(&c, 0, &[1], None, Some(at)).unwrap(),
                RallyPlan::Buildings(vec![1], at)
            );
            assert_eq!(
                plan_rally(&c, 0, &[1], Some(4), Some(at)).unwrap(),
                RallyPlan::Denied(Denial::Needs)
            );
        }
        for mode in [
            Mode::Default,
            Mode::Conquest,
            Mode::Assault,
            Mode::Annihilation,
        ] {
            c.mode.kind = mode;
            assert_eq!(
                plan_rally(&c, 0, &[2], None, Some(at)).unwrap(),
                RallyPlan::Denied(Denial::Unseen)
            );
            assert_eq!(
                plan_rally(&c, 0, &[], None, Some(at)).unwrap(),
                RallyPlan::Personal(at)
            );
        }
        let mut capped = vec![-1; 51];
        capped[50] = 1;
        assert_eq!(
            plan_rally(&c, 0, &capped, None, Some(at)).unwrap(),
            RallyPlan::Denied(Denial::Unseen)
        );
        assert_eq!(
            plan_rally(&c, 0, &capped, Some(1), Some(at)).unwrap(),
            RallyPlan::Buildings(vec![1], at)
        );
        c.mode.kind = Mode::Horde;
        c.mode.income_excluded_slot = Some(2);
        assert_eq!(
            plan_rally(&c, 0, &[2], None, Some(at)).unwrap(),
            RallyPlan::Buildings(vec![2], at)
        );
        assert_eq!(
            plan_rally(&c, 0, &[3], None, Some(at)).unwrap(),
            RallyPlan::Denied(Denial::Unseen)
        );
        assert_eq!(
            plan_rally(&c, 2, &[1], None, Some(at)).unwrap(),
            RallyPlan::Denied(Denial::Unseen)
        );
        assert_eq!(
            plan_rally(&c, 2, &[3], None, Some(at)).unwrap(),
            RallyPlan::Buildings(vec![3], at)
        );
        c.mode.kind = Mode::Tutorial;
        assert_eq!(
            plan_rally(&c, 0, &[], None, Some(at)).unwrap(),
            RallyPlan::Personal(at)
        );
        assert_eq!(
            plan_rally(&c, 0, &[1], None, None).unwrap(),
            RallyPlan::Unchanged
        );
        assert_eq!(
            plan_rally(&c, 0, &[], Some(1), None).unwrap(),
            RallyPlan::Unchanged
        );
    }
}
