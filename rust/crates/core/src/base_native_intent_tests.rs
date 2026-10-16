//! Native behavior checks under Axel's 6206861267 decision. No Source replay.
use super::*;
use crate::{
    economy::{self, TechState, Technology},
    economy_state::{EconomyState, ModeContext, PlayerEconomy},
    production::{Context, Denial as ProductionDenial, Error, UnitView},
    recipient_snapshot,
};

fn ordinary(tech: bool) -> Match {
    let source = input(false);
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&source["map"]).unwrap()).unwrap();
    let mut options = make(&source).options.clone();
    options.mode = Mode::Conquest;
    options.tech = tech;
    Match::new(&map, options).unwrap()
}
fn dispatch(m: &mut Match, owner: usize, value: Json) -> Option<crate::match_commands::Denial> {
    m.execute_command(owner, &Command::from_json(&value).unwrap(), false)
        .unwrap()
}
fn denied(m: &mut Match, owner: usize, value: Json, expected: &str) {
    assert_eq!(dispatch(m, owner, value).unwrap().name(), expected);
}
fn ticks(m: &mut Match, count: usize, construction: bool) {
    for _ in 0..count {
        if construction {
            m.advance_classic_depot_tick().unwrap();
        } else {
            m.advance_classic_base_tick().unwrap();
        }
    }
}
fn site(m: &mut Match, name: &str, queue: bool) -> UnitId {
    let engineer = own(m, 0, "engineer");
    let (node, at) = choose(m);
    let at = if name == "depot" {
        at
    } else {
        let hq = m.world.game.units[&own(m, 0, "hq")].at();
        Point {
            x: hq.x + 20.,
            z: hq.z,
        }
    };
    let id = m.next_unit_id;
    assert_eq!(
        dispatch(
            m,
            0,
            json!({"t":"build","kind":name,"ids":[engineer],"x":at.x,"z":at.z,"queue":queue})
        ),
        None
    );
    if name == "depot" {
        assert_eq!(
            m.classic_runtime.as_ref().unwrap().nodes[node].depot,
            Some(id)
        );
    }
    id
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
    let (mut p, mut w) = component();
    let units = [view(1, 0, "hq"), view(2, 0, "hq"), view(3, 1, "hq")];
    let c = context(&units);
    let before = w.players[0].mp;
    for _ in 0..10 {
        p.queue_unit(&c, &mut w, 0, Faction::Usa, "rifle", None)
            .unwrap();
    }
    assert_eq!(p.building(1).unwrap().jobs().len(), 5);
    assert_eq!(p.building(2).unwrap().jobs().len(), 5);
    assert!(p.building(3).is_none());
    assert_eq!(
        w.players[0].mp,
        before - 10. * economy::price_of(Mode::Classic, "rifle").unwrap().mp
    );
    assert_eq!(
        p.queue_unit(&c, &mut w, 0, Faction::Usa, "rifle", None),
        Err(Error::Denied(ProductionDenial::QueueFull))
    );
    let mut death = c;
    death.sudden_death = true;
    assert_eq!(
        p.queue_unit(&death, &mut w, 0, Faction::Usa, "rifle", None),
        Err(Error::Denied(ProductionDenial::SuddenDeath))
    );
    let (mut p, mut w) = component();
    assert_eq!(
        p.queue_unit(&c, &mut w, 0, Faction::Usa, "rifle", Some(3)),
        Err(Error::Denied(ProductionDenial::QueueFull))
    );
    let (mut fuel_state, mut fuel_wallet) = component();
    let motorpool = [view(4, 0, "motorpool")];
    let mp = fuel_wallet.players[0].mp;
    let fuel = fuel_wallet.players[0].fuel;
    fuel_state
        .queue_unit(
            &context(&motorpool),
            &mut fuel_wallet,
            0,
            Faction::Usa,
            "tank",
            Some(4),
        )
        .unwrap();
    assert_eq!(fuel_wallet.players[0].mp, mp - 200.);
    assert_eq!(fuel_wallet.players[0].fuel, fuel - 60.);
    let foreign = [view(3, 1, "hq")];
    assert_eq!(
        p.queue_unit(&context(&foreign), &mut w, 0, Faction::Usa, "rifle", None),
        Err(Error::Denied(ProductionDenial::Needs))
    );
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
fn base22_waiting_identity_refund_preserves_progress_and_owner_rows() {
    let mut m = make(&input(true));
    ticks(&mut m, 1000, false);
    let hq = own(&m, 0, "hq");
    for _ in 0..3 {
        assert_eq!(
            dispatch(&mut m, 0, json!({"t":"buy","unit":"rifle","from":hq})),
            None
        );
    }
    ticks(&mut m, 40, false);
    let before = m.production.building(hq).unwrap().clone();
    let head = before.jobs()[0].id;
    let waiter = before.jobs()[2].id;
    denied(
        &mut m,
        1,
        json!({"t":"cancelProduction","id":hq,"job":waiter}),
        "unseen",
    );
    denied(
        &mut m,
        0,
        json!({"t":"cancelProduction","id":hq,"job":head}),
        "notWaiting",
    );
    let mp = m.start.economy.players[0].mp;
    assert_eq!(
        dispatch(
            &mut m,
            0,
            json!({"t":"cancelProduction","id":hq,"job":waiter})
        ),
        None
    );
    assert_eq!(
        m.start.economy.players[0].mp,
        mp + before.jobs()[2].charge.mp
    );
    assert_eq!(
        m.production.building(hq).unwrap().progress(),
        before.progress()
    );
    assert_eq!(
        m.production.building(hq).unwrap().jobs(),
        &before.jobs()[..2]
    );
    let rows = recipient_snapshot::production_for(&m, 0).unwrap();
    assert_eq!(rows["productionJobs"][0][1][0]["status"], "active");
    assert_eq!(rows["productionJobs"][0][1][1]["status"], "waiting");
    for owner in [1, 2] {
        assert!(
            recipient_snapshot::production_for(&m, owner).unwrap()["productionJobs"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r[0] != hq)
        );
    }
    ticks(&mut m, 300, false);
    assert_eq!(
        m.production.building(hq).unwrap().jobs()[0].id,
        before.jobs()[1].id
    );
}

#[test]
fn base23_building_rally_overrides_personal_rally_for_real_buy() {
    let mut m = ordinary(true);
    let hq = own(&m, 0, "hq");
    let enemy = own(&m, 1, "hq");
    assert_eq!(
        dispatch(&mut m, 0, json!({"t":"rally","x":42,"z":30})),
        None
    );
    denied(
        &mut m,
        0,
        json!({"t":"rally","id":enemy,"x":30,"z":30}),
        "unseen",
    );
    assert_eq!(
        dispatch(&mut m, 0, json!({"t":"rally","id":hq,"x":35,"z":32})),
        None
    );
    let id = m.next_unit_id;
    let mp = m.start.economy.players[0].mp;
    assert_eq!(
        dispatch(&mut m, 0, json!({"t":"buy","unit":"rifle","from":hq})),
        None
    );
    assert_eq!(
        m.commands.units[&id].route_end,
        Some(Point { x: 35., z: 32. })
    );
    assert_eq!(m.player_rallies[0], Some(Point { x: 42., z: 30. }));
    assert_eq!(
        m.start.economy.players[0].mp,
        mp - economy::price_of(Mode::Conquest, "rifle").unwrap().mp
    );
    denied(&mut m, 0, json!({"t":"rally","x":-1,"z":30}), "blocked");
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
fn base26_paid_site_binds_node_and_assigns_actual_builder() {
    let mut m = make(&input(false));
    let mp = m.start.economy.players[0].mp;
    let id = site(&mut m, "depot", false);
    let engineer = own(&m, 0, "engineer");
    assert_eq!(m.start.economy.players[0].mp, mp - 60.);
    assert_eq!(m.world.game.units[&id].hp, 150.);
    assert_eq!(m.world.game.units[&id].built, Some(0.));
    assert_eq!(m.commands.units[&engineer].build_target, Some(id));
    assert!(!m.world.game.units[&engineer].path.is_empty());
}

#[test]
fn base27_blocked_unseen_and_node_placement_refuse_without_debit() {
    let mut m = make(&input(false));
    let engineer = own(&m, 0, "engineer");
    let mp = m.start.economy.players[0].mp;
    denied(
        &mut m,
        0,
        json!({"t":"build","kind":"barracks","ids":[engineer],"x":250,"z":10}),
        "notVisible",
    );
    let hq = m.world.game.units[&own(&m, 0, "hq")].at();
    denied(
        &mut m,
        0,
        json!({"t":"build","kind":"barracks","ids":[engineer],"x":hq.x,"z":hq.z}),
        "blocked",
    );
    assert_eq!(m.start.economy.players[0].mp, mp);
    let id = site(&mut m, "depot", false);
    assert!(m.world.game.units[&id].cells.is_some());
}

#[test]
fn base29_actual_engineer_completes_site_before_it_becomes_prerequisite() {
    let mut m = make(&input(false));
    ticks(&mut m, 1000, false);
    let id = site(&mut m, "barracks", false);
    denied(&mut m, 0, json!({"t":"buy","unit":"mg","from":id}), "needs");
    ticks(&mut m, 100, true);
    assert!(m.world.game.units[&id].built.unwrap() < 1.);
    ticks(&mut m, 1000, true);
    assert_eq!(m.world.game.units[&id].built, Some(1.));
    assert_eq!(m.world.game.units[&id].hp, 1500.);
    assert_eq!(
        dispatch(&mut m, 0, json!({"t":"buy","unit":"mg","from":id})),
        None
    );
}

#[test]
fn base30_supported_section_repair_restores_health_without_wallet_charge() {
    use crate::environment::{Context as EnvironmentContext, DamageContext};
    let mut m = make(&input(false));
    let id = site(&mut m, "depot", false);
    ticks(&mut m, 800, true);
    let cell = m.world.game.units[&id].cells.as_ref().unwrap()[0] as usize;
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let skip = BTreeSet::new();
    let mut hit = DamageContext::plain(&skip);
    hit.owner = Some(1);
    hit.contact_id = Some("native-intent-section");
    let mut context = EnvironmentContext {
        tick: m.world.game.tick as u64,
        seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &order,
        grid: &m.grid,
        rain: m.world.game.rain,
        wet: m.world.game.wet.unwrap_or(0.),
        wind_angle: m.start.wind.0,
        wind_strength: m.start.wind.1,
    };
    let (damage, effects) = m
        .environment
        .damage_section(&mut m.world, cell, 50., &mut context, &hit)
        .unwrap();
    assert!(damage > 0.);
    for effect in effects {
        crate::match_combat::absorb_environment_effect(&mut m.combat, effect, &mut |_| {});
    }
    let hp = m.world.game.units[&id].hp;
    let mp = m.start.economy.players[0].mp;
    assert!(m.repair_building_sections(id, 10.).unwrap() > 0.);
    assert!(m.world.game.units[&id].hp > hp);
    assert_eq!(m.start.economy.players[0].mp, mp);
    assert!(!m.world.game.sections[&(cell as u32)].failed);
}

#[test]
fn base31_cancel_paid_site_refunds_three_quarters_and_removes_sections() {
    let mut m = make(&input(false));
    let id = site(&mut m, "depot", false);
    let cells = m.world.game.units[&id].cells.clone().unwrap();
    let mp = m.start.economy.players[0].mp;
    denied(&mut m, 1, json!({"t":"cancel","id":id}), "unseen");
    assert_eq!(dispatch(&mut m, 0, json!({"t":"cancel","id":id})), None);
    assert_eq!(m.start.economy.players[0].mp, mp + 45.);
    assert!(!m.world.game.units.contains_key(&id));
    assert!(
        cells
            .iter()
            .all(|c| m.world.game.chars[*c as usize] == b'.'
                && !m.world.game.sections.contains_key(c))
    );
    assert!(
        m.classic_runtime
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .all(|n| n.depot != Some(id))
    );
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
fn base38_airfield_trains_aircraft_only_after_tier_gate() {
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
fn base45_supplycache_is_an_unfinished_nonproducer() {
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
fn base52_classic_and_conquest_start_with_their_own_forces() {
    for m in [make(&input(false)), ordinary(true)] {
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
            if m.options.mode == Mode::Classic {
                assert_eq!(names.iter().filter(|&&k| k == "engineer").count(), 1);
                assert!(!names.contains(&"barracks"));
            } else {
                assert_eq!(names.iter().filter(|&&k| k == "rifle").count(), 2);
                assert!(names.contains(&"mg") && names.contains(&"barracks"));
            }
        }
    }
}

#[test]
fn base53_assault_defender_gets_trenches_bunker_and_four_flak() {
    let source = input(false);
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&source["map"]).unwrap()).unwrap();
    let mut options = make(&source).options.clone();
    options.mode = Mode::Assault;
    let m = Match::new_assault(&map, options, 1).unwrap();
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
fn base54_guard_fort_types_are_finished_owned_structural_cells() {
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
fn base55_recovery_engineer_queue_obeys_price_pop_and_capacity() {
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
fn base56_real_classic_deadline_clears_paid_queues_and_starts_decay() {
    let mut m = make(&input(false));
    ticks(&mut m, 29980, false);
    assert!(!m.classic_sudden_death());
    let hq = own(&m, 0, "hq");
    for _ in 0..3 {
        assert_eq!(
            dispatch(&mut m, 0, json!({"t":"buy","unit":"rifle","from":hq})),
            None
        );
    }
    let spent = m.story[0].mp_spent;
    for _ in 0..24 {
        if m.classic_sudden_death() {
            break;
        }
        ticks(&mut m, 1, false);
    }
    assert!(m.classic_sudden_death());
    assert!(m.production.building(hq).unwrap().jobs().is_empty());
    assert_eq!(m.story[0].mp_spent, spent);
    denied(
        &mut m,
        0,
        json!({"t":"cancelProduction","id":hq,"job":123}),
        "suddenDeath",
    );
    denied(&mut m, 0, json!({"t":"cancelProduction"}), "suddenDeath");
    denied(&mut m, 0, json!({"t":"tech","kind":"tier"}), "suddenDeath");
    denied(
        &mut m,
        0,
        json!({"t":"buy","unit":"rifle","from":hq}),
        "suddenDeath",
    );
    let hp = m.world.game.units[&hq].hp;
    ticks(&mut m, 200, false);
    assert!((hp - m.world.game.units[&hq].hp - 300.).abs() < 0.001);
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
fn base59_live_queue_snapshot_keeps_ids_progress_rally_and_owner() {
    let mut m = make(&input(true));
    let hq = own(&m, 0, "hq");
    assert_eq!(
        dispatch(&mut m, 0, json!({"t":"rally","id":hq,"x":35,"z":30})),
        None
    );
    assert_eq!(
        dispatch(&mut m, 0, json!({"t":"buy","unit":"rifle","from":hq})),
        None
    );
    ticks(&mut m, 40, false);
    let rows = recipient_snapshot::production_for(&m, 0).unwrap();
    let q = rows["queues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r[0] == hq)
        .unwrap();
    assert!(q[1].as_f64().unwrap() > 0. && q[1].as_f64().unwrap() < 1.);
    assert_eq!(q[2], 35.);
    assert_eq!(q[3], 30.);
    assert_eq!(q[4], "rifle");
    for owner in [1, 2] {
        let rows = recipient_snapshot::production_for(&m, owner).unwrap();
        for field in ["queues", "productionJobs"] {
            assert!(rows[field].as_array().unwrap().iter().all(|r| r[0] != hq));
        }
    }
}
