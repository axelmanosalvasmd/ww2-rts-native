//! Finish a detached commander look in the same order as the ordinary command stream.
use crate::{
    game::Point,
    match_commands::{Command, MoveOrder},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SituationKind {
    Defense,
    Wait,
    Push,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Situation {
    pub kind: SituationKind,
    pub until: f64,
    pub point: Option<usize>,
    pub threat_id: Option<u32>,
    pub at: Option<Point>,
    pub counter: Option<String>,
    pub safe: Option<usize>,
    pub failed_at: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Hold,
    TakeOther,
    Prepare,
    Take,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    pub situation: Option<SituationKind>,
    pub action: Option<Action>,
    pub buy: Option<String>,
    pub aim: Option<usize>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LookState {
    pub operation: Option<Situation>,
    pub decision: Option<Decision>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LookOrders {
    /// Support, abilities and focus commands emitted earlier in this look.
    pub immediate: Vec<Command>,
    pub retreat: Vec<u32>,
    pub moves: Vec<MoveOrder>,
    pub attack_moves: Vec<MoveOrder>,
    pub operation: Vec<Command>,
    pub claims: BTreeSet<u32>,
    /// Released operation members keep their manual orders until this simulation time.
    pub released_until: BTreeMap<u32, f64>,
}

impl LookState {
    /// Store the decision and emit retreat, move, attack-move, then operation commands.
    /// Empty groups emit nothing. Claimed and recently released members keep operation ownership.
    pub fn finish(
        &mut self,
        now: f64,
        mut situation: Option<Situation>,
        buy: Option<String>,
        aim: Option<usize>,
        failures: &[u8],
        mut orders: LookOrders,
    ) -> Result<Vec<Command>, String> {
        if !now.is_finite()
            || now < 0.0
            || situation.as_ref().is_some_and(|s| {
                !s.until.is_finite()
                    || s.until < 0.0
                    || s.at.is_some_and(|p| !p.x.is_finite() || !p.z.is_finite())
            })
            || orders
                .released_until
                .values()
                .any(|until| !until.is_finite())
            || orders
                .moves
                .iter()
                .chain(&orders.attack_moves)
                .any(|o| !o.at.x.is_finite() || !o.at.z.is_finite())
        {
            return Err("invalid detached look completion".into());
        }
        let controlled = |id: &u32| {
            orders.claims.contains(id)
                || orders
                    .released_until
                    .get(id)
                    .is_some_and(|until| now < *until)
        };
        orders.retreat.retain(|id| !controlled(id));
        orders
            .moves
            .retain(|o| u32::try_from(o.unit).is_ok_and(|id| !controlled(&id)));
        orders
            .attack_moves
            .retain(|o| u32::try_from(o.unit).is_ok_and(|id| !controlled(&id)));
        let (kind, action, recorded_aim) = if let Some(s) = &mut situation {
            if let Some(i) = s.point {
                s.failed_at = failures.get(i).copied().unwrap_or(s.failed_at);
            }
            let action = match s.kind {
                SituationKind::Defense => Action::Hold,
                SituationKind::Wait if s.safe.is_some() => Action::TakeOther,
                SituationKind::Wait => Action::Prepare,
                SituationKind::Push => Action::Take,
            };
            (Some(s.kind), Some(action), aim.or(s.point).or(s.safe))
        } else {
            (None, None, aim)
        };
        let mut result = orders.immediate;
        if !orders.retreat.is_empty() {
            result.push(Command::Retreat {
                ids: orders.retreat.into_iter().map(i64::from).collect(),
            });
        }
        for (group, attack_move) in [(orders.moves, false), (orders.attack_moves, true)] {
            if !group.is_empty() {
                result.push(Command::Move {
                    orders: group,
                    control_ids: None,
                    attack_move,
                    queue: false,
                    together: false,
                    face: None,
                });
            }
        }
        result.extend(orders.operation);
        if situation.is_some() {
            self.operation = situation;
        }
        self.decision = Some(Decision {
            situation: kind,
            action,
            buy,
            aim: recorded_aim,
        });
        Ok(result)
    }
}

#[cfg(test)]
#[path = "commander_look_tests.rs"]
mod tests;
