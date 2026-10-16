//! Main-supported native checks under owner decision 6206861267.
#[path = "bases_runtime2_tests.rs"]
mod runtime2;
use super::*;
use crate::{
    economy::{self, Faction, TechState, Technology},
    economy_state::{ArmySize, EconomyState, ModeContext, PlayerEconomy},
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{MatchOptions, PlayerConfig},
    production::{Context, Denial as ProductionDenial, Error, ProductionState, UnitView},
    weather::WeatherChoice,
};
use serde_json::{Value as Json, json};

fn map() -> AuthoritativeMap {
    AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Native base intent", "w":128, "h":128,
            "rows":vec![".".repeat(128);128],
            "spawns":[{"x":8,"y":8},{"x":112,"y":112}],
            "points":[{"id":"neutral","x":64,"y":64,"mp":0,"vp":0}]
        }))
        .unwrap(),
    )
    .unwrap()
}
fn options(mode: Mode, tech: bool) -> MatchOptions {
    MatchOptions {
        seed: 701,
        mode,
        players: (0..2)
            .map(|slot| PlayerConfig {
                name: format!("p{slot}"),
                team: slot,
                faction: if slot == 0 {
                    Faction::Usa
                } else {
                    Faction::Germany
                },
            })
            .collect(),
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: Some(217),
        living_weather: false,
        map_key: None,
        tech,
        supply: true,
        logistics: false,
        sandbox: false,
    }
}
fn ordinary(tech: bool) -> Match {
    Match::new(&map(), options(Mode::Conquest, tech)).unwrap()
}
fn own(m: &Match, slot: usize, kind: &str) -> UnitId {
    m.world
        .game
        .units
        .values()
        .find(|u| u.owner == slot && m.rules.kinds[u.kind].name == kind)
        .unwrap()
        .id
}
fn dispatch(m: &mut Match, owner: usize, value: Json) -> Option<crate::match_commands::Denial> {
    m.execute_command(owner, &Command::from_json(&value).unwrap(), false)
        .unwrap()
}
fn denied(m: &mut Match, owner: usize, value: Json, expected: &str) {
    assert_eq!(dispatch(m, owner, value).unwrap().name(), expected);
}
fn view(id: u32, owner: usize, kind: &'static str) -> UnitView<'static> {
    UnitView {
        id,
        owner: Some(owner),
        kind,
        hp: 3000.,
        built: 1.,
    }
}
fn context<'a>(units: &'a [UnitView<'static>]) -> Context<'a> {
    Context {
        mode: ModeContext {
            kind: Mode::Classic,
            defender_team: None,
            income_excluded_slot: None,
        },
        army: ArmySize::Standard,
        tech_enabled: true,
        naval: true,
        sudden_death: false,
        units,
        drops: &[],
        world: None,
    }
}
// Component wallets are explicit supplied facts, never installed into a live Match.
fn component() -> (ProductionState, EconomyState) {
    (
        ProductionState::new(vec![TechState::new(3, 0, 0, 0).unwrap(); 3]).unwrap(),
        EconomyState {
            players: (0..3)
                .map(|i| PlayerEconomy {
                    team: if i == 2 { 1 } else { 0 },
                    mp: 10000.,
                    mun: 1000.,
                    fuel: 1000.,
                    ..Default::default()
                })
                .collect(),
        },
    )
}
fn catalog_number(kind: &str, field: &str, expected: f64) {
    assert_eq!(
        Catalog::compiled()
            .unwrap()
            .unit(kind)
            .unwrap()
            .get(field)
            .and_then(Value::number),
        Some(expected)
    );
}
fn catalog_strings(kind: &str, field: &str) -> Vec<&'static str> {
    Catalog::compiled()
        .unwrap()
        .unit(kind)
        .unwrap()
        .get(field)
        .unwrap()
        .array()
        .unwrap()
        .iter()
        .map(|s| s.string().unwrap())
        .collect()
}

#[test]
fn base20_paid_shortest_owned_five_slot_queues() {
    for mode in [Mode::Classic, Mode::World] {
        let (mut p, mut w) = component();
        let mut unfinished = view(5, 0, "hq");
        unfinished.built = 0.5;
        let units = [
            view(1, 0, "hq"),
            view(2, 0, "hq"),
            view(3, 1, "hq"),
            view(4, 0, "rifle"),
            unfinished,
        ];
        let mut c = context(&units);
        c.mode.kind = mode;
        c.world = Some(crate::economy_state::WorldContext { regions: &[] });
        let before = w.clone();
        let price = economy::price_of(mode, "rifle").unwrap();
        for queued in 1usize..=10 {
            p.queue_unit(&c, &mut w, 0, Faction::Usa, "rifle", None)
                .unwrap();
            assert_eq!(p.building(1).unwrap().jobs().len(), queued.div_ceil(2));
            assert_eq!(p.building(2).map_or(0, |b| b.jobs().len()), queued / 2);
            assert_eq!(
                w.players[0].mp,
                before.players[0].mp - queued as f64 * price.mp
            );
            assert_eq!(w.players[0].fuel, before.players[0].fuel);
        }
        assert!(p.building(3).is_none());
        assert!(p.building(4).is_none());
        assert!(p.building(5).is_none());
        let full = p.clone();
        let paid = w.clone();
        for from in [None, Some(1)] {
            assert_eq!(
                p.queue_unit(&c, &mut w, 0, Faction::Usa, "rifle", from),
                Err(Error::Denied(ProductionDenial::QueueFull))
            );
            assert_eq!(p, full);
            assert_eq!(w, paid);
        }
        let (mut p, mut w) = component();
        // Explicit selection must override a shorter eligible queue.
        for _ in 0..2 {
            p.queue_unit(&c, &mut w, 0, Faction::Usa, "rifle", Some(1))
                .unwrap();
        }
        assert!(p.building(2).is_none());
        p.queue_unit(&c, &mut w, 0, Faction::Usa, "rifle", None)
            .unwrap();
        p.queue_unit(&c, &mut w, 0, Faction::Usa, "rifle", Some(1))
            .unwrap();
        assert_eq!(p.building(1).unwrap().jobs().len(), 3);
        assert_eq!(p.building(2).unwrap().jobs().len(), 1);
        assert_eq!(w.players[0].mp, before.players[0].mp - 4. * price.mp);
        let unchanged = p.clone();
        let wallet = w.clone();
        // An invalid explicit maker cannot fall back to another eligible building.
        for from in [3, 4, 5, 99] {
            assert_eq!(
                p.queue_unit(&c, &mut w, 0, Faction::Usa, "rifle", Some(from)),
                Err(Error::Denied(ProductionDenial::QueueFull))
            );
            assert_eq!(p, unchanged);
            assert_eq!(w, wallet);
        }
        let mut death = c;
        death.sudden_death = true;
        for from in [None, Some(1)] {
            assert_eq!(
                p.queue_unit(&death, &mut w, 0, Faction::Usa, "rifle", from),
                Err(Error::Denied(ProductionDenial::SuddenDeath))
            );
            assert_eq!(p, unchanged);
            assert_eq!(w, wallet);
        }
        for unavailable in [&units[2..], &units[0..0]] {
            let mut no_maker = c;
            no_maker.units = unavailable;
            assert_eq!(
                p.queue_unit(&no_maker, &mut w, 0, Faction::Usa, "rifle", None),
                Err(Error::Denied(ProductionDenial::Needs))
            );
            assert_eq!(p, unchanged);
            assert_eq!(w, wallet);
        }
        let (mut fuel_state, mut fuel_wallet) = component();
        let motorpool = [view(6, 0, "motorpool")];
        let mut fuel_context = c;
        fuel_context.units = &motorpool;
        let mp = fuel_wallet.players[0].mp;
        let fuel = fuel_wallet.players[0].fuel;
        let job = fuel_state
            .queue_unit(
                &fuel_context,
                &mut fuel_wallet,
                0,
                Faction::Usa,
                "tank",
                Some(6),
            )
            .unwrap();
        assert_eq!(fuel_wallet.players[0].mp, mp - 200.);
        assert_eq!(fuel_wallet.players[0].fuel, fuel - 60.);
        let jobs = fuel_state.building(6).unwrap().jobs();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, job);
        assert_eq!(jobs[0].charge, economy::price_of(mode, "tank").unwrap());
        assert_eq!(fuel_wallet.players[1..], before.players[1..]);
    }
}

#[test]
fn base21_training_pauses_and_emits_each_catalog_kind_at_its_duration() {
    let trains = [
        ("engineer", 12.),
        ("rifle", 15.),
        ("conscript", 12.),
        ("mg", 18.),
        ("flak", 20.),
        ("mortar", 20.),
        ("sniper", 20.),
        ("medic", 15.),
        ("ranger", 20.),
        ("at", 22.),
        ("halftrack", 22.),
        ("lorry", 15.),
        ("lcvp", 20.),
        ("gunboat", 25.),
        ("destroyer", 75.),
        ("armoredcar", 25.),
        ("flaktrack", 30.),
        ("fighter", 30.),
        ("attacker", 35.),
        ("bomber", 45.),
        ("rocket", 30.),
        ("tank", 35.),
        ("medium", 40.),
        ("tankdestroyer", 40.),
        ("tiger", 50.),
        ("churchill", 50.),
        ("commando", 22.),
        ("howitzer", 30.),
        ("flamer", 20.),
    ];
    for (kind, train) in trains {
        catalog_number(kind, "train", train);
        let (mut p, w) = component();
        p.record_paid(1, kind, economy::price_of(Mode::Classic, kind).unwrap())
            .unwrap();
        let mut units = [view(1, 0, "hq")];
        units[0].built = 0.5;
        assert!(
            p.advance(&context(&units), &w, train)
                .unwrap()
                .units
                .is_empty()
        );
        assert_eq!(p.building(1).unwrap().progress(), 0.);
        units[0].built = 1.;
        assert!(
            p.advance(&context(&units), &w, train - 0.25)
                .unwrap()
                .units
                .is_empty()
        );
        let mut c = context(&units);
        c.sudden_death = true;
        assert!(p.advance(&c, &w, 1.).unwrap().units.is_empty());
        let out = p.advance(&context(&units), &w, 0.25).unwrap().units;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, kind);
        assert_eq!(out[0].owner, 0);
        assert!(p.building(1).unwrap().jobs().is_empty());
    }
}

#[test]
fn base24_horde_allies_share_finished_producers_and_labs_only() {
    let (p, w) = component();
    let units = [
        view(1, 0, "hq"),
        view(2, 0, "barracks"),
        view(3, 0, "armory"),
    ];
    let mut c = context(&units);
    c.mode.kind = Mode::Horde;
    c.mode.income_excluded_slot = Some(2);
    assert_eq!(
        p.production_buildings(&c, &w, 1, Some("rifle")).unwrap(),
        [1, 2]
    );
    assert_eq!(p.tech_labs(&c, &w, 1, Technology::Infantry).unwrap(), 1);
    assert!(p.production_buildings(&c, &w, 2, None).unwrap().is_empty());
    c.mode.kind = Mode::Conquest;
    assert!(p.production_buildings(&c, &w, 1, None).unwrap().is_empty());
}

#[test]
fn base32_scuttle_marks_only_owned_living_targets_without_refund() {
    let mut m = ordinary(true);
    let own_hq = own(&m, 0, "hq");
    let enemy = own(&m, 1, "hq");
    let mp = m.start.economy.players[0].mp;
    let hp = m.world.game.units[&enemy].hp;
    assert_eq!(
        dispatch(&mut m, 0, json!({"t":"destroy","ids":[own_hq,enemy]})),
        None
    );
    assert_eq!(m.world.game.units[&own_hq].hp, 0.);
    assert_eq!(m.world.game.units[&enemy].hp, hp);
    assert_eq!(m.start.economy.players[0].mp, mp);
    let enemy_mp = m.start.economy.players[1].mp;
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    m.retire_dead(&order).unwrap();
    assert!(!m.world.game.units.contains_key(&own_hq));
    assert_eq!(m.start.economy.players[0].mp, mp);
    assert_eq!(m.start.economy.players[1].mp, enemy_mp);
}

#[test]
fn base33_destroyed_owned_building_retires_to_rubble() {
    let mut m = ordinary(true);
    let id = own(&m, 0, "hq");
    let cells = m.world.game.units[&id].cells.clone().unwrap();
    assert_eq!(dispatch(&mut m, 0, json!({"t":"destroy","id":id})), None);
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    m.retire_dead(&order).unwrap();
    assert!(!m.world.game.units.contains_key(&id));
    assert!(
        cells
            .iter()
            .all(|c| m.world.game.chars[*c as usize] == b'R')
    );
}

#[test]
fn base37_motorpool_requires_finished_barracks_before_tier() {
    let mut m = ordinary(true);
    for _ in 0..400 {
        m.advance_tick().unwrap();
    }
    let rifle = own(&m, 0, "rifle");
    let barracks = own(&m, 0, "barracks");
    m.world.game.units.get_mut(&barracks).unwrap().built = Some(0.5);
    denied(
        &mut m,
        0,
        json!({"t":"build","kind":"motorpool","ids":[rifle],"x":35,"z":20}),
        "needs",
    );
    m.world.game.units.get_mut(&barracks).unwrap().built = Some(1.);
    // Earn rather than overwrite the live wallet before testing tier admission.
    denied(
        &mut m,
        0,
        json!({"t":"build","kind":"motorpool","ids":[rifle],"x":35,"z":20}),
        "tier",
    );
    catalog_number("motorpool", "hpPer", 1900.);
    catalog_number("motorpool", "buildTime", 45.);
}

#[test]
fn base38_airfield_queues_catalog_aircraft() {
    let (mut p, mut w) = component();
    let units = [view(1, 0, "airfield")];
    let c = context(&units);
    for kind in ["fighter", "attacker", "bomber"] {
        p.queue_unit(&c, &mut w, 0, Faction::Usa, kind, Some(1))
            .unwrap();
    }
    assert_eq!(
        p.building(1).unwrap().queue().collect::<Vec<_>>(),
        ["fighter", "attacker", "bomber"]
    );
    assert_eq!(
        catalog_strings("airfield", "makes"),
        ["fighter", "attacker", "bomber"]
    );
    assert_eq!(
        Catalog::compiled()
            .unwrap()
            .unit("airfield")
            .unwrap()
            .get("needs")
            .and_then(Value::string),
        Some("motorpool")
    );
}

#[test]
fn base39_shipyard_naval_map_gate_precedes_payment() {
    let (mut p, mut w) = component();
    let units = [view(1, 0, "shipyard")];
    let mut c = context(&units);
    c.naval = false;
    let before = w.players[0].mp;
    assert_eq!(
        p.queue_unit(&c, &mut w, 0, Faction::Usa, "lcvp", Some(1)),
        Err(Error::Denied(ProductionDenial::Needs))
    );
    assert_eq!(w.players[0].mp, before);
    c.naval = true;
    p.queue_unit(&c, &mut w, 0, Faction::Usa, "lcvp", Some(1))
        .unwrap();
    assert_eq!(p.building(1).unwrap().jobs()[0].kind, "lcvp");
    assert_eq!(
        Catalog::compiled()
            .unwrap()
            .unit("shipyard")
            .unwrap()
            .get("coast")
            .and_then(Value::boolean),
        Some(true)
    );
}

#[test]
fn base44_wall_and_gate_static_dimensions_and_producer_exclusion() {
    for (kind, cost, vision) in [("wall", 30., 4.), ("gate", 50., 8.)] {
        for (field, value) in [
            ("cost", cost),
            ("hpPer", 400.),
            ("size", 1.),
            ("vision", vision),
        ] {
            catalog_number(kind, field, value);
        }
        assert!(!economy::build_kinds(Mode::Classic).unwrap().contains(&kind));
        assert!(
            Catalog::compiled()
                .unwrap()
                .unit(kind)
                .unwrap()
                .get("makes")
                .is_none()
        );
    }
    assert_eq!(
        Catalog::compiled()
            .unwrap()
            .unit("gate")
            .unwrap()
            .get("gate")
            .unwrap()
            .get("shut")
            .and_then(Value::number),
        Some(16.)
    );
}

#[test]
fn base45_supplycache_catalog_and_nonproducer() {
    for (field, value) in [
        ("cost", 60.),
        ("buildTime", 12.),
        ("hpPer", 400.),
        ("size", 2.),
        ("vision", 12.),
    ] {
        catalog_number("supplycache", field, value);
    }
    let (p, w) = component();
    let units = [view(1, 0, "supplycache")];
    assert!(
        p.production_buildings(&context(&units), &w, 0, None)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        Catalog::compiled()
            .unwrap()
            .unit("supplycache")
            .unwrap()
            .get("logisticsCache")
            .and_then(Value::boolean),
        Some(true)
    );
}

#[test]
fn base46_regional_base_is_never_buyable_and_has_no_recruit_list() {
    let (mut p, mut w) = component();
    let units = [view(1, 0, "hq")];
    let before = w.players[0].mp;
    assert_eq!(
        p.queue_unit(&context(&units), &mut w, 0, Faction::Usa, "worldbase", None),
        Err(Error::Denied(ProductionDenial::Blocked))
    );
    assert_eq!(w.players[0].mp, before);
    catalog_number("worldbase", "cost", 0.);
    catalog_number("worldbase", "hpPer", 600.);
    assert!(
        Catalog::compiled()
            .unwrap()
            .unit("worldbase")
            .unwrap()
            .get("makes")
            .is_none()
    );
}

#[test]
fn base47_tech_off_blocks_research_and_removes_purchase_tier_gate() {
    let mut m = ordinary(false);
    denied(&mut m, 0, json!({"t":"tech","kind":"tier"}), "blocked");
    let (_, mut w) = component();
    let mut p = ProductionState::new(vec![TechState::new(1, 0, 0, 0).unwrap(); 3]).unwrap();
    let units = [view(1, 0, "barracks")];
    let mut c = context(&units);
    c.tech_enabled = false;
    p.queue_unit(&c, &mut w, 0, Faction::Usa, "mortar", Some(1))
        .unwrap();
    assert_eq!(p.building(1).unwrap().jobs()[0].kind, "mortar");
}

#[test]
fn base48_tier_gating_refuses_mortar_before_wallet_debit() {
    let (_, mut w) = component();
    let mut p = ProductionState::new(vec![TechState::new(1, 0, 0, 0).unwrap(); 3]).unwrap();
    let units = [view(1, 0, "barracks")];
    let c = context(&units);
    let before = w.players[0].mp;
    assert_eq!(
        p.queue_unit(&c, &mut w, 0, Faction::Usa, "mortar", Some(1)),
        Err(Error::Denied(ProductionDenial::Tier))
    );
    assert_eq!(w.players[0].mp, before);
    for kind in [
        "motorpool",
        "shipyard",
        "armory",
        "pillbox",
        "halftrack",
        "armoredcar",
        "flaktrack",
        "tank",
        "at",
        "mortar",
        "flak",
        "lcvp",
        "gunboat",
    ] {
        assert_eq!(economy::tier_of(kind).unwrap(), 2);
    }
    for kind in [
        "airfield",
        "medium",
        "tankdestroyer",
        "rocket",
        "howitzer",
        "tiger",
        "churchill",
        "ranger",
        "commando",
        "destroyer",
        "fighter",
        "attacker",
        "bomber",
    ] {
        assert_eq!(economy::tier_of(kind).unwrap(), 3);
    }
}

#[test]
fn base52_conquest_starts_with_its_own_forces() {
    let m = ordinary(true);
    for owner in 0..2 {
        let names = m
            .world
            .game
            .units
            .values()
            .filter(|u| u.owner == owner)
            .map(|u| m.rules.kinds[u.kind].name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names.iter().filter(|&&k| k == "hq").count(), 1);
        let hq = &m.world.game.units[&own(&m, owner, "hq")];
        assert_eq!(hq.built, Some(1.));
        assert_eq!(hq.cells.as_ref().unwrap().len(), 9);
        assert_eq!(names.iter().filter(|&&k| k == "rifle").count(), 2);
        assert!(names.contains(&"mg") && names.contains(&"barracks"));
    }
}

#[test]
fn base53_assault_defender_gets_trenches_bunker_and_four_flak() {
    let m = Match::new_assault(&map(), options(Mode::Assault, true), 1).unwrap();
    for (owner, count) in [(0, 0), (1, 4)] {
        let flak = m
            .world
            .game
            .units
            .values()
            .filter(|u| u.owner == owner && m.rules.kinds[u.kind].name == "flakpos")
            .collect::<Vec<_>>();
        assert_eq!(flak.len(), count);
        assert!(flak.iter().all(|u| u.built == Some(1.)));
    }
    assert!(m.world.game.chars.contains(&b'T'));
    assert!(m.world.game.chars.contains(&b'#'));
    assert_eq!(
        m.world
            .game
            .units
            .values()
            .filter(|u| u.owner == 1 && m.rules.kinds[u.kind].name == "bunker")
            .count(),
        1
    );
}

#[test]
fn base54_finished_wall_structure_primitive() {
    // The World constructor's fort creation primitive; region ranking/transfer remains open.
    let mut m = ordinary(true);
    let id = m.next_unit_id;
    m.place_starting_building(0, "wall", 40 * 128 + 40).unwrap();
    let wall = &m.world.game.units[&id];
    assert_eq!(wall.owner, 0);
    assert_eq!(wall.built, Some(1.));
    assert_eq!(wall.hp, 400.);
    assert_eq!(wall.cells.as_ref().unwrap().len(), 1);
}

#[test]
fn base55_engineer_queue_price_pop_and_capacity() {
    let (mut p, mut w) = component();
    let units = [view(1, 0, "hq")];
    let c = context(&units);
    let before = w.players[0].mp;
    p.queue_unit(&c, &mut w, 0, Faction::Usa, "engineer", Some(1))
        .unwrap();
    assert_eq!(w.players[0].mp, before - 60.);
    assert_eq!(p.building(1).unwrap().jobs()[0].kind, "engineer");
    for _ in 0..4 {
        p.queue_unit(&c, &mut w, 0, Faction::Usa, "engineer", Some(1))
            .unwrap();
    }
    assert_eq!(
        p.queue_unit(&c, &mut w, 0, Faction::Usa, "engineer", Some(1)),
        Err(Error::Denied(ProductionDenial::QueueFull))
    );
    let mut full = vec![view(1, 0, "hq")];
    full.extend((2..26).map(|id| view(id, 0, "rifle")));
    let (mut p, mut w) = component();
    assert_eq!(
        p.queue_unit(
            &context(&full),
            &mut w,
            0,
            Faction::Usa,
            "engineer",
            Some(1)
        ),
        Err(Error::Denied(ProductionDenial::Pop))
    );
    catalog_number("engineer", "cost", 60.);
    assert_eq!(
        Catalog::compiled()
            .unwrap()
            .table("WORLD_TUNING")
            .unwrap()
            .get("recoverHQ")
            .and_then(Value::number),
        Some(200.)
    );
}

#[test]
fn base57_ordinary_retreat_selects_living_hq_then_spawn() {
    let mut m = ordinary(true);
    let rifle = own(&m, 0, "rifle");
    let hq = own(&m, 0, "hq");
    assert_eq!(
        m.reaction_home(rifle).unwrap(),
        m.world.game.units[&hq].at()
    );
    m.world.game.units.get_mut(&hq).unwrap().hp = 0.;
    assert_eq!(m.reaction_home(rifle).unwrap(), m.start.players[0].spawn);
}

#[test]
fn base03_army_keys_scale_population_income_and_starting_wallets() {
    // Values from pinned CFG.armies; unknown names select Standard, case sensitively.
    for (key, army, pop, income) in [
        ("standard", ArmySize::Standard, 1.0, 1.0),
        ("large", ArmySize::Large, 2.5, 3.0),
        ("massive", ArmySize::Massive, 5.0, 6.0),
        ("endless", ArmySize::Endless, 5.0, 20.0),
        ("", ArmySize::Standard, 1.0, 1.0),
        ("unknown", ArmySize::Standard, 1.0, 1.0),
        ("Large", ArmySize::Standard, 1.0, 1.0),
    ] {
        assert_eq!(ArmySize::from_key(key), army);
        let mut opts = options(Mode::Conquest, false);
        opts.army = ArmySize::from_key(key);
        let mut m = Match::new(&map(), opts).unwrap();
        assert_eq!(m.start.economy.players[0].mp, 150.0 * income, "{key}");
        assert_eq!(m.start.economy.players[1].mp, 150.0 * income, "{key}");
        let context = crate::economy_state::Context {
            mode: Default::default(),
            army: m.options.army,
            units: &[],
            drops: &[],
            points: &[],
            nodes: &[],
            world: None,
        };
        assert_eq!(
            m.start.economy.population_cap(&context, 0).unwrap(),
            12.0 * pop,
            "{key}"
        );
        m.start
            .economy
            .advance_income(&context, 1.0, false)
            .unwrap();
        assert_eq!(m.start.economy.players[0].inc, 4.0 * income, "{key}");
        assert_eq!(m.start.economy.players[0].mp, 154.0 * income, "{key}");
    }
}

#[test]
fn base02_available_startup_profiles_scale_wallets_by_army_income() {
    let source = map();
    let mut json = source.source().clone();
    json["defend"] = json!([0]);
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&json).unwrap()).unwrap();
    let rules = crate::rules::Rules::compiled().unwrap();
    for (army, income) in [
        (ArmySize::Standard, 1.0),
        (ArmySize::Large, 3.0),
        (ArmySize::Massive, 6.0),
        (ArmySize::Endless, 20.0),
    ] {
        let mut opts = options(Mode::Conquest, false);
        opts.army = army;
        let plan = crate::match_runtime::StartPlan::prepare(&map, &opts, &rules).unwrap();
        assert_eq!(
            plan.economy
                .players
                .iter()
                .map(|p| p.mp)
                .collect::<Vec<_>>(),
            vec![150.0 * income; 2]
        );
        opts.mode = Mode::Assault;
        let plan =
            crate::match_runtime::StartPlan::prepare_assault(&map, &opts, &rules, 1).unwrap();
        assert_eq!(
            plan.economy
                .players
                .iter()
                .map(|p| p.mp)
                .collect::<Vec<_>>(),
            vec![320.0 * income, 250.0 * income]
        );
        opts.mode = Mode::Horde;
        let plan = crate::match_runtime::StartPlan::prepare_horde(&map, &opts, &rules).unwrap();
        assert_eq!(
            plan.economy
                .players
                .iter()
                .map(|p| p.mp)
                .collect::<Vec<_>>(),
            vec![250.0 * income, 0.0]
        );
    }
}
