//! Private notice timers and lessons from a seat's delivered observations.
use std::collections::BTreeMap;

use crate::{commander_ai::Observation, game::Point, math};

#[derive(Clone, Debug, PartialEq)]
pub struct Sighting {
    pub id: u32,
    pub kind: String,
    pub owner: usize,
    pub at: Point,
    pub time: f64,
    pub worth: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mind {
    roster: BTreeMap<u32, Point>,
    respect: BTreeMap<String, u8>,
    noticed: BTreeMap<u32, f64>,
    last_seen: BTreeMap<u32, f64>,
    losses: BTreeMap<u32, (Point, f64)>,
    failures: Vec<u8>,
    lost_points: Vec<bool>,
    owned: Vec<Option<bool>>,
    point_seen: Vec<Option<f64>>,
}

fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}

impl Mind {
    /// Enemy units are current camera-filtered contacts; sightings may be remembered.
    pub fn update(&mut self, observation: &Observation, sightings: &[Sighting], learn: bool) {
        let input = observation.input();
        let now = input.tick as f64 / 20.0;
        let team = input.players[input.slot].team;
        for unit in &input.units {
            if input.players[unit.owner].team != team {
                self.noticed.entry(unit.id).or_insert(now);
                self.last_seen.insert(unit.id, now);
            }
        }
        self.last_seen.retain(|_, time| now - *time <= 60.0);
        self.noticed.retain(|id, _| self.last_seen.contains_key(id));
        self.failures.resize(input.points.len(), 0);
        self.lost_points.resize(input.points.len(), false);
        self.lost_points.fill(false);
        self.owned.resize(input.points.len(), None);
        self.point_seen.resize(input.points.len(), None);

        let alive: BTreeMap<_, _> = input
            .units
            .iter()
            .enumerate()
            .filter(|(index, unit)| {
                unit.owner == input.slot && !observation.unit_is_structure(*index)
            })
            .map(|(_, unit)| (unit.id, unit.at))
            .collect();
        for (&id, &at) in &self.roster {
            if alive.contains_key(&id) {
                continue;
            }
            if learn
                && let Some(threat) = sightings
                    .iter()
                    .filter(|s| {
                        input.players.get(s.owner).is_some_and(|p| p.team != team)
                            && now - s.time <= 25.0
                            && distance(s.at, at) <= 28.0
                    })
                    .min_by(|a, b| {
                        distance(a.at, at)
                            .total_cmp(&distance(b.at, at))
                            .then_with(|| a.id.cmp(&b.id))
                    })
            {
                let respect = self.respect.entry(threat.kind.clone()).or_default();
                *respect = (*respect + 1).min(6);
            }
            if let Some(index) = input
                .points
                .iter()
                .position(|p| distance(p.at, at) <= observation.point_radius() + 12.0)
            {
                self.failures[index] = (self.failures[index] + 1).min(4);
                self.lost_points[index] = true;
            }
            self.losses.insert(id, (at, now));
        }
        self.losses.retain(|_, (_, time)| now - *time <= 30.0);
        self.roster = alive;

        for (index, point) in input.points.iter().enumerate() {
            let owner = point.owner.and_then(|owner| input.players.get(owner));
            let held = owner.is_some_and(|p| p.team == team);
            if held && self.owned[index] == Some(false) {
                self.failures[index] = self.failures[index].saturating_sub(1);
                self.respect.retain(|_, value| {
                    *value = value.saturating_sub(1);
                    *value > 0
                });
            }
            self.owned[index] = Some(held);
            if owner.is_some_and(|p| p.team != team) && self.point_seen[index].is_none() {
                self.point_seen[index] = Some(now);
            }
        }
    }

    pub fn watched(&self, id: u32, now: f64, notice: f64) -> bool {
        self.noticed
            .get(&id)
            .is_some_and(|time| now - time + 1e-9 >= notice)
    }

    pub fn point_ready(&self, index: usize, now: f64, notice: f64) -> bool {
        self.point_seen
            .get(index)
            .copied()
            .flatten()
            .is_some_and(|time| now - time + 1e-9 >= notice)
    }

    pub fn lesson(&self, buy: &str, have_at: usize, have_mg: usize) -> Option<&'static str> {
        let count = |kinds: &[&str]| {
            kinds
                .iter()
                .map(|kind| u32::from(self.respect.get(*kind).copied().unwrap_or(0)))
                .sum::<u32>()
        };
        if count(&["tank", "medium", "tiger"]) >= 1
            && have_at < 2
            && matches!(buy, "rifle" | "conscript" | "ranger" | "mg")
        {
            return Some("at");
        }
        if count(&["rifle", "conscript", "ranger", "mg"]) >= 2
            && have_mg < 3
            && matches!(buy, "rifle" | "conscript" | "ranger")
        {
            return Some("mg");
        }
        None
    }

    pub(crate) fn losses_near(&self, at: Point, now: f64, age: f64, radius: f64) -> usize {
        self.losses
            .values()
            .filter(|(p, t)| now - *t < age && distance(*p, at) < radius)
            .count()
    }

    pub fn point_failures(&self) -> Vec<u8> {
        self.failures.clone()
    }

    /// A loss at this point on the latest update, including losses after the cap.
    pub fn lost_at_point(&self, index: usize) -> bool {
        self.lost_points.get(index).copied().unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commander_ai::{Input, PlayerObservation, PointObservation, UnitObservation};

    fn unit(id: u32, kind: &str, owner: usize, x: f64) -> UnitObservation {
        UnitObservation {
            id,
            kind: kind.into(),
            owner,
            at: Point { x, z: 20.0 },
            hp: 100.0,
            supp: 0.0,
            target_id: 0,
            cover: 0,
            path: vec![],
            attack_id: false,
            retreating: false,
            riding: false,
            build: false,
            dig: false,
            entrench: false,
            garrison: false,
            enter: false,
            fire_at: false,
            nade: false,
        }
    }

    fn observation(tick: i64, units: Vec<UnitObservation>, owner: Option<usize>) -> Observation {
        Observation::new(Input {
            width: 40,
            height: 40,
            tick,
            slot: 0,
            players: [0, 1, 0]
                .map(|team| PlayerObservation {
                    team,
                    out: false,
                    spawn: Point { x: 20.0, z: 20.0 },
                })
                .into(),
            units,
            points: vec![PointObservation {
                at: Point { x: 20.0, z: 20.0 },
                vp: 1.0,
                mp: 1.0,
                owner,
                cut: false,
                locked: false,
            }],
            flags: vec![0; 1600],
            chars: vec![b'.'; 1600],
            manpower: 0.0,
            research: None,
            runtime: None,
        })
        .unwrap()
    }

    fn sighting(id: u32, kind: &str, x: f64, time: f64) -> Sighting {
        Sighting {
            id,
            kind: kind.into(),
            owner: 1,
            at: Point { x, z: 20.0 },
            time,
            worth: 100.0,
        }
    }

    #[test]
    fn intent_ai30_contact_notice_and_unseen_expiry() {
        let mut mind = Mind::default();
        mind.update(
            &observation(200, vec![unit(9, "tank", 1, 24.0)], None),
            &[],
            true,
        );
        assert!(!mind.watched(9, 11.249_999, 1.25));
        assert!(mind.watched(9, 11.25 - 5e-10, 1.25));
        assert!(!mind.watched(10, 20.0, 0.0));
        mind.update(
            &observation(400, vec![unit(9, "tank", 1, 24.0)], None),
            &[],
            true,
        );
        assert_eq!(mind.noticed[&9], 10.0);
        mind.update(&observation(1600, vec![], None), &[], true);
        assert!(mind.watched(9, 80.0, 1.25));
        mind.update(
            &observation(1601, vec![], None),
            &[sighting(9, "tank", 24.0, 80.05)],
            true,
        );
        assert!(!mind.watched(9, 80.05, 0.0));
        mind.update(
            &observation(1602, vec![unit(9, "tank", 1, 24.0)], None),
            &[],
            true,
        );
        assert!(!mind.watched(9, 80.1, 1.25));
        assert!(mind.watched(9, 81.35, 1.25));
    }

    #[test]
    fn intent_ai30_point_notice_starts_at_first_enemy_ownership() {
        let mut mind = Mind::default();
        for owner in [None, Some(0), Some(2)] {
            mind.update(&observation(200, vec![], owner), &[], true);
            assert!(!mind.point_ready(0, 50.0, 0.0));
        }
        mind.update(&observation(1000, vec![], Some(1)), &[], true);
        assert!(!mind.point_ready(0, 50.499_999, 0.5));
        assert!(mind.point_ready(0, 50.5 - 5e-10, 0.5));
        mind.update(&observation(1040, vec![], Some(2)), &[], true);
        mind.update(&observation(1060, vec![], Some(1)), &[], true);
        assert!(mind.point_ready(0, 53.0, 0.5));
        assert!(!mind.point_ready(1, 100.0, 0.0));
    }

    #[test]
    fn intent_ai32_losses_use_nearest_recent_enemy_with_id_ties() {
        let mut mind = Mind::default();
        mind.update(
            &observation(0, vec![unit(1, "rifle", 0, 20.0)], Some(1)),
            &[],
            true,
        );
        let mut sightings = vec![
            sighting(8, "mg", 25.0, 5.0),
            sighting(7, "tank", 15.0, 5.0),
            sighting(6, "tiger", 20.0, 4.999),
            sighting(5, "medium", 48.001, 30.0),
            sighting(4, "rifle", 20.0, 30.0),
        ];
        sightings[4].owner = 2;
        sightings.push(sighting(2, "rifle", 26.0, 30.0));
        mind.update(&observation(600, vec![], Some(1)), &sightings, true);
        assert_eq!(mind.respect.get("tank"), Some(&1));
        assert_eq!(mind.respect.len(), 1);
        assert_eq!(mind.point_failures(), vec![1]);
        assert_eq!(mind.losses[&1], (Point { x: 20.0, z: 20.0 }, 30.0));
        mind.update(&observation(1200, vec![], Some(1)), &[], true);
        assert_eq!(mind.losses.len(), 1);
        mind.update(&observation(1201, vec![], Some(1)), &[], true);
        assert!(mind.losses.is_empty());
        assert_eq!(mind.point_failures(), vec![1]);
    }

    #[test]
    fn intent_ai32_respect_and_failures_cap_and_recapture_spends_each_once() {
        let mut mind = Mind::default();
        let army = (1..=8).map(|id| unit(id, "rifle", 0, 20.0)).collect();
        mind.update(&observation(0, army, Some(1)), &[], true);
        mind.update(
            &observation(20, vec![], Some(1)),
            &[sighting(20, "tank", 48.0, 1.0)],
            true,
        );
        assert_eq!(mind.respect["tank"], 6);
        assert_eq!(mind.point_failures(), vec![4]);
        assert!(mind.lost_at_point(0));
        mind.update(&observation(40, vec![], Some(2)), &[], true);
        assert!(!mind.lost_at_point(0));
        assert_eq!(mind.respect["tank"], 5);
        assert_eq!(mind.point_failures(), vec![3]);
        mind.update(&observation(60, vec![], Some(2)), &[], true);
        assert_eq!(mind.respect["tank"], 5);
        assert_eq!(mind.point_failures(), vec![3]);
        assert!(Mind::default().respect.is_empty());
    }

    #[test]
    fn intent_ai32_roster_excludes_structures_and_learning_can_be_disabled() {
        let mut mind = Mind::default();
        mind.update(
            &observation(
                0,
                vec![unit(1, "rifle", 0, 40.0), unit(2, "hq", 0, 20.0)],
                Some(1),
            ),
            &[],
            false,
        );
        mind.update(
            &observation(20, vec![], Some(1)),
            &[sighting(3, "tank", 40.0, 1.0)],
            false,
        );
        assert!(mind.respect.is_empty());
        assert_eq!(mind.point_failures(), vec![1]);
        assert_eq!(mind.losses.len(), 1);
        mind.update(
            &observation(40, vec![unit(4, "engineer", 0, 40.001)], Some(1)),
            &[],
            true,
        );
        mind.update(&observation(60, vec![], Some(1)), &[], true);
        assert_eq!(mind.point_failures(), vec![1]);
        assert_eq!(mind.losses.len(), 2);
    }

    #[test]
    fn intent_ai32_lessons_prioritize_armor_and_fade_after_captures() {
        let mut mind = Mind::default();
        mind.update(
            &observation(
                0,
                vec![unit(1, "rifle", 0, 20.0), unit(2, "rifle", 0, 24.0)],
                Some(1),
            ),
            &[],
            true,
        );
        mind.update(
            &observation(20, vec![], Some(1)),
            &[sighting(10, "mg", 22.0, 1.0)],
            true,
        );
        assert_eq!(mind.lesson("rifle", 0, 0), Some("mg"));
        assert_eq!(mind.lesson("mg", 0, 0), None);
        assert_eq!(mind.lesson("rifle", 0, 3), None);
        mind.update(
            &observation(40, vec![unit(3, "rifle", 0, 20.0)], Some(1)),
            &[],
            true,
        );
        mind.update(
            &observation(60, vec![], Some(1)),
            &[sighting(11, "tank", 20.0, 3.0)],
            true,
        );
        for buy in ["rifle", "conscript", "ranger", "mg"] {
            assert_eq!(mind.lesson(buy, 1, 0), Some("at"));
        }
        assert_eq!(mind.lesson("rifle", 2, 0), Some("mg"));
        assert_eq!(mind.lesson("tank", 0, 0), None);
        mind.update(&observation(80, vec![], Some(0)), &[], true);
        assert_eq!(mind.lesson("rifle", 0, 0), None);
        assert!(!mind.respect.contains_key("tank"));
        assert_eq!(mind.respect["mg"], 1);
    }
}
