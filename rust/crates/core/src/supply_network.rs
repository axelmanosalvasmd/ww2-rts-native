//! Ordinary ground supply distances and refill rates from explicit authoritative facts.
use crate::{
    catalog::{Catalog, Value},
    game::{CELL, MOVE, Point, VBLOCK},
    math,
    navigation::TerrainView,
    objectives::{self, Objective, ObjectivePlayer, ObjectiveRules, ObjectiveUnit},
    unit_spawn::SpawnPlacer,
};

#[derive(Clone, Copy, Debug)]
pub struct SupplyRules {
    pub damage_cells: usize,
    pub near: f64,
    pub far: f64,
    pub floor: f64,
    pub network_ticks: u64,
}
impl SupplyRules {
    pub fn compiled() -> Result<Self, String> {
        let c = Catalog::compiled().map_err(|e| e.to_string())?;
        let s = c
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("supply")
            .ok_or("missing supply rules")?;
        fn n(s: &Value, k: &str) -> Result<f64, String> {
            s.get(k)
                .and_then(Value::number)
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("invalid supply {k}"))
        }
        let damage = n(s, "damageCells")?;
        let beat = n(s, "network")?;
        let r = Self {
            damage_cells: damage as usize,
            near: n(s, "near")?,
            far: n(s, "far")?,
            floor: n(s, "floor")?,
            network_ticks: beat as u64,
        };
        if damage < 0.0
            || damage.fract() != 0.0
            || damage > 65532.0
            || beat < 0.0
            || beat.fract() != 0.0
            || beat > u32::MAX as f64
            || r.near < 0.0
            || r.far <= r.near
            || !(0.0..=1.0).contains(&r.floor)
        {
            return Err("invalid ground supply rules".into());
        }
        Ok(r)
    }
}
#[derive(Clone, Copy)]
pub struct SupplyUnit<'a> {
    pub fact: ObjectiveUnit<'a>,
    pub built: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TeamNetwork {
    pub team: i32,
    pub steps: Vec<u16>,
    pub closed: Vec<u8>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GroundSupplyNetwork {
    pub width: usize,
    pub height: usize,
    pub teams: Vec<TeamNetwork>,
}

pub struct SupplyInput<'a> {
    pub view: TerrainView<'a>,
    pub chars: &'a [u8],
    pub initial_chars: &'a [u8],
    pub initial_heights: Option<&'a [i8]>,
    pub points: &'a [Objective],
    pub units: &'a [SupplyUnit<'a>],
    pub players: &'a [ObjectivePlayer],
}
impl GroundSupplyNetwork {
    /// Build a new network, leaving the caller's previous network intact on invalid inputs.
    pub fn compute(input: SupplyInput<'_>, placer: &mut SpawnPlacer) -> Result<Self, String> {
        let SupplyInput {
            view,
            chars,
            initial_chars,
            initial_heights,
            points,
            units,
            players,
        } = input;
        let n = view
            .width
            .checked_mul(view.height)
            .ok_or("invalid supply shape")?;
        if n == 0
            || n > i32::MAX as usize
            || view.flags.len() != n
            || chars.len() != n
            || initial_chars.len() != n
            || view.heights.is_some_and(|h| h.len() != n)
            || initial_heights.is_some_and(|h| h.len() != n)
        {
            return Err("invalid supply shape".into());
        }
        if points.iter().any(|p| {
            p.owner.is_some_and(|o| o >= players.len())
                || !p.at.x.is_finite()
                || !p.at.z.is_finite()
        }) || players
            .iter()
            .any(|p| !p.spawn.x.is_finite() || !p.spawn.z.is_finite())
            || units
                .iter()
                .any(|u| u.built.is_some_and(|v| !v.is_finite()))
        {
            return Err("invalid supply facts".into());
        }
        if units.iter().any(|u| {
            u.fact.owner >= players.len()
                || !u.fact.at.x.is_finite()
                || !u.fact.at.z.is_finite()
                || !u.fact.hp.is_finite()
        }) {
            return Err("invalid supply unit".into());
        }
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        for u in units {
            catalog.unit(u.fact.kind).map_err(|e| e.to_string())?;
        }
        let rules = SupplyRules::compiled()?;
        let objective_rules =
            ObjectiveRules::compiled().map_err(|e| format!("supply zoc: {e:?}"))?;
        let facts: Vec<_> = units.iter().map(|u| u.fact).collect();
        let mut teams = Vec::new();
        for p in players {
            if !p.out && !teams.contains(&p.team) {
                teams.push(p.team);
            }
        }
        let mut result = Self {
            width: view.width,
            height: view.height,
            teams: Vec::new(),
        };
        for team in teams {
            let held: Vec<_> = points
                .iter()
                .filter(|p| p.owner.is_some_and(|o| players[o].team == team))
                .map(|p| p.at)
                .collect();
            let closed =
                objectives::zoc_closed(view, &facts, players, team, &held, objective_rules)
                    .map_err(|e| format!("supply zoc: {e:?}"))?;
            let mut sources: Vec<_> = units
                .iter()
                .filter(|u| {
                    u.fact.kind == "hq"
                        && u.fact.hp > 0.0
                        && u.built.unwrap_or(1.0) >= 1.0
                        && players[u.fact.owner].team == team
                        && !players[u.fact.owner].out
                })
                .map(|u| u.fact.at)
                .collect();
            if sources.is_empty() {
                sources.extend(
                    players
                        .iter()
                        .filter(|p| p.team == team && !p.out)
                        .map(|p| p.spawn),
                );
            }
            sources.extend(
                points
                    .iter()
                    .filter(|p| {
                        p.owner.is_some_and(|o| players[o].team == team)
                            && !p.cut
                            && !p.contested
                            && p.kind.as_deref() == Some("depot")
                    })
                    .map(|p| p.at),
            );
            let k = rules.damage_cells + 2;
            let mut buckets = vec![Vec::new(); k];
            let mut pending = 0usize;
            let mut steps = vec![65535u16; n];
            for at in sources {
                let c = placer
                    .nearest_free(view, at, MOVE, None)
                    .map_err(|e| format!("supply seed: {e:?}"))?;
                if steps[c] != 0 {
                    steps[c] = 0;
                    buckets[0].push(c);
                    pending += 1;
                }
            }
            let mut d = 0usize;
            while pending > 0 {
                while let Some(c) = buckets[d % k].pop() {
                    pending -= 1;
                    if usize::from(steps[c]) != d {
                        continue;
                    }
                    let x = c % view.width;
                    let neighbors = [
                        (x > 0).then(|| c - 1),
                        (x + 1 < view.width).then_some(c + 1),
                        c.checked_sub(view.width),
                        (c + view.width < n).then_some(c + view.width),
                    ];
                    for nb in neighbors.into_iter().flatten() {
                        if view.flags[nb] & (MOVE | VBLOCK) != 0
                            || closed[nb] != 0
                            || (level(view.heights, nb) - level(view.heights, c)).abs() > 1
                        {
                            continue;
                        }
                        let fillable = chars[nb] == b'+'
                            || chars[nb] == b'R'
                            || (chars[nb] == b'F' && initial_chars[nb] != b'F')
                            || (chars[nb] == b'.'
                                && level(view.heights, nb) < level(initial_heights, nb));
                        let next =
                            (d + 1 + if fillable { rules.damage_cells } else { 0 }).min(65534);
                        if next < usize::from(steps[nb]) {
                            steps[nb] = next as u16;
                            buckets[next % k].push(nb);
                            pending += 1;
                        }
                    }
                }
                d += 1;
            }
            result.teams.push(TeamNetwork {
                team,
                steps,
                closed,
            });
        }
        Ok(result)
    }
    /// Source lookup uses the best distance in a square of three cells, but rejects a closed standing cell.
    pub fn rate_at(&self, team: Option<i32>, at: Point) -> Result<f64, String> {
        if !at.x.is_finite() || !at.z.is_finite() {
            return Err("invalid supply position".into());
        }
        let x = (at.x / CELL).floor();
        let y = (at.z / CELL).floor();
        if x < 0.0 || y < 0.0 || x >= self.width as f64 || y >= self.height as f64 {
            return Ok(0.0);
        }
        let Some(net) = team.and_then(|t| self.teams.iter().find(|n| n.team == t)) else {
            return Ok(0.0);
        };
        let n = self
            .width
            .checked_mul(self.height)
            .ok_or("invalid supply network shape")?;
        if net.steps.len() != n || net.closed.len() != n {
            return Err("invalid supply network shape".into());
        }
        let (x, y) = (x as usize, y as usize);
        if net.closed[y * self.width + x] != 0 {
            return Ok(0.0);
        }
        let mut steps = 65535;
        for row in y.saturating_sub(3)..=(y + 3).min(self.height - 1) {
            for col in x.saturating_sub(3)..=(x + 3).min(self.width - 1) {
                steps = steps.min(net.steps[row * self.width + col]);
            }
        }
        if steps == 65535 {
            return Ok(0.0);
        }
        let r = SupplyRules::compiled()?;
        let metres = f64::from(steps) * CELL;
        Ok(if metres <= r.near {
            1.0
        } else {
            math::max(
                r.floor,
                1.0 - (1.0 - r.floor) * (metres - r.near) / (r.far - r.near),
            )
        })
    }
}
fn level(h: Option<&[i8]>, c: usize) -> i16 {
    h.map_or(0, |h| i16::from(h[c]))
}
