use super::*;
use crate::{
    economy_state::ArmySize,
    game::{CELL, Point},
    grid::SpatialGrid,
};
use std::collections::BTreeSet;

pub(super) fn capture(state: &Match) -> Result<Vec<Map<String, Value>>, SnapshotError> {
    capture_perspective(state, false)
}

pub(super) fn capture_perspective(
    state: &Match,
    full_map: bool,
) -> Result<Vec<Map<String, Value>>, SnapshotError> {
    state
        .preflight_legacy_scenario()
        .map_err(|_| SnapshotError::Unsupported {
            unit: 0,
            capability: SnapshotCapability::Scenario,
        })?;
    let g = &state.world.game;
    for slot in 0..g.players.len() {
        state
            .recipient_routing(slot)
            .map_err(|_| SnapshotError::Rules)?;
    }
    for point in &state.start.points {
        finite(&[point.progress])?;
        if point.owner.is_some_and(|slot| slot >= g.players.len())
            || point.capper.is_some_and(|slot| slot >= g.players.len())
            || point
                .needs
                .is_some_and(|index| index >= state.start.points.len())
        {
            return Err(SnapshotError::Rules);
        }
    }
    let catalog = Catalog::compiled().map_err(|_| SnapshotError::Rules)?;
    let cfg = catalog.table("CFG").map_err(|_| SnapshotError::Rules)?;
    let key = match state.options.army {
        ArmySize::Standard => "standard",
        ArmySize::Large => "large",
        ArmySize::Massive => "massive",
        ArmySize::Endless => "endless",
    };
    let army = cfg
        .get("armies")
        .and_then(|a| a.get(key))
        .ok_or(SnapshotError::Rules)?;
    let value = |name: &str| {
        army.get(name)
            .and_then(crate::catalog::Value::number)
            .ok_or(SnapshotError::Rules)
    };
    let (pop, income) = (value("pop")?, value("income")?);
    finite(&[pop, income])?;
    let wx = [
        g.rain,
        g.wet.unwrap_or(0.),
        state.start.wind.0,
        state.start.wind.1,
    ];
    finite(&wx)?;
    let names = catalog
        .table("SUPPORT_TYPES")
        .map_err(|_| SnapshotError::Rules)?
        .array()
        .ok_or(SnapshotError::Rules)?;
    let grid = SpatialGrid::new(&g.units, 16.);
    let mut result = Vec::with_capacity(g.players.len());
    for slot in 0..g.players.len() {
        let mut fields = Map::new();
        let mode = super::mode_row(state)?;
        if !mode.is_null() {
            fields.insert("mode".into(), mode);
        }
        if state.options.mode == Mode::Classic {
            let nodes = state
                .classic_snapshot_nodes()
                .map_err(|_| SnapshotError::Rules)?
                .ok_or(SnapshotError::Rules)?;
            fields.insert("nodes".into(), json!(nodes));
        }
        // Reuse Source's six-field ghost rows. Never consult a hidden live building here.
        if state.options.mode == Mode::Classic {
            let rows = state
                .classic_known_buildings
                .iter()
                .find(|(team, _)| *team == g.players[slot].team)
                .map_or(&[][..], |(_, rows)| rows.as_slice());
            let ghosts = rows
                .iter()
                .filter(|row| !seen_by(state, slot, row.id))
                .map(|row| {
                    let kind = state
                        .rules
                        .kinds
                        .get(row.kind)
                        .ok_or(SnapshotError::Rules)?;
                    if row.id == 0 || row.owner >= g.players.len() || !kind.building {
                        return Err(SnapshotError::Rules);
                    }
                    finite(&[row.at.x, row.at.z])?;
                    if let Some(built) = row.built {
                        finite(&[built])?;
                    }
                    Ok(json!([
                        row.id,
                        kind.name,
                        row.owner,
                        r(row.at.x),
                        r(row.at.z),
                        row.built.map(r)
                    ]))
                })
                .collect::<Result<Vec<_>, SnapshotError>>()?;
            fields.insert("ghosts".into(), json!(ghosts));
        }
        if let Some(mission) = &state.authored_mission {
            let teams = g
                .players
                .iter()
                .map(|player| player.team)
                .collect::<Vec<_>>();
            fields.insert(
                "scenario".into(),
                mission.projection(slot, &teams, full_map),
            );
        } else if state.scenario.is_some() {
            // The admitted migrated engine has no authored objectives.
            fields.insert("scenario".into(), json!({"version":1,"objectives":[]}));
        }
        fields.insert("army".into(), json!({"pop":pop,"income":income}));
        fields.insert("wx".into(), json!(wx.map(r)));
        let player = state
            .start
            .players
            .get(slot)
            .ok_or(SnapshotError::InvalidRecipient)?;
        let mut support = Map::new();
        for name in names {
            let name = name.string().ok_or(SnapshotError::Rules)?;
            let cooldown = player.support.get(name).ok_or(SnapshotError::Rules)?;
            finite(&[*cooldown])?;
            support.insert(name.into(), json!(math::max(0., cooldown.ceil())));
        }
        fields.insert("sup".into(), Value::Object(support));
        let points = state
            .start
            .points
            .iter()
            .map(|point| {
                let teams = if point.contested {
                    point
                        .on_point
                        .iter()
                        .filter(|id| full_map || seen_by(state, slot, **id))
                        .filter_map(|id| g.units.get(id))
                        .map(|u| g.players[u.owner].team)
                        .collect::<BTreeSet<_>>()
                } else {
                    BTreeSet::new()
                };
                let allied =
                    |owner: Option<usize>| owner.is_some_and(|owner| g.allied(owner, slot));
                let locked = point.needs.is_some_and(|index| {
                    !allied(point.owner) && !allied(state.start.points[index].owner)
                });
                json!([
                    point.owner.map_or(-1, |n| n as i64),
                    point.capper.map_or(-1, |n| n as i64),
                    r(point.progress),
                    u8::from(teams.len() > 1),
                    u8::from(point.cut),
                    u8::from(locked)
                ])
            })
            .collect::<Vec<_>>();
        fields.insert("points".into(), json!(points));
        let visible = |at: Point| {
            full_map
                || g.reveal
                || vision::team_sees(g, &state.rules, &grid, g.players[slot].team, at)
        };
        fields.insert(
            "smokes".into(),
            json!(
                g.smokes
                    .iter()
                    .filter(|s| visible(Point { x: s.x, z: s.z }))
                    .map(|s| json!([r(s.x), r(s.z), s.r, s.id]))
                    .collect::<Vec<_>>()
            ),
        );
        fields.insert(
            "fires".into(),
            json!(
                state
                    .world
                    .fires
                    .iter()
                    .map(|(cell, _)| *cell)
                    .filter(|cell| visible(Point {
                        x: (cell % g.w) as f64 * CELL + CELL / 2.,
                        z: (cell / g.w) as f64 * CELL + CELL / 2.
                    }))
                    .collect::<Vec<_>>()
            ),
        );
        result.push(fields);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_public_state_rejects_without_substituting_defaults() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/recipient-snapshot.json"))
                .unwrap();
        let mut state = super::super::tests::initialize(&fixture["cases"][0]);
        let count = state.start.points.len();
        state.start.points[0].needs = Some(count);
        assert!(matches!(
            SnapshotCache::new(&state),
            Err(SnapshotError::Rules)
        ));
        state.start.points[0].needs = None;
        state.world.game.rain = f64::NAN;
        assert!(SnapshotCache::new(&state).is_err());
        state.world.game.rain = 0.;
        let name = state.start.players[0]
            .support
            .keys()
            .next()
            .unwrap()
            .clone();
        state.start.players[0].support.remove(&name);
        assert!(matches!(
            SnapshotCache::new(&state),
            Err(SnapshotError::Rules)
        ));
    }
}
