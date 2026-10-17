//! Ordinary current-tick full-map projection with independent terrain delivery.
use super::*;
use crate::{match_tick::TickEvent, observed_memory::ObservedRow};

/// One shared ordinary spectator terrain stream. Create a fresh stream per match.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpectatorDelivery {
    dimensions: Option<(usize, usize)>,
    terrain_key: Option<(u32, i64)>,
    terrain: Vec<ObservedRow>,
    positions: std::collections::BTreeMap<usize, usize>,
}

impl SpectatorDelivery {
    pub fn new(state: &Match) -> Result<Self, SnapshotError> {
        SnapshotCache::new(state)?;
        Ok(Self {
            dimensions: Some((state.world.game.w, state.world.game.h)),
            terrain_key: None,
            terrain: Vec::new(),
            positions: std::collections::BTreeMap::new(),
        })
    }

    /// A joining watcher's start uses fresh memory, independently of the stream.
    pub fn full_terrain(state: &Match) -> Result<Value, SnapshotError> {
        SnapshotCache::new(state)?;
        Self::default().terrain_rows(state, true)
    }

    /// Build the complete current ordinary watcher snapshot without publishing
    /// any seat's terrain, fog, wreck memory, or producer acknowledgment.
    pub fn capture(
        &mut self,
        state: &Match,
        events: &[TickEvent],
        full: bool,
    ) -> Result<Value, SnapshotError> {
        let cache = SnapshotCache::new(state)?;
        composition::admit_actors(state, &[])?;
        let mut fields = cache.recipient(0)?.fields;
        let units = cache
            .rows
            .iter()
            .map(|row| {
                let mut row = row.as_array().unwrap().clone();
                // The prototype's EVERYTHING visibility retains stale target IDs.
                row[11] = json!(0);
                let flags = row[12].as_u64().unwrap();
                row[12] = json!(if row[2].as_u64() == Some(0) {
                    flags & !(2048 | 4096 | 8192 | 16384)
                } else {
                    flags & (2047 | 32768 | (7 << 20) | 262144)
                });
                Value::Array(row)
            })
            .collect::<Vec<_>>();
        fields.insert("units".into(), json!(units));
        let public = public_state::capture_perspective(state, true)?;
        fields.extend(
            public
                .into_iter()
                .next()
                .ok_or(SnapshotError::InvalidRecipient)?,
        );
        fields.insert(
            "falling".into(),
            falling::project_perspective(state, 0, true)?,
        );
        let raw = state
            .ordinary_snapshot_raw_lists()
            .map_err(|_| SnapshotError::Rules)?;
        fields.insert("wrecks".into(), raw.wrecks);
        let g = &state.world.game;
        let team = g.players[0].team;
        if state
            .world
            .mine_seen
            .iter()
            .any(|(cell, _)| *cell >= g.chars.len())
        {
            return Err(SnapshotError::Rules);
        }
        fields.insert(
            "foundMines".into(),
            json!(
                state
                    .world
                    .mine_seen
                    .iter()
                    .filter(|(cell, bits)| **bits >> team & 1 != 0 && g.chars[**cell] == b'N')
                    .map(|(cell, _)| *cell)
                    .collect::<Vec<_>>()
            ),
        );
        let shots = events
            .iter()
            .map(ShotRef::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let selected = [SelectedSeat {
            slot: 0,
            full,
            omit: true,
        }];
        let projected = shots::project_shots_perspective(state, &selected, &shots, true)?;
        fields.insert(
            "shots".into(),
            projected.into_iter().next().ok_or(SnapshotError::Rules)?.1,
        );
        fields.insert("flights".into(), json!([]));
        fields.insert("t".into(), json!("s"));
        // Keep the prototype's literal removal list, including its retained tech.
        for key in [
            "movement",
            "queues",
            "productionJobs",
            "plans",
            "orders",
            "air",
            "mp",
            "inc",
            "mun",
            "fuel",
            "fuelInc",
            "upkeep",
            "sup",
            "rally",
            "home",
            "works",
            "covers",
            "logistics",
        ] {
            fields.remove(key);
        }
        // Commit this stream only after every projected component has admitted.
        let mut next = self.clone();
        fields.insert("cells".into(), next.terrain_rows(state, full)?);
        *self = next;
        Ok(Value::Object(fields))
    }

    fn terrain_rows(&mut self, state: &Match, full: bool) -> Result<Value, SnapshotError> {
        let world = &state.world;
        let g = &world.game;
        crate::observed_memory::validate_world(world).map_err(|_| SnapshotError::Rules)?;
        let dimensions = (g.w, g.h);
        if self.dimensions.is_some_and(|old| old != dimensions) {
            return Err(SnapshotError::InvalidRecipient);
        }
        let key = (g.terrain_version, g.vision_tick.unwrap_or(-1));
        // The prototype rechecks pending terrain only when terrain or vision changes.
        if self.terrain_key == Some(key) {
            return Ok(if full {
                Value::Array(self.terrain.iter().map(cell_json).collect())
            } else {
                json!([])
            });
        }
        let team = g
            .players
            .first()
            .ok_or(SnapshotError::InvalidRecipient)?
            .team;
        let grid = crate::grid::SpatialGrid::new(&g.units, 16.);
        let mut changed = Vec::new();
        for stored in world.cell_log() {
            let cell = stored.cell;
            let mut row = world.cell_row(cell).map_err(|_| SnapshotError::Rules)?;
            let owner = world.mines.get(&cell).copied().unwrap_or(-1);
            let owned = if owner >= 0 {
                g.players
                    .get(owner as usize)
                    .ok_or(SnapshotError::Rules)?
                    .team
                    == team
            } else {
                false
            };
            let known = owned || world.mine_seen.get(&cell).copied().unwrap_or(0) >> team & 1 != 0;
            let mine_owned = (row.ch == b'N' && known).then_some(owned);
            if row.ch == b'N' && !known {
                row.ch = g.physical(cell);
                row.data.mine = false;
                row.data.mine_material = None;
            }
            let observed = ObservedRow {
                row,
                mine_owned,
                revision: 0,
            };
            observed.route_row().map_err(|_| SnapshotError::Rules)?;
            let d = &observed.row.data;
            if !matches!(
                d.ground_material.as_str(),
                "soil" | "road" | "wood" | "stone" | "concrete" | "steel" | "water"
            ) || !d.ground_wear.is_finite()
                || d.trench_front.is_some_and(|v| !v.is_finite())
                || d.section.as_ref().is_some_and(|s| {
                    !s.hp.is_finite() || s.hp < 0. || !s.max_hp.is_finite() || s.max_hp < 0.
                })
            {
                return Err(SnapshotError::Rules);
            }
            let position = self.positions.get(&cell).copied();
            let old = position.map(|index| &self.terrain[index]);
            if old.is_some_and(|old| old == &observed) {
                continue;
            }
            if stored.ch == b'N'
                && !known
                && old.map_or_else(
                    || {
                        observed.row.ch == world.initial().chars[cell]
                            && observed.row.height
                                == world.initial().height.as_ref().map_or(0, |h| h[cell])
                    },
                    |old| {
                        old.row.ch == observed.row.ch
                            && old.row.height == observed.row.height
                            && old.row.state == observed.row.state
                    },
                )
            {
                continue;
            }
            if let Some(building) = state.building_cells.get(&cell) {
                let live = g.units.get(&building.unit);
                let owner = live.map_or(building.owner, |unit| unit.owner);
                let at = live.map_or(building.at, |unit| unit.at());
                // The prototype uses full unit visibility for live footprints,
                // but still asks seat 0's team sight for a vanished enemy ruin.
                if !g.allied(owner, 0)
                    && live.is_none()
                    && !vision::team_sees(g, &state.rules, &grid, team, at)
                {
                    continue;
                }
            }
            changed.push(cell_json(&observed));
            if let Some(position) = position {
                self.terrain[position] = observed;
            } else {
                self.positions.insert(cell, self.terrain.len());
                self.terrain.push(observed);
            }
        }
        self.dimensions = Some(dimensions);
        self.terrain_key = Some(key);
        Ok(if full {
            Value::Array(self.terrain.iter().map(cell_json).collect())
        } else {
            Value::Array(changed)
        })
    }
}

fn cell_json(observed: &ObservedRow) -> Value {
    let row = &observed.row;
    let d = &row.data;
    let mut data = json!({"ground":char::from(d.ground).to_string(),"object":char::from(d.object).to_string(),
        "material":d.material,"groundMaterial":d.ground_material,"groundWear":f64::from(d.ground_wear)});
    if let Some(front) = d.trench_front {
        data["trenchFront"] = json!(f64::from(front));
    }
    if d.mine {
        data["mine"] = json!(true);
        if let Some(material) = &d.mine_material {
            data["mineMaterial"] = json!(material);
        }
    }
    if let Some(owned) = observed.mine_owned {
        data["mineOwned"] = json!(owned);
    }
    if let Some(s) = &d.section {
        data["section"] = json!({"id":s.id,"structureId":s.structure_id,"state":s.state,"hp":s.hp,"maxHp":s.max_hp,"material":s.material,"anchor":s.anchor});
    }
    json!([
        row.cell,
        char::from(row.ch).to_string(),
        row.height,
        row.state,
        data
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Match {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/recipient-snapshot.json"))
                .unwrap();
        super::super::tests::initialize(&fixture["cases"][0])
    }

    #[test]
    fn ordinary_watcher_uses_cached_masks_and_keeps_literal_prototype_fields() {
        let mut state = fixture();
        for player in &mut state.world.game.players {
            player.visible.clear();
        }
        let cache = SnapshotCache::new(&state).unwrap();
        // Literal cached prototype rows, with fog lifted and private fields cleared.
        let expected = json!([
            [
                1, "rifle", 0, 6.1, 6.2, 0, 2.1, 100, 13, 3, 3, 0, 32783, 2, 0.8
            ],
            [2, "rifle", 1, 30, 30, 0, 0, 100, 0, 1, 0, 0, 0, 0, 1],
            [3, "rifle", 2, 35, 35, 0, 0, 100, 0, 1, 3, 0, 2, 0, 1]
        ]);
        let own_tech = cache.recipient(0).unwrap().fields["tech"].clone();
        let before = format!(
            "{:?}{:?}{:?}",
            state.world, state.recipients, state.routing_fog
        );
        let mut stream = SpectatorDelivery::new(&state).unwrap();
        let view = stream.capture(&state, &[], true).unwrap();
        super::super::tests::equal(&view["units"], &expected, "spectator.units");
        assert_eq!(view["tech"], own_tech);
        assert_eq!(
            view["units"].as_array().unwrap().len(),
            state.world.game.units.len()
        );
        for name in [
            "movement",
            "queues",
            "productionJobs",
            "plans",
            "orders",
            "air",
            "mp",
            "inc",
            "mun",
            "fuel",
            "fuelInc",
            "upkeep",
            "sup",
            "rally",
            "home",
            "works",
            "covers",
            "logistics",
            "fog",
        ] {
            assert!(view.get(name).is_none(), "watcher private field {name}");
        }
        assert_eq!(
            format!(
                "{:?}{:?}{:?}",
                state.world, state.recipients, state.routing_fog
            ),
            before
        );
    }

    #[test]
    fn watcher_terrain_start_and_deltas_do_not_acknowledge_seat_memory() {
        let mut state = fixture();
        let cell = 5 * state.world.game.w + 6;
        assert_ne!(state.world.game.chars[cell], b'T');
        assert_ne!(state.world.game.chars[cell + 1], b'T');
        state.world.set_cell(cell, b'T').unwrap();
        for player in &mut state.world.game.players {
            player.visible.clear();
        }
        let before = format!(
            "{:?}{:?}{:?}",
            state.world, state.recipients, state.routing_fog
        );
        let start = SpectatorDelivery::full_terrain(&state).unwrap();
        assert!(start.as_array().unwrap().iter().any(|row| row[0] == cell));
        let mut stream = SpectatorDelivery::new(&state).unwrap();
        let first = stream.capture(&state, &[], false).unwrap();
        assert_eq!(first["cells"], start);
        assert_eq!(
            stream.capture(&state, &[], false).unwrap()["cells"],
            json!([])
        );
        assert_eq!(SpectatorDelivery::full_terrain(&state).unwrap(), start);
        assert_eq!(
            stream.capture(&state, &[], false).unwrap()["cells"],
            json!([])
        );
        assert_eq!(
            format!(
                "{:?}{:?}{:?}",
                state.world, state.recipients, state.routing_fog
            ),
            before
        );
        state.world.set_cell(cell + 1, b'T').unwrap();
        let next = stream.capture(&state, &[], false).unwrap();
        assert_eq!(next["cells"].as_array().unwrap().len(), 1);
        assert_eq!(next["cells"][0][0], cell + 1);
    }

    #[test]
    fn watcher_keeps_dead_targets_and_shot_identities_with_full_flight_extrapolation() {
        use crate::{
            environment::{Effect, Hurt},
            game::Point,
            match_firing::LaunchEvent,
            projectiles::Point3,
        };
        let mut state = fixture();
        for player in &mut state.world.game.players {
            player.visible.clear();
        }
        let salvo = |shooter| {
            TickEvent::Firing(LaunchEvent::Salvo {
                shooter,
                owner: 2,
                at: Point { x: 130., z: 130. },
                count: 3,
                spread: 2.,
            })
        };
        let flight = TickEvent::Firing(LaunchEvent::Flight {
            id: 77,
            kind: state.world.game.units[&3].kind,
            effect: None,
            shooter: 3,
            owner: 2,
            at: Point3::new(130., 4., 130.),
            velocity: Point3::new(10., 0., 0.),
            gravity: 9.8,
            time: 1.,
            expires: 4.,
        });
        // Events outlive their actors, and a surviving unit can still name one.
        state.world.game.units.remove(&3).unwrap();
        state.firing.get_mut(1).unwrap().target_id = 3;
        let hurt = |local| {
            TickEvent::Environment(Effect::Hurt(Hurt {
                unit: 3,
                owner: 2,
                at: Point { x: 130., z: 130. },
                killed: true,
                from: Some(2),
                local,
            }))
        };
        let events = [salvo(3), salvo(999), flight, hurt(false), hurt(true)];
        let mut stream = SpectatorDelivery::new(&state).unwrap();
        let rows = stream.capture(&state, &events, false).unwrap();
        assert_eq!(rows["units"][0][9], 3);
        assert_eq!(
            rows["shots"][0],
            json!({"f":3,"fo":2,"x":130.,"z":130.,"k":"salvo","n":3,"r":2.,"physical":true,"local":true})
        );
        assert_eq!(
            rows["shots"][1],
            json!({"f":999,"fo":2,"x":130.,"z":130.,"k":"salvo","n":3,"r":2.,"physical":true,"local":true})
        );
        super::super::tests::equal(
            &rows["shots"][2],
            &json!({"k":"flight","kind":"rifle","flight":77,"seq":0,"f":3,"fo":2,"x":130.,"y":4.,"z":130.,"v":[10.,0.,0.],"gravity":9.8,"time":1.,"expires":4.,"until":1.15,"local":true}),
            "spectator.shots.flight",
        );
        assert_eq!(
            rows["shots"][3],
            json!({"t":3,"to":2,"x":130.,"z":130.,"k":"hurt","kill":true,"fo":2})
        );
        assert_eq!(
            rows["shots"][4],
            json!({"t":3,"to":2,"x":130.,"z":130.,"k":"hurt","kill":true,"fo":2,"local":true})
        );

        // A seated recipient keeps its existing dead-identity redaction even
        // when reveal admits these distant events and their ground cells.
        state.world.game.reveal = true;
        assert_eq!(
            SnapshotCache::new(&state)
                .unwrap()
                .recipient(0)
                .unwrap()
                .fields["units"][0][9],
            0
        );
        let shots = events
            .iter()
            .map(ShotRef::try_from)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let seated = shots::project_shots_perspective(
            &state,
            &[SelectedSeat {
                slot: 0,
                full: false,
                omit: false,
            }],
            &shots,
            false,
        )
        .unwrap();
        let seated = &seated[0].1;
        for index in 0..3 {
            assert!(seated[index].get("f").is_none());
            assert!(seated[index].get("fo").is_none());
        }
        assert_eq!(
            seated[3],
            json!({"to":2,"x":130.,"z":130.,"k":"hurt","kill":true,"fo":2})
        );
        assert_eq!(
            seated[4],
            json!({"x":130.,"z":130.,"k":"hurt","kill":true,"local":true})
        );
    }

    #[test]
    fn vanished_enemy_footprint_waits_for_seat_zero_team_sight() {
        use crate::{game::Point, match_runtime::BuildingCell};
        let mut state = fixture();
        let cell = 17 * state.world.game.w + 17;
        state.world.game.units.get_mut(&1).unwrap().x = 3.;
        state.world.game.units.get_mut(&1).unwrap().z = 3.;
        state.world.game.units.get_mut(&2).unwrap().x = 4.;
        state.world.game.units.get_mut(&2).unwrap().z = 4.;
        state.world.set_cell(cell, b'T').unwrap();
        state.building_cells.insert(
            cell,
            BuildingCell {
                unit: 999,
                owner: 2,
                at: Point { x: 35., z: 35. },
            },
        );
        let mut stream = SpectatorDelivery::new(&state).unwrap();
        let first = stream.capture(&state, &[], false).unwrap();
        assert!(
            !first["cells"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row[0] == cell)
        );
        // Moving a seat 0 teammate into actual sight reveals the retained ruin,
        // independently of maintained unit visibility and canonical reveal.
        state.world.game.units.get_mut(&2).unwrap().x = 35.;
        state.world.game.units.get_mut(&2).unwrap().z = 35.;
        assert_eq!(
            stream.capture(&state, &[], false).unwrap()["cells"],
            json!([])
        );
        state.world.game.vision_tick = Some(state.world.game.vision_tick.unwrap_or(-1) + 1);
        let next = stream.capture(&state, &[], false).unwrap();
        assert!(
            next["cells"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row[0] == cell)
        );
        assert_eq!(
            stream.capture(&state, &[], false).unwrap()["cells"],
            json!([])
        );
    }

    #[test]
    fn rejected_watcher_capture_preserves_stream_and_world_admission_is_closed() {
        let mut state = fixture();
        let mut stream = SpectatorDelivery::new(&state).unwrap();
        stream.capture(&state, &[], true).unwrap();
        let before = stream.clone();
        let event =
            TickEvent::Environment(crate::environment::Effect::LastHit { unit: 1, owner: 0 });
        assert!(stream.capture(&state, &[event], false).is_err());
        assert_eq!(stream, before);
        state.options.mode = Mode::World;
        assert_eq!(
            stream.capture(&state, &[], false),
            Err(SnapshotError::UnsupportedMode)
        );
        assert_eq!(stream, before);
    }
}
