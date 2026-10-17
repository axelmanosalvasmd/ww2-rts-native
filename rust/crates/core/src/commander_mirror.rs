//! Per-look assumptions over detached recipient facts, discarded after planning.
use crate::{
    catalog::{Catalog, Value as Rule},
    commander_ai::Input,
    economy::{self, AbilityCost, Currency, Technology},
};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Mirror {
    input: Input,
    cooldowns: BTreeMap<u32, f64>,
    stances: BTreeMap<(u32, String), bool>,
}

impl Mirror {
    pub(crate) fn new(input: &Input) -> Self {
        Self {
            input: input.clone(),
            cooldowns: BTreeMap::new(),
            stances: BTreeMap::new(),
        }
    }

    pub(crate) fn input(&self) -> &Input {
        &self.input
    }

    /// Overrides exist only after this look queues the corresponding order.
    #[cfg(test)]
    pub(crate) fn cooldown(&self, id: u32) -> Option<f64> {
        self.cooldowns.get(&id).copied()
    }

    #[cfg(test)]
    pub(crate) fn stance(&self, id: u32, key: &str) -> Option<bool> {
        self.stances.get(&(id, key.into())).copied()
    }

    /// Undefined submission queues an assumed acceptance. An immediate refusal
    /// changes nothing. Actual delayed execution never calls back into this copy.
    pub(crate) fn apply(&mut self, command: &Value, accepted: bool) -> Result<(), String> {
        if !accepted {
            return Ok(());
        }
        let mut next = self.clone();
        next.apply_inner(command)?;
        *self = next;
        Ok(())
    }

    fn apply_inner(&mut self, command: &Value) -> Result<(), String> {
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let mode = self
            .input
            .runtime
            .as_ref()
            .ok_or("missing mirror context")?
            .mode;
        let kind = command["t"].as_str().ok_or("missing mirror command type")?;
        let name = |key: &str| {
            command[key]
                .as_str()
                .ok_or_else(|| format!("missing mirror {key}"))
        };
        let number = |rule: &Rule, key: &str| {
            rule.get(key)
                .and_then(Rule::number)
                .ok_or_else(|| format!("missing mirror rule {key}"))
        };
        match kind {
            "buy" => {
                let price = economy::price_of(mode, name("unit")?).map_err(|e| e.to_string())?;
                self.input.manpower -= price.mp;
                self.input.runtime.as_mut().unwrap().fuel -= price.fuel;
            }
            "support" => {
                let support = name("kind")?;
                let price = economy::sup_cost(mode, support).map_err(|e| e.to_string())?;
                match price.currency {
                    Currency::Manpower => self.input.manpower -= price.cost,
                    Currency::Munitions => {
                        self.input.runtime.as_mut().unwrap().munitions -= price.cost
                    }
                }
                let cd = number(catalog.support(support).map_err(|e| e.to_string())?, "cd")?;
                self.input
                    .runtime
                    .as_mut()
                    .unwrap()
                    .support
                    .insert(support.into(), cd);
            }
            "build" => {
                self.input.manpower -= number(
                    catalog.unit(name("kind")?).map_err(|e| e.to_string())?,
                    "cost",
                )?;
            }
            "tech" => {
                let technology = match name("kind")? {
                    "tier" => Technology::Tier,
                    "inf" => Technology::Infantry,
                    "guns" => Technology::Guns,
                    "armor" => Technology::Armor,
                    _ => return Err("unknown mirror technology".into()),
                };
                let research = self
                    .input
                    .research
                    .as_ref()
                    .ok_or("missing mirror research")?;
                let price = economy::tech_cost(research.tech, technology, mode)
                    .map_err(|e| e.to_string())?
                    .ok_or("missing mirror research price")?;
                self.input.manpower -= price.mp;
                self.input.runtime.as_mut().unwrap().munitions -= price.mun;
            }
            "dig" => {
                let fort = command["kind"].as_str().unwrap_or("trench");
                let price = catalog
                    .table("FORTS")
                    .map_err(|e| e.to_string())?
                    .get(fort)
                    .and_then(|r| r.get("cost"))
                    .and_then(Rule::number)
                    .unwrap_or(number(
                        catalog.table("CFG").map_err(|e| e.to_string())?,
                        "digCost",
                    )?);
                self.input.manpower -= price;
            }
            _ => {}
        }
        for id in command["ids"].as_array().into_iter().flatten() {
            let Some(unit) = id
                .as_u64()
                .and_then(|id| self.input.units.iter_mut().find(|u| u.id as u64 == id))
            else {
                continue;
            };
            if unit.owner != self.input.slot {
                continue;
            }
            match kind {
                "ability" => {
                    let ability = catalog
                        .unit(&unit.kind)
                        .map_err(|e| e.to_string())?
                        .get("ab")
                        .ok_or("missing mirror ability")?;
                    let id = ability
                        .get("id")
                        .and_then(Rule::string)
                        .ok_or("missing mirror ability id")?;
                    if matches!(id, "grenade" | "satchel" | "barrage") {
                        unit.nade = true;
                    } else {
                        let cost = economy::ab_cost(
                            mode,
                            AbilityCost {
                                id,
                                munitions: ability.get("mun").and_then(Rule::number),
                            },
                        )
                        .map_err(|e| e.to_string())?;
                        self.input.runtime.as_mut().unwrap().munitions -= cost;
                        let cooldown = number(ability, "cd")?;
                        self.cooldowns.insert(unit.id, cooldown);
                        self.input
                            .runtime
                            .as_mut()
                            .unwrap()
                            .units
                            .entry(unit.id)
                            .or_default()
                            .cooldown = cooldown;
                        if id == "ura" {
                            unit.supp = 0.0;
                        }
                    }
                }
                "build" | "assist" => unit.build = true,
                "garrison" => unit.enter = true,
                "fireat" => unit.fire_at = true,
                "dig" => unit.dig = true,
                "entrench" => unit.entrench = true,
                "stance" => {
                    let key = name("key")?;
                    let on = command["on"]
                        .as_bool()
                        .ok_or("missing mirror stance value")?;
                    self.stances.insert((unit.id, key.into()), on);
                    if let Some(facts) =
                        self.input.runtime.as_mut().unwrap().units.get_mut(&unit.id)
                    {
                        match key {
                            "autoRetreat" => facts.auto_retreat = on,
                            "holdPos" => facts.hold_position = on,
                            "holdFire" => facts.hold_fire = on,
                            "auto" => facts.auto_cast = on,
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        commander_ai::{PlayerObservation, UnitObservation},
        commander_tactics::{Context, UnitFacts},
        economy::{Mode, TechState},
        game::Point,
    };
    use serde_json::json;

    fn fixture(mode: Mode) -> Input {
        let units = ["rifle", "mg", "conscript", "tank", "engineer", "rifle"]
            .into_iter()
            .enumerate()
            .map(|(i, kind)| UnitObservation {
                id: i as u32 + 1,
                kind: kind.into(),
                owner: usize::from(i == 5),
                at: Point { x: 10.0, z: 10.0 },
                hp: 100.0,
                supp: 70.0,
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
            })
            .collect::<Vec<_>>();
        Input {
            width: 20,
            height: 20,
            tick: 0,
            slot: 0,
            players: [0, 1]
                .map(|team| PlayerObservation {
                    team,
                    out: false,
                    spawn: Point { x: 10.0, z: 10.0 },
                })
                .into(),
            units: units.clone(),
            points: vec![],
            flags: vec![0; 400],
            chars: vec![b'.'; 400],
            manpower: 2000.0,
            research: Some(crate::commander_ai::ResearchObservation {
                faction: economy::Faction::Usa,
                tech: TechState::new(1, 0, 0, 0).unwrap(),
                busy: vec![],
                stock_base: false,
                armory_ready: false,
                standard_army: false,
            }),
            runtime: Some(Context {
                mode,
                fuel: 500.0,
                munitions: 500.0,
                units: units.iter().map(|u| (u.id, UnitFacts::default())).collect(),
                ..Context::default()
            }),
        }
    }

    #[test]
    fn ai12_local_wallets_follow_queued_prices_and_refusals_leave_no_reservation() {
        for mode in [Mode::Conquest, Mode::Classic] {
            let delivered = fixture(mode);
            let saved = delivered.clone();
            let mut mirror = Mirror::new(&delivered);
            let buy = json!({"t":"buy", "unit":"tank"});
            mirror.apply(&buy, false).unwrap();
            assert_eq!(mirror.input(), &delivered);
            mirror.apply(&buy, true).unwrap();
            let price = economy::price_of(mode, "tank").unwrap();
            assert_eq!(mirror.input.manpower, 2000.0 - price.mp);
            assert_eq!(
                mirror.input.runtime.as_ref().unwrap().fuel,
                500.0 - price.fuel
            );
            mirror
                .apply(&json!({"t":"support", "kind":"smoke"}), true)
                .unwrap();
            let support = economy::sup_cost(mode, "smoke").unwrap();
            let expected_mp = 2000.0
                - price.mp
                - if support.currency == Currency::Manpower {
                    support.cost
                } else {
                    0.0
                };
            assert_eq!(mirror.input.manpower, expected_mp);
            assert_eq!(
                mirror.input.runtime.as_ref().unwrap().munitions,
                500.0
                    - if support.currency == Currency::Munitions {
                        support.cost
                    } else {
                        0.0
                    }
            );
            assert!(mirror.input.runtime.as_ref().unwrap().support["smoke"] > 0.0);
            let before = mirror.input.manpower;
            let catalog = Catalog::compiled().unwrap();
            mirror
                .apply(&json!({"t":"build", "kind":"barracks", "ids":[5,6]}), true)
                .unwrap();
            assert_eq!(
                mirror.input.manpower,
                before
                    - catalog
                        .unit("barracks")
                        .unwrap()
                        .get("cost")
                        .and_then(Rule::number)
                        .unwrap()
            );
            let before = mirror.input.manpower;
            let mun = mirror.input.runtime.as_ref().unwrap().munitions;
            let research = economy::tech_cost(
                delivered.research.as_ref().unwrap().tech,
                Technology::Tier,
                mode,
            )
            .unwrap()
            .unwrap();
            mirror
                .apply(&json!({"t":"tech", "kind":"tier"}), true)
                .unwrap();
            assert_eq!(mirror.input.manpower, before - research.mp);
            assert_eq!(
                mirror.input.runtime.as_ref().unwrap().munitions,
                mun - research.mun
            );
            assert_eq!(mirror.input.research, delivered.research);
            let before = mirror.input.manpower;
            mirror
                .apply(&json!({"t":"dig", "ids":[1,6]}), true)
                .unwrap();
            assert_eq!(
                mirror.input.manpower,
                before
                    - catalog
                        .table("FORTS")
                        .unwrap()
                        .get("trench")
                        .unwrap()
                        .get("cost")
                        .and_then(Rule::number)
                        .unwrap()
            );
            assert_eq!(delivered, saved);
            assert_eq!(mirror.input.units[5], delivered.units[5]);
            assert_eq!(Mirror::new(&delivered).input(), &saved);
        }
    }

    #[test]
    fn ai12_pending_work_abilities_and_stances_change_only_own_local_rows() {
        let delivered = fixture(Mode::Classic);
        let mut mirror = Mirror::new(&delivered);
        for t in ["assist", "garrison", "fireat", "dig", "entrench"] {
            mirror
                .apply(&json!({"t":t, "ids":[5,6,999]}), true)
                .unwrap();
        }
        let worker = &mirror.input.units[4];
        assert!(worker.build && worker.enter && worker.fire_at && worker.dig && worker.entrench);
        for key in ["autoRetreat", "holdPos", "holdFire", "auto"] {
            mirror
                .apply(
                    &json!({"t":"stance", "ids":[5,6], "key":key, "on":true}),
                    true,
                )
                .unwrap();
            assert_eq!(mirror.stance(5, key), Some(true));
            assert_eq!(mirror.stance(6, key), None);
        }
        let facts = &mirror.input.runtime.as_ref().unwrap().units[&5];
        assert!(facts.auto_retreat && facts.hold_position && facts.hold_fire && facts.auto_cast);
        let mun = mirror.input.runtime.as_ref().unwrap().munitions;
        mirror
            .apply(&json!({"t":"ability", "ids":[1,6]}), true)
            .unwrap();
        assert!(mirror.input.units[0].nade);
        assert_eq!(mirror.input.runtime.as_ref().unwrap().munitions, mun);
        assert_eq!(mirror.cooldown(1), None);
        mirror
            .apply(&json!({"t":"ability", "ids":[2,3,4,6]}), true)
            .unwrap();
        assert_eq!(mirror.input.runtime.as_ref().unwrap().munitions, mun - 30.0);
        assert_eq!(mirror.input.units[2].supp, 0.0);
        for id in [2, 3, 4] {
            assert!(mirror.cooldown(id).unwrap() > 0.0);
            assert_eq!(
                mirror.input.runtime.as_ref().unwrap().units[&id].cooldown,
                mirror.cooldown(id).unwrap()
            );
        }
        assert_eq!(mirror.input.units[5], delivered.units[5]);
        assert_eq!(mirror.cooldown(6), None);
        let mut free = Mirror::new(&fixture(Mode::Conquest));
        free.apply(&json!({"t":"ability", "ids":[2,3,4]}), true)
            .unwrap();
        assert_eq!(free.input.runtime.as_ref().unwrap().munitions, 500.0);
        for kind in ["commando", "mortar"] {
            let mut input = delivered.clone();
            input.units[0].kind = kind.into();
            let mut aimed = Mirror::new(&input);
            aimed
                .apply(&json!({"t":"ability", "ids":[1]}), true)
                .unwrap();
            assert!(aimed.input.units[0].nade);
            assert_eq!(aimed.cooldown(1), None);
            assert_eq!(aimed.input.runtime.as_ref().unwrap().munitions, 500.0);
        }
        mirror
            .apply(
                &json!({"t":"stance", "ids":[5], "key":"autoRetreat", "on":false}),
                true,
            )
            .unwrap();
        assert!(!mirror.input.runtime.as_ref().unwrap().units[&5].auto_retreat);
        let saved = mirror.clone();
        assert!(
            mirror
                .apply(&json!({"t":"tech", "kind":"missing"}), true)
                .is_err()
        );
        assert_eq!(mirror, saved);
    }
}
