//! Seat shopping priorities from detached observations and private lessons.
use crate::{
    catalog::{Catalog, Value as Rule},
    commander_ai::{Level, Observation, UnitObservation},
    commander_mind::{Mind, Sighting},
    commander_tactics::Skill,
    economy::{self, Mode},
};
use serde_json::{Value, json};

pub(crate) struct Shopping {
    pub buy: String,
    pub commands: Vec<Value>,
    pub remaining_mp: f64,
}

fn flag(d: &Rule, key: &str) -> bool {
    d.get(key).and_then(Rule::boolean).unwrap_or(false)
}
fn heavy(kind: &str) -> bool {
    matches!(kind, "tank" | "medium" | "tiger" | "churchill")
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan(
    observation: &Observation,
    mine: &[&UnitObservation],
    recent: &[&Sighting],
    remembered_armor: usize,
    mind: &Mind,
    counter: Option<&str>,
    budget: f64,
    available_engineers: usize,
    reserves: (f64, f64),
    level: Level,
) -> Result<Shopping, String> {
    let input = observation.input();
    let context = input.runtime.as_ref().ok_or("missing shopping context")?;
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let def = |kind: &str| catalog.unit(kind).map_err(|e| e.to_string());
    let count = |kind: &str| mine.iter().filter(|u| u.kind == kind).count();
    let skill = Skill::for_level(level);
    let classic = context.mode.is_construction();
    let skirmish = context.mode.is_skirmish() && !context.horde;
    let adaptive = classic && skill.adaptive;
    let remembered = adaptive || skill.memory;
    let team = input.players[input.slot].team;
    let enemy = |u: &&UnitObservation| input.players[u.owner].team != team;
    let enemies: Vec<_> = input.units.iter().filter(enemy).collect();
    let trains = |kind: &str| -> Result<bool, String> {
        if input
            .research
            .as_ref()
            .is_some_and(|r| economy::tier_of(kind).unwrap_or(u8::MAX) > r.tech.tier())
        {
            return Ok(false);
        }
        if !classic && !skirmish {
            return Ok(true);
        }
        for building in &input.units {
            let access = building.owner == input.slot
                || (skirmish
                    && context.mode == Mode::Horde
                    && input.players[building.owner].team == team);
            if !access
                || building.hp <= 0.0
                || !context
                    .units
                    .get(&building.id)
                    .is_some_and(|f| f.built >= 1.0)
            {
                continue;
            }
            let d = def(&building.kind)?;
            let makes = d.get("makes").and_then(Rule::array);
            if makes.is_some_and(|types| types.iter().any(|t| t.string() == Some(kind)))
                || (skirmish && makes.is_some() && building.kind == "barracks" && kind == "rifle")
            {
                return Ok(true);
            }
        }
        Ok(false)
    };
    let can = |kind: &str| economy::can_build(kind, context.faction).map_err(|e| e.to_string());
    let price = |kind: &str| economy::price_of(context.mode, kind).map_err(|e| e.to_string());
    let affords = |kind: &str| -> Result<bool, String> {
        let p = price(kind)?;
        Ok(budget >= p.mp && context.fuel >= p.fuel)
    };
    let seen_armor = if remembered {
        recent.iter().filter(|s| heavy(&s.kind)).count()
    } else {
        0
    };
    let seen_tanks = enemies
        .iter()
        .filter(|u| heavy(&u.kind))
        .count()
        .max(seen_armor)
        .max(if context.horde { 0 } else { remembered_armor });
    let seen_inf = if remembered {
        recent
            .iter()
            .filter(|s| def(&s.kind).is_ok_and(|d| flag(d, "infantry")))
            .count()
    } else {
        0
    };
    let now = input.tick as f64 / 20.0;
    let enemy_planes = enemies
        .iter()
        .filter(|u| def(&u.kind).is_ok_and(|d| flag(d, "air")))
        .count()
        .max(if skill.memory {
            recent
                .iter()
                .filter(|s| now - s.time < 30.0 && def(&s.kind).is_ok_and(|d| flag(d, "air")))
                .count()
        } else {
            0
        });
    let tanks = count("tank") + count("medium") + count("tiger") + count("churchill");
    let flak = count("flak") + count("flaktrack");
    let infantry = mine
        .iter()
        .filter(|u| def(&u.kind).is_ok_and(|d| flag(d, "infantry")))
        .count();
    let air = if enemy_planes > 0 && flak < 3.min(enemy_planes.div_ceil(2)) {
        Some(if trains("flaktrack")? && tanks > 0 {
            "flaktrack"
        } else {
            "flak"
        })
    } else if enemy_planes > count("fighter") && trains("fighter")? {
        Some("fighter")
    } else if count("attacker") < 2 && mine.len() >= 8 && trains("attacker")? {
        Some("attacker")
    } else if count("bomber") < 1 && count("attacker") >= 1 && mine.len() >= 9 && trains("bomber")?
    {
        Some("bomber")
    } else {
        None
    };
    let garrison = enemies.iter().any(|u| u.garrison);
    let dug_in = enemies
        .iter()
        .any(|u| matches!(u.kind.as_str(), "mg" | "at") && matches!(u.cover, 1 | 2));
    let want = if let Some(air) = air {
        air
    } else if seen_tanks > count("at") + count("tankdestroyer") {
        "at"
    } else if garrison && count("rocket") == 0 {
        "rocket"
    } else if garrison && count("flamer") == 0 {
        "flamer"
    } else if tanks == 0 && mine.len() >= 4 {
        "tank"
    } else if (dug_in || mine.len() >= 6) && count("mortar") == 0 {
        "mortar"
    } else if mine.len() >= 8 && count("howitzer") == 0 && trains("howitzer")? {
        "howitzer"
    } else if seen_inf >= 6 && count("sniper") == 0 && mine.len() >= 5 {
        "sniper"
    } else if count("medic") == 0 && infantry >= 5 {
        "medic"
    } else if count("armoredcar") == 0 && mine.len() >= 7 {
        "armoredcar"
    } else if tanks < 2 && mine.len() >= 8 {
        "tank"
    } else if count("mg") * 2 < count("rifle") || (adaptive && seen_inf >= 6 && count("mg") < 3) {
        "mg"
    } else {
        "rifle"
    };
    let mut buy = want;
    if want == "tank" && trains("medium")? && affords("medium")? {
        buy = "medium";
    }
    if want == "at"
        && count("at") > count("tankdestroyer")
        && trains("tankdestroyer")?
        && affords("tankdestroyer")?
    {
        buy = "tankdestroyer";
    }
    if want == "rifle" && can("conscript")? {
        buy = "conscript";
    }
    if want == "rifle" && can("ranger")? && count("ranger") < 2 && count("rifle") >= 1 {
        buy = "ranger";
    }
    if can("tiger")?
        && count("tiger") == 0
        && mine.len() >= 5
        && affords("tiger")?
        && matches!(want, "tank" | "rifle")
    {
        buy = "tiger";
    }
    if want == "rifle" && can("commando")? && count("commando") < 2 && count("rifle") >= 1 {
        buy = "commando";
    }
    if can("churchill")?
        && count("churchill") == 0
        && mine.len() >= 5
        && affords("churchill")?
        && matches!(want, "tank" | "rifle")
    {
        buy = "churchill";
    }
    if context.mode == Mode::World
        && want == "rifle"
        && count("lorry") < 3.min(infantry / 6)
        && trains("lorry")?
        && affords("lorry")?
    {
        buy = "lorry";
    }
    if !context.horde
        && let Some(learned) = mind.lesson(buy, count("at"), count("mg")).or(counter)
    {
        buy = learned;
    }
    if !trains(buy)? || !can(buy)? || price(buy)?.fuel > context.fuel {
        buy = if input.research.is_some()
            && count("mg") * 2 < count("rifle") + count("conscript")
            && trains("mg")?
        {
            "mg"
        } else if trains("conscript")? && can("conscript")? {
            "conscript"
        } else {
            "rifle"
        };
    }
    let engineers = available_engineers;
    let mut commands = Vec::new();
    if context.mode == Mode::World
        && (engineers == 0
            || !input
                .units
                .iter()
                .any(|u| u.owner == input.slot && u.kind == "hq"))
    {
        commands.push(json!({"t":"recover"}));
    }
    if classic
        && engineers
            < if crate::commander_construction::free_nodes(observation) > 0 {
                2
            } else {
                1
            }
    {
        buy = "engineer";
    }
    let reserve = if mine.len() >= 3 {
        reserves.0 + reserves.1
    } else {
        0.0
    };
    let mut remaining_mp = budget;
    let mut remaining_fuel = context.fuel;
    if !context.horde && trains(buy)? {
        let p = price(buy)?;
        for _ in 0..if context.large_army { 4 } else { 1 } {
            if remaining_mp - reserve >= p.mp && remaining_fuel >= p.fuel {
                commands.push(json!({"t":"buy","unit":buy}));
                remaining_mp -= p.mp;
                remaining_fuel -= p.fuel;
            }
        }
    }
    Ok(Shopping {
        buy: buy.into(),
        commands,
        remaining_mp,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        commander_ai::{Input, PlayerObservation, Point, ResearchObservation},
        commander_tactics::{Context, UnitFacts},
        economy::{Faction, TechState},
    };

    fn observation(kinds: &[&str], enemies: &[&str], context: Context) -> Observation {
        let catalog = Catalog::compiled().unwrap();
        let units = kinds
            .iter()
            .map(|k| (*k, 0))
            .chain(enemies.iter().map(|k| (*k, 1)))
            .enumerate()
            .map(|(i, (kind, owner))| {
                let d = catalog.unit(kind).unwrap();
                let n = |key| d.get(key).and_then(Rule::number).unwrap();
                UnitObservation {
                    id: i as u32 + 1,
                    kind: kind.into(),
                    owner,
                    at: Point { x: 30.0, z: 30.0 },
                    hp: n("models") * n("hpPer"),
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
            })
            .collect::<Vec<_>>();
        let mut context = context;
        for u in &units {
            context.units.insert(
                u.id,
                UnitFacts {
                    built: 1.0,
                    ..UnitFacts::default()
                },
            );
        }
        Observation::new(Input {
            width: 40,
            height: 40,
            tick: 2000,
            slot: 0,
            players: vec![
                PlayerObservation {
                    team: 0,
                    out: false,
                    spawn: Point { x: 20.0, z: 20.0 },
                },
                PlayerObservation {
                    team: 1,
                    out: false,
                    spawn: Point { x: 80.0, z: 80.0 },
                },
            ],
            units,
            points: vec![],
            flags: vec![0; 1600],
            chars: vec![b'.'; 1600],
            manpower: 2000.0,
            research: None,
            runtime: Some(context),
        })
        .unwrap()
    }
    fn run(observation: &Observation, budget: f64, mind: &Mind, counter: Option<&str>) -> Shopping {
        run_reserved(observation, budget, mind, counter, (0.0, 0.0))
    }
    fn run_reserved(
        observation: &Observation,
        budget: f64,
        mind: &Mind,
        counter: Option<&str>,
        reserves: (f64, f64),
    ) -> Shopping {
        let catalog = Catalog::compiled().unwrap();
        let mine: Vec<_> = observation
            .input()
            .units
            .iter()
            .filter(|u| {
                u.owner == 0
                    && u.kind != "engineer"
                    && !flag(catalog.unit(&u.kind).unwrap(), "structure")
            })
            .collect();
        plan(
            observation,
            &mine,
            &[],
            0,
            mind,
            counter,
            budget,
            observation
                .input()
                .units
                .iter()
                .filter(|u| u.owner == 0 && u.kind == "engineer")
                .count(),
            reserves,
            Level::Normal,
        )
        .unwrap()
    }
    fn context(faction: Faction) -> Context {
        Context {
            mode: Mode::Other,
            faction,
            fuel: 2000.0,
            ..Context::default()
        }
    }

    #[test]
    fn intent_ai12_each_large_army_purchase_reserves_its_own_fuel() {
        let mode = Mode::Classic;
        let price = economy::price_of(mode, "medium").unwrap();
        assert!(price.fuel > 0.0);
        let make = |fuel| {
            observation(
                &[
                    "hq",
                    "barracks",
                    "motorpool",
                    "engineer",
                    "rifle",
                    "rifle",
                    "rifle",
                    "rifle",
                ],
                &[],
                Context {
                    mode,
                    fuel,
                    large_army: true,
                    faction: Faction::Usa,
                    ..Context::default()
                },
            )
        };
        let input = make(price.fuel);
        let before = input.clone();
        let one = run(&input, price.mp * 4.0, &Mind::default(), Some("medium"));
        assert_eq!(one.buy, "medium");
        assert_eq!(one.commands.iter().filter(|c| c["t"] == "buy").count(), 1);
        assert_eq!(one.remaining_mp, price.mp * 3.0);
        assert_eq!(input, before);
        let four = run(
            &make(price.fuel * 4.0),
            price.mp * 4.0,
            &Mind::default(),
            Some("medium"),
        );
        assert_eq!(four.commands.iter().filter(|c| c["t"] == "buy").count(), 4);
        assert_eq!(four.remaining_mp, 0.0);
    }

    #[test]
    fn intent_ai33_air_armor_and_garrison_priorities_precede_tank() {
        let army = ["rifle", "rifle", "rifle", "rifle"];
        let o = observation(&army, &["tank", "fighter"], context(Faction::Usa));
        assert_eq!(run(&o, 2000.0, &Mind::default(), None).buy, "flak");
        let o = observation(&army, &["tank"], context(Faction::Usa));
        assert_eq!(run(&o, 2000.0, &Mind::default(), None).buy, "at");
        let mut o = observation(&army, &["rifle"], context(Faction::Usa))
            .input()
            .clone();
        o.units.last_mut().unwrap().garrison = true;
        assert_eq!(
            run(
                &Observation::new(o).unwrap(),
                2000.0,
                &Mind::default(),
                None
            )
            .buy,
            "rocket"
        );
        let o = observation(&army, &[], context(Faction::Usa));
        assert_eq!(run(&o, 2000.0, &Mind::default(), None).buy, "medium");
    }

    #[test]
    fn intent_ai33_faction_swaps_and_counter_after_swap() {
        for (faction, expected) in [
            (Faction::Usa, "ranger"),
            (Faction::Ussr, "conscript"),
            (Faction::Uk, "commando"),
            (Faction::Germany, "rifle"),
        ] {
            let o = observation(&["rifle", "mg"], &[], context(faction));
            assert_eq!(run(&o, 2000.0, &Mind::default(), None).buy, expected);
            assert_eq!(run(&o, 2000.0, &Mind::default(), Some("at")).buy, "at");
        }
        for (faction, expected) in [(Faction::Germany, "tiger"), (Faction::Uk, "churchill")] {
            let o = observation(
                &["rifle", "rifle", "rifle", "rifle", "rifle"],
                &[],
                context(faction),
            );
            assert_eq!(run(&o, 2000.0, &Mind::default(), None).buy, expected);
        }
    }

    #[test]
    fn intent_ai33_garrison_followup_mortar_and_medic_priorities() {
        let mut input = observation(
            &["rocket", "tank", "rifle"],
            &["rifle"],
            context(Faction::Germany),
        )
        .input()
        .clone();
        input.units.last_mut().unwrap().garrison = true;
        assert_eq!(
            run(
                &Observation::new(input).unwrap(),
                2000.0,
                &Mind::default(),
                None
            )
            .buy,
            "flamer"
        );
        let o = observation(
            &["tank", "rifle", "rifle", "rifle", "mg", "mg"],
            &[],
            context(Faction::Germany),
        );
        assert_eq!(run(&o, 2000.0, &Mind::default(), None).buy, "mortar");
        let o = observation(
            &["tank", "mortar", "rifle", "rifle", "rifle", "mg", "mg"],
            &[],
            context(Faction::Germany),
        );
        assert_eq!(run(&o, 2000.0, &Mind::default(), None).buy, "medic");
    }

    #[test]
    fn intent_ai33_learned_armor_overrides_faction_rifle_substitution() {
        let mut mind = Mind::default();
        let old = observation(&["rifle"], &[], context(Faction::Ussr));
        mind.update(&old, &[], true);
        let mut input = observation(&["conscript", "mg"], &[], context(Faction::Ussr))
            .input()
            .clone();
        input.tick += 20;
        for u in &mut input.units {
            u.id += 10;
        }
        let next = Observation::new(input).unwrap();
        mind.update(
            &next,
            &[Sighting {
                id: 99,
                kind: "tank".into(),
                owner: 1,
                at: Point { x: 30.0, z: 30.0 },
                time: 101.0,
                worth: 100.0,
            }],
            true,
        );
        assert_eq!(run(&next, 2000.0, &mind, Some("mg")).buy, "at");
    }

    #[test]
    fn intent_ai33_facility_tier_and_fuel_fallbacks() {
        let o = observation(
            &["rifle", "mg"],
            &[],
            Context {
                mode: Mode::Conquest,
                ..context(Faction::Usa)
            },
        );
        let result = run(&o, 2000.0, &Mind::default(), None);
        assert_eq!(result.buy, "rifle");
        assert!(result.commands.is_empty());
        let o = observation(
            &["rifle", "mg", "barracks"],
            &[],
            Context {
                mode: Mode::Conquest,
                ..context(Faction::Usa)
            },
        );
        assert_eq!(run(&o, 2000.0, &Mind::default(), Some("tank")).buy, "rifle");
        let mut input = o.input().clone();
        input.research = Some(ResearchObservation {
            faction: Faction::Usa,
            tech: TechState::new(1, 0, 0, 0).unwrap(),
            busy: vec![],
            stock_base: false,
            armory_ready: false,
            standard_army: true,
        });
        assert_eq!(
            run(
                &Observation::new(input).unwrap(),
                2000.0,
                &Mind::default(),
                Some("tank")
            )
            .buy,
            "rifle"
        );
        let o = observation(
            &[
                "engineer",
                "engineer",
                "rifle",
                "mg",
                "hq",
                "barracks",
                "motorpool",
            ],
            &[],
            Context {
                mode: Mode::Classic,
                fuel: 0.0,
                ..context(Faction::Germany)
            },
        );
        assert_eq!(run(&o, 2000.0, &Mind::default(), Some("tank")).buy, "rifle");
    }

    #[test]
    fn intent_ai33_engineers_reserves_multibuy_and_horde() {
        let o = observation(
            &["engineer", "hq", "barracks"],
            &[],
            Context {
                mode: Mode::Classic,
                nodes: vec![Point { x: 70.0, z: 70.0 }],
                ..context(Faction::Germany)
            },
        );
        assert_eq!(run(&o, 2000.0, &Mind::default(), None).buy, "engineer");
        let o = observation(
            &["engineer", "hq", "barracks"],
            &[],
            Context {
                mode: Mode::Classic,
                nodes: vec![],
                ..context(Faction::Germany)
            },
        );
        assert_eq!(run(&o, 2000.0, &Mind::default(), None).buy, "rifle");
        let p = economy::price_of(Mode::Other, "mg").unwrap().mp;
        let o = observation(
            &["rifle", "rifle", "rifle"],
            &[],
            Context {
                large_army: true,
                ..context(Faction::Germany)
            },
        );
        let r = run_reserved(&o, p * 5.0, &Mind::default(), None, (p, p));
        assert_eq!(r.commands.len(), 3);
        assert_eq!(r.remaining_mp, p * 2.0);
        let o = observation(
            &["rifle"],
            &[],
            Context {
                horde: true,
                ..context(Faction::Germany)
            },
        );
        assert!(run(&o, 2000.0, &Mind::default(), None).commands.is_empty());
    }

    #[test]
    fn intent_ai33_world_lorry_overrides_faction_and_recovers_without_hq() {
        let kinds = [
            "engineer",
            "engineer",
            "hq",
            "barracks",
            "motorpool",
            "tank",
            "tank",
            "mortar",
            "howitzer",
            "medic",
            "armoredcar",
            "rifle",
            "rifle",
            "rifle",
            "mg",
            "mg",
            "mg",
        ];
        let o = observation(
            &kinds,
            &[],
            Context {
                mode: Mode::World,
                ..context(Faction::Usa)
            },
        );
        assert_eq!(run(&o, 2000.0, &Mind::default(), None).buy, "lorry");
        let o = observation(
            &["barracks"],
            &[],
            Context {
                mode: Mode::World,
                ..context(Faction::Usa)
            },
        );
        let result = run(&o, 2000.0, &Mind::default(), None);
        assert_eq!(result.buy, "engineer");
        assert_eq!(result.commands.first(), Some(&json!({"t":"recover"})));
    }
}
