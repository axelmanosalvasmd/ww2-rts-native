//! Source timeline sampling over borrowed authoritative facts.
//! Match uses this component for periodic and finish samples. It does not admit modes or advance a clock.
use crate::{
    economy::Mode, game::LOCAL_DEFENDER_OWNER, match_end::MatchSample, math, rules::Rules,
};

#[derive(Clone, Copy, Debug)]
pub struct TimelinePlayer {
    pub team: i32,
    pub vp: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct TimelineUnit {
    pub owner: usize,
    pub kind: usize,
    pub hp: f64,
}

/// Team IDs index every array. Catalog structure flags and the canonical kind name
/// decide which living actors count; no prepared counts or expected sample is accepted.
pub fn sample(
    mode: Mode,
    tick: i64,
    players: &[TimelinePlayer],
    point_owners: &[Option<usize>],
    units: impl IntoIterator<Item = TimelineUnit>,
    rules: &Rules,
) -> Result<MatchSample, String> {
    if mode == Mode::Other
        || !(1..=6).contains(&players.len())
        || players.iter().any(|p| !(0..6).contains(&p.team))
        || !(0..=9_007_199_254_740_991).contains(&tick)
    {
        return Err("invalid timeline players or tick".into());
    }
    let n = players
        .iter()
        .map(|p| p.team)
        .max()
        .expect("checked players") as usize
        + 1;
    let t = math::round(tick as f64 / 20.0);
    if matches!(mode, Mode::Default | Mode::Conquest) {
        let mut vp = vec![0.0; n];
        for player in players {
            if !player.vp.is_finite() {
                return Err("nonfinite Conquest sample score".into());
            }
            vp[player.team as usize] += player.vp;
            if !vp[player.team as usize].is_finite() {
                return Err("nonfinite Conquest sample score".into());
            }
        }
        for value in &mut vp {
            *value = value.floor();
        }
        return Ok(MatchSample::Conquest { t, vp });
    }
    let mut points = vec![0.0; n];
    let mut structures = vec![0.0; n];
    for owner in point_owners.iter().flatten() {
        let player = players.get(*owner).ok_or("invalid timeline point owner")?;
        points[player.team as usize] += 1.0;
    }
    for unit in units {
        let kind = rules
            .kinds
            .get(unit.kind)
            .ok_or("invalid timeline unit kind")?;
        if !unit.hp.is_finite()
            || (unit.owner >= players.len()
                && !(mode == Mode::World && unit.owner == LOCAL_DEFENDER_OWNER))
        {
            return Err("invalid timeline unit".into());
        }
        if unit.hp > 0.0
            && kind.structure
            && (mode.is_construction() || kind.name == "bunker")
            && let Some(player) = players.get(unit.owner)
        {
            structures[player.team as usize] += 1.0;
        }
    }
    Ok(MatchSample::Territories {
        t,
        points,
        structures,
    })
}
