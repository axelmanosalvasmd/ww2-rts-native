//! Source scuttling keeps work/control state until the next actual retirement phase.
use crate::{match_commands::Denial, match_runtime::Match};

impl Match {
    pub(crate) fn execute_destroy(
        &mut self,
        owner: usize,
        ids: &[i64],
        scalar: Option<i64>,
    ) -> Result<Option<Denial>, String> {
        let game = &self.world.game;
        let limit = 50.max(game.units.len());
        let selected = ids
            .iter()
            .copied()
            .take(limit)
            .chain(scalar)
            .filter_map(|id| u32::try_from(id).ok())
            .filter(|id| {
                game.units
                    .get(id)
                    .is_some_and(|unit| unit.owner == owner && unit.hp > 0.0 && !unit.riding)
            })
            .collect::<Vec<_>>();
        if selected.is_empty() {
            return Ok(Some(Denial::Unseen));
        }
        for &id in &selected {
            if id == 0 || game.units[&id].id != id || self.combat.get(id).is_none() {
                return Err("invalid scuttle unit or combat identity".into());
            }
        }
        let last_hit = i32::try_from(owner).map_err(|_| "invalid scuttle owner")?;
        // Source list construction precedes mutation, including repeated IDs.
        for id in selected {
            self.world.game.units.get_mut(&id).unwrap().hp = 0.0;
            self.combat.get_mut(id).unwrap().last_hit = Some(last_hit);
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::match_commands::Command;
    use serde_json::{Value, json};

    #[test]
    fn scuttle_preserves_controls_and_refuses_atomically_without_a_combat_owner() {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/recipient-snapshot.json"))
                .unwrap();
        let scene = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == "untouched-startup")
            .unwrap();
        let mut state = crate::recipient_snapshot::tests::initialize(scene);
        let owned = state
            .world
            .game
            .units
            .values()
            .filter(|unit| unit.owner == 0 && unit.hp > 0.0)
            .map(|unit| unit.id)
            .take(2)
            .collect::<Vec<_>>();
        assert_eq!(owned.len(), 2);
        let command =
            Command::from_json(&json!({"t":"destroy","ids":[owned[0]],"id":owned[1]})).unwrap();
        let controls = state.commands.units.clone();
        let works = state.field_works.clone();
        let wallets = format!("{:?}", state.start.economy.players);
        let hp = owned
            .iter()
            .map(|id| state.world.game.units[id].hp.to_bits())
            .collect::<Vec<_>>();
        let prior = state.combat.get(owned[0]).unwrap().clone();
        state.combat.remove(owned[1]);
        assert!(state.execute_command(0, &command, false).is_err());
        assert_eq!(state.combat.get(owned[0]), Some(&prior));
        assert_eq!(
            owned
                .iter()
                .map(|id| state.world.game.units[id].hp.to_bits())
                .collect::<Vec<_>>(),
            hp
        );
        state.combat.initialize(owned[1]);
        assert_eq!(state.execute_command(0, &command, false).unwrap(), None);
        for id in owned {
            assert_eq!(state.world.game.units[&id].hp, 0.0);
            assert_eq!(state.combat.get(id).unwrap().last_hit, Some(0));
        }
        assert_eq!(state.commands.units, controls);
        assert_eq!(state.field_works, works);
        assert_eq!(format!("{:?}", state.start.economy.players), wallets);
        assert_eq!(
            state.execute_command(0, &command, false).unwrap(),
            Some(Denial::Unseen)
        );
    }

    fn scuttle_scene() -> (Match, u32, u32, u32) {
        use crate::{
            catalog::Catalog,
            economy::{Faction, Mode},
            economy_state::ArmySize,
            game::Point,
            map::AuthoritativeMap,
            match_commands::UnitControl,
            match_runtime::{MatchOptions, PlayerConfig},
            unit_spawn,
            weather::WeatherChoice,
        };
        let map = AuthoritativeMap::from_json(
            &serde_json::to_vec(&json!({
                "name":"Scuttle passengers", "w":60,"h":60,
                "rows":vec![".".repeat(60);60], "heights":vec!["0".repeat(60);60],
                "spawns":[{"x":4,"y":4},{"x":55,"y":55}], "points":[{"x":50,"y":5}]
            }))
            .unwrap(),
        )
        .unwrap();
        let mut m = Match::new(
            &map,
            MatchOptions {
                seed: 701,
                mode: Mode::Default,
                players: [Faction::Usa, Faction::Germany]
                    .into_iter()
                    .enumerate()
                    .map(|(team, faction)| PlayerConfig {
                        name: format!("p{team}"),
                        team: team as i32,
                        faction,
                    })
                    .collect(),
                shuffle: false,
                army: ArmySize::Standard,
                weather: WeatherChoice::Map,
                weather_seed: Some(1),
                living_weather: false,
                map_key: None,
                tech: false,
                supply: false,
                logistics: false,
                sandbox: false,
            },
        )
        .unwrap();
        let mut added = Vec::new();
        for name in ["halftrack", "rifle"] {
            let id = m.next_unit_id;
            let unit = unit_spawn::initial_unit(
                &m.rules,
                Catalog::compiled().unwrap(),
                m.rules.kind(name).unwrap(),
                id,
                0,
                Point { x: 51.0, z: 51.0 },
            )
            .unwrap();
            m.commands
                .insert(id, UnitControl::new(&unit, &m.rules).unwrap());
            m.combat.initialize(id);
            m.firing.initialize(id);
            m.abilities.initialize(id);
            m.recovery.initialize(id);
            m.world.game.units.insert(id, unit);
            m.next_unit_id += 1;
            added.push(id);
        }
        for unit in m.world.game.units.values_mut() {
            unit.hold_pos = true;
        }
        for control in m.commands.units.values_mut() {
            control.hold_fire = true;
            control.auto_cast = false;
            control.auto_retreat = false;
        }
        m.grid.rebuild(&m.world.game.units);
        m.initialize_recipient_routing().unwrap();
        let barracks = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "barracks")
            .unwrap()
            .id;
        (m, added[0], added[1], barracks)
    }

    #[test]
    fn intent_cmb145_scuttle_retires_owned_units_and_ejects_passengers_without_refund_or_bounty() {
        use crate::{
            environment::Effect,
            game::{CELL, Point},
            match_carriers::BoardingOutcome,
        };
        // The passenger survives the thrown exit, dies from its injury, or is
        // stranded beyond shore reach. All three execute the ordinary full tick.
        for exit in ["survive", "injury", "stranded", "missing carrier"] {
            let (mut m, carrier, rider, building) = scuttle_scene();
            let (mut control, _, _, _) = scuttle_scene();
            assert_eq!(
                m.carrier_board_command(0, &[i64::from(rider)], Some(i64::from(carrier)), false)
                    .unwrap(),
                None
            );
            assert_eq!(
                m.apply_unit_boarding(rider).unwrap(),
                BoardingOutcome::Boarded { carrier }
            );
            m.abilities.get_mut(rider).unwrap().cooldown = 3.0;
            if exit == "injury" {
                m.world.game.units.get_mut(&rider).unwrap().hp = 1.0;
            }
            if exit == "stranded" {
                // The land carrier was left surrounded by impassable ground.
                // The exit must use the passenger's own movement class and drown
                // it when the nearest usable cell is beyond shore reach.
                for y in 20..=30 {
                    for x in 20..=30 {
                        m.world.set_cell(y * m.world.game.w + x, b'W').unwrap();
                    }
                }
            }
            let enemy = m
                .world
                .game
                .units
                .values()
                .find(|u| u.owner == 1 && !m.rules.kinds[u.kind].structure)
                .unwrap()
                .id;
            let enemy_hp = m.world.game.units[&enemy].hp;
            let rider_hp = m.world.game.units[&rider].hp;
            let building_cells = m.world.game.units[&building].cells.clone().unwrap();
            let wallets = m
                .start
                .economy
                .players
                .iter()
                .map(|p| p.mp)
                .collect::<Vec<_>>();
            m.combat.get_mut(carrier).unwrap().last_hit = Some(1);
            assert_eq!(
                m.execute_command(
                    1,
                    &Command::Destroy {
                        ids: vec![i64::from(carrier)],
                        id: None
                    },
                    false
                )
                .unwrap(),
                Some(Denial::Unseen)
            );
            assert_eq!(
                m.execute_command(
                    0,
                    &Command::Destroy {
                        ids: vec![i64::from(rider)],
                        id: None
                    },
                    false
                )
                .unwrap(),
                Some(Denial::Unseen)
            );
            let command = Command::from_json(
                &json!({"t":"destroy","ids":[rider,enemy,building],"id":carrier,"queue":true}),
            )
            .unwrap();
            assert_eq!(m.execute_command(0, &command, false).unwrap(), None);
            assert_eq!(
                m.start
                    .economy
                    .players
                    .iter()
                    .map(|p| p.mp)
                    .collect::<Vec<_>>(),
                wallets
            );
            assert_eq!(m.world.game.units[&enemy].hp, enemy_hp);
            assert_eq!(
                m.world.game.units[&rider].hp, rider_hp,
                "carried units cannot be scuttled"
            );
            for id in [carrier, building] {
                assert_eq!(m.world.game.units[&id].hp, 0.0);
                assert_eq!(m.combat.get(id).unwrap().last_hit, Some(0));
            }
            if exit == "missing carrier" {
                // Death can happen after the rider's turn. Its next turn sees
                // the already retired carrier and must still eject normally.
                let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
                m.retire_dead(&order).unwrap();
                assert!(!m.world.game.units.contains_key(&carrier));
            }
            let report = m.advance_tick().unwrap_or_else(|e| panic!("{exit}: {e}"));
            let baseline = control.advance_tick().unwrap();
            assert_eq!(baseline.tick, report.tick);
            assert!(!m.world.game.units.contains_key(&carrier), "{exit}");
            assert!(!m.world.game.units.contains_key(&building), "{exit}");
            if exit != "stranded" {
                assert!(
                    m.world
                        .game
                        .wrecks
                        .iter()
                        .any(|wreck| wreck.id == Some(carrier)),
                    "{exit}: carrier wreck"
                );
            }
            assert!(
                building_cells
                    .iter()
                    .all(|c| m.world.game.chars[*c as usize] != b'B'),
                "{exit}: building cleared"
            );
            assert!(
                m.combat
                    .get(rider)
                    .is_none_or(|meta| meta.carrier.is_none())
            );
            assert!(
                m.combat
                    .get(carrier)
                    .is_none_or(|meta| meta.cargo.is_empty())
            );
            let hurt = report
                .environment()
                .find_map(|effect| match effect {
                    Effect::Hurt(hurt) if hurt.unit == rider => Some(hurt),
                    _ => None,
                })
                .expect("thrown exit publishes hurt through the existing tick event stream");
            assert_eq!(hurt.owner, 0);
            assert_eq!(hurt.from, None);
            assert_eq!(hurt.killed, matches!(exit, "injury" | "stranded"));
            if hurt.killed {
                assert!(!m.world.game.units.contains_key(&rider));
            } else {
                let passenger = &m.world.game.units[&rider];
                assert!(!passenger.riding);
                assert_eq!(passenger.hp, 70.0, "five 20-HP models lose 30% strength");
                assert_eq!(passenger.at(), Point { x: 51.0, z: 51.0 });
                assert_eq!(
                    m.abilities.get(rider).unwrap().cooldown,
                    2.95,
                    "ejected passenger continues its ground turn"
                );
            }
            if exit == "stranded" {
                assert!(crate::math::hypot(hurt.at.x - 51.0, hurt.at.z - 51.0) > 5.0);
                let cell = m.world.game.cell_of(hurt.at.x, hurt.at.z).unwrap();
                assert_eq!(m.world.game.chars[cell], b'.');
            }
            assert_eq!(m.story[1].kills, 0.0);
            assert!(report.retired.iter().all(|row| row.enemy_killer.is_none()));
            assert_eq!(
                m.start
                    .economy
                    .players
                    .iter()
                    .map(|p| p.mp)
                    .collect::<Vec<_>>(),
                control
                    .start
                    .economy
                    .players
                    .iter()
                    .map(|p| p.mp)
                    .collect::<Vec<_>>(),
                "{exit}: only ordinary income, with no refund or bounty"
            );
            assert_eq!(hurt.at.x % CELL, 1.0);
        }
    }
}
