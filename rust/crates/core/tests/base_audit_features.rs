//! Economy and purchase behaviors exercised through the existing native components.
use ww2_core::{
    economy::{Faction, Mode, TechState},
    economy_state::{
        self as es, ArmySize, EconomyState, ModeContext, PlayerEconomy, Region, RegionKind,
        WorldContext,
    },
    production::{self, Denial, ProductionState},
};
fn wallets() -> EconomyState {
    EconomyState {
        players: vec![
            PlayerEconomy {
                team: 0,
                mp: 10000.0,
                ..Default::default()
            },
            PlayerEconomy {
                team: 0,
                ..Default::default()
            },
            PlayerEconomy {
                team: 1,
                ..Default::default()
            },
        ],
    }
}
fn context<'a>(mode: Mode) -> es::Context<'a> {
    es::Context {
        mode: ModeContext {
            kind: mode,
            ..Default::default()
        },
        army: ArmySize::Standard,
        units: &[],
        drops: &[],
        points: &[],
        nodes: &[],
        world: None,
    }
}
#[test]
fn feature_base04_conquest_points_catchup_and_absent_players() {
    for (army, multiplier) in [
        (ArmySize::Standard, 1.0),
        (ArmySize::Large, 3.0),
        (ArmySize::Massive, 6.0),
        (ArmySize::Endless, 20.0),
    ] {
        let mut w = wallets();
        w.players[2].vp = 720.0;
        let mut c = context(Mode::Default);
        c.army = army;
        c.points = &[
            es::Point {
                owner: Some(0),
                cut: false,
                mp: 2.0,
                vp: 1.0,
            },
            es::Point {
                owner: Some(0),
                cut: true,
                mp: 99.0,
                vp: 99.0,
            },
        ];
        w.advance_income(&c, 1.0, false).unwrap();
        assert_eq!(w.players[0].inc, 12.0 * multiplier);
        assert_eq!(w.players[0].vp, 1.0);
        let paid = w.players[0].mp;
        w.players[0].away = true;
        w.advance_income(&c, 1.0, false).unwrap();
        assert_eq!(w.players[0].mp, paid);
        assert_eq!(w.players[0].vp, 1.0);
        w.players[0].away = false;
        w.players[0].out = true;
        w.advance_income(&c, 1.0, true).unwrap();
        assert_eq!(w.players[0].mp, paid);
    }
}
#[test]
fn feature_base06_classic_depot_completion_destruction_and_currency_income() {
    let mut w = wallets();
    let mut c = context(Mode::Classic);
    c.nodes = &[
        es::Node {
            depot: Some(8),
            rate: 2.5,
            fuel: false,
            region: None,
        },
        es::Node {
            depot: Some(9),
            rate: 1.5,
            fuel: true,
            region: None,
        },
    ];
    c.points = &[
        es::Point {
            owner: Some(1),
            cut: false,
            mp: 999.0,
            vp: 2.0,
        },
        es::Point {
            owner: Some(0),
            cut: true,
            mp: 999.0,
            vp: 9.0,
        },
    ];
    let mut depots = [
        es::DepotFacts {
            id: 8,
            owner: Some(0),
            hp: 400.0,
            built: 0.5,
        },
        es::DepotFacts {
            id: 9,
            owner: Some(0),
            hp: 400.0,
            built: 0.5,
        },
    ];
    w.advance_income_with_depots(&c, &depots, 1.0, false)
        .unwrap();
    assert_eq!(w.players[0].inc, 3.0);
    assert_eq!(w.players[0].fuel, 0.5);
    assert_eq!(w.players[0].mun, 3.5);
    assert_eq!(w.players[0].vp, 0.0);
    for d in &mut depots {
        d.built = 1.0;
    }
    w.advance_income_with_depots(&c, &depots, 1.0, false)
        .unwrap();
    assert_eq!(w.players[0].inc, 5.5);
    assert_eq!(w.players[0].fuel_inc, Some(2.0));
    depots[0].hp = 0.0;
    depots[1].owner = Some(2);
    w.advance_income_with_depots(&c, &depots, 1.0, false)
        .unwrap();
    assert_eq!(w.players[0].inc, 3.0);
    assert_eq!(w.players[0].fuel_inc, Some(0.5));
    let before = w.players[0].clone();
    w.players[0].out = true;
    w.advance_income_with_depots(&c, &depots, 1.0, false)
        .unwrap();
    assert_eq!(
        (w.players[0].mp, w.players[0].fuel, w.players[0].mun),
        (before.mp, before.fuel, before.mun)
    );
}
#[test]
fn feature_base08_world_region_loss_removes_income_and_depot_payment() {
    let mut w = wallets();
    let regions = [
        Region {
            team: 0,
            kind: RegionKind::City,
        },
        Region {
            team: 0,
            kind: RegionKind::Industrial,
        },
        Region {
            team: 0,
            kind: RegionKind::Resource,
        },
    ];
    let mut c = context(Mode::World);
    c.world = Some(WorldContext { regions: &regions });
    c.nodes = &[es::Node {
        depot: Some(8),
        rate: 1.5,
        fuel: true,
        region: Some(1),
    }];
    let depots = [es::DepotFacts {
        id: 8,
        owner: Some(0),
        hp: 400.0,
        built: 1.0,
    }];
    w.advance_income_with_depots(&c, &depots, 1.0, false)
        .unwrap();
    assert_eq!(w.players[0].inc, 4.275);
    assert_eq!(w.players[0].fuel_inc, Some(2.15));
    assert_eq!(w.players[0].mun, 0.55);
    let mut regions_after_loss = regions;
    regions_after_loss[1].team = 1;
    c.world = Some(WorldContext {
        regions: &regions_after_loss,
    });
    w.advance_income_with_depots(&c, &depots, 1.0, false)
        .unwrap();
    assert_eq!(w.players[0].inc, 3.95);
    assert_eq!(w.players[0].fuel_inc, Some(0.5));
    assert_eq!(w.players[0].mun, 1.0);
}
#[test]
fn feature_base16_world_capacity_loss_and_active_seat_split() {
    let mut w = wallets();
    let regions = vec![
        Region {
            team: 0,
            kind: RegionKind::Other
        };
        8
    ];
    let mut c = context(Mode::World);
    c.world = Some(WorldContext { regions: &regions });
    assert_eq!(w.population_cap(&c, 0).unwrap(), 30.0);
    let mut regions_after_loss = regions.clone();
    regions_after_loss[0].team = 1;
    c.world = Some(WorldContext {
        regions: &regions_after_loss,
    });
    assert_eq!(w.population_cap(&c, 0).unwrap(), 29.0);
    w.players[1].out = true;
    assert_eq!(w.population_cap(&c, 0).unwrap(), 34.0);
    c.army = ArmySize::Massive;
    assert_eq!(w.population_cap(&c, 0).unwrap(), 170.0);
    let capped_regions = vec![
        Region {
            team: 0,
            kind: RegionKind::Other
        };
        200
    ];
    c.world = Some(WorldContext {
        regions: &capped_regions,
    });
    assert_eq!(w.population_cap(&c, 0).unwrap(), 800.0);
}

fn recruit_context<'a>(units: &'a [production::UnitView<'a>]) -> production::Context<'a> {
    production::Context {
        mode: ModeContext::default(),
        army: ArmySize::Standard,
        tech_enabled: false,
        naval: false,
        sudden_death: false,
        units,
        drops: &[],
        world: None,
    }
}
#[test]
fn feature_base13_conquest_population_two_accepts_two_then_refuses_without_debit() {
    let state = ProductionState::new(vec![TechState::new(1, 0, 0, 0).unwrap(); 3]).unwrap();
    let mut w = wallets();
    let mut units = vec![production::UnitView {
        id: 1,
        owner: Some(0),
        kind: "hq",
        hp: 100.0,
        built: 1.0,
    }];
    for id in 2..12 {
        units.push(production::UnitView {
            id,
            owner: Some(0),
            kind: "rifle",
            hp: 100.0,
            built: 1.0,
        });
    }
    for id in 12..14 {
        let c = recruit_context(&units);
        let before = w.players[0].mp;
        let completed = state
            .prepare_recruit(&c, &mut w, 0, Faction::Usa, "rifle", Some(1))
            .unwrap()
            .commit();
        assert_eq!(w.players[0].mp, before - completed.request.spend.mp);
        units.push(production::UnitView {
            id,
            owner: Some(0),
            kind: "rifle",
            hp: 100.0,
            built: 1.0,
        });
    }
    let before = w.clone();
    let c = recruit_context(&units);
    assert!(matches!(
        state.prepare_recruit(&c, &mut w, 0, Faction::Usa, "rifle", Some(1)),
        Err(production::Error::Denied(Denial::Pop))
    ));
    assert_eq!(w, before);
    assert!(state.building(1).is_none());
}
#[test]
fn feature_base17_recruitment_refusal_precedence_and_maker_validity() {
    let state = ProductionState::new(vec![TechState::new(1, 0, 0, 0).unwrap(); 3]).unwrap();
    let mut w = wallets();
    w.players[0].mp = 0.0;
    let mut units = [
        production::UnitView {
            id: 1,
            owner: Some(0),
            kind: "hq",
            hp: 100.0,
            built: 1.0,
        },
        production::UnitView {
            id: 2,
            owner: Some(1),
            kind: "hq",
            hp: 100.0,
            built: 1.0,
        },
    ];
    let mut c = recruit_context(&units);
    c.tech_enabled = true;
    for (kind, expected) in [
        ("worldbase", Denial::Blocked),
        ("engineer", Denial::Needs),
        ("gunboat", Denial::Needs),
        ("mortar", Denial::Tier),
        ("rifle", Denial::Mp),
    ] {
        assert!(
            matches!(state.prepare_recruit(&c,&mut w,0,Faction::Usa,kind,None),Err(production::Error::Denied(d)) if d==expected),
            "{kind}"
        );
    }
    w.players[0].mp = 1000.0;
    let c = recruit_context(&units);
    assert!(matches!(
        state.prepare_recruit(&c, &mut w, 0, Faction::Usa, "rifle", Some(2)),
        Err(production::Error::Denied(Denial::Needs))
    ));
    units[0].built = 0.5;
    let c = recruit_context(&units);
    assert!(matches!(
        state.prepare_recruit(&c, &mut w, 0, Faction::Usa, "rifle", None),
        Err(production::Error::Denied(Denial::Needs))
    ));
}
#[test]
fn feature_base18_instant_recruit_selects_owned_finished_maker_without_queue() {
    let state = ProductionState::new(vec![TechState::new(1, 0, 0, 0).unwrap(); 3]).unwrap();
    let mut w = wallets();
    let units = [
        production::UnitView {
            id: 1,
            owner: Some(0),
            kind: "hq",
            hp: 100.0,
            built: 1.0,
        },
        production::UnitView {
            id: 2,
            owner: Some(0),
            kind: "barracks",
            hp: 100.0,
            built: 1.0,
        },
    ];
    for (from, expected) in [(None, 1), (Some(2), 2)] {
        let c = recruit_context(&units);
        let paid = state
            .prepare_recruit(&c, &mut w, 0, Faction::Usa, "rifle", from)
            .unwrap()
            .commit();
        assert_eq!(paid.request.building, expected);
        assert_eq!(paid.request.spend.fuel, 0.0);
    }
    assert!(state.building(1).is_none());
    assert!(state.building(2).is_none());
}
#[test]
fn feature_base02_03_conquest_starting_wallet_and_caps_scale_all_named_armies() {
    use ww2_core::{
        map::AuthoritativeMap,
        match_runtime::{MatchOptions, PlayerConfig, StartPlan},
        rules::Rules,
        weather::WeatherChoice,
    };
    let map=AuthoritativeMap::from_json(&serde_json::to_vec(&serde_json::json!({"name":"native economy","w":40,"h":40,"rows":vec![".".repeat(40);40],"spawns":[{"x":5,"y":5},{"x":34,"y":34}],"points":[]})).unwrap()).unwrap();
    let rules = Rules::compiled().unwrap();
    for (army, income, cap) in [
        (ArmySize::Standard, 1.0, 12.0),
        (ArmySize::Large, 3.0, 30.0),
        (ArmySize::Massive, 6.0, 60.0),
        (ArmySize::Endless, 20.0, 60.0),
    ] {
        let options = MatchOptions {
            seed: 0x3c1055,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "A".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "B".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army,
            weather: WeatherChoice::Map,
            weather_seed: None,
            living_weather: true,
            map_key: None,
            tech: false,
            supply: true,
            logistics: false,
            sandbox: false,
        };
        let plan = StartPlan::prepare(&map, &options, &rules).unwrap();
        for (slot, p) in plan.economy.players.iter().enumerate() {
            assert_eq!(p.mp, 150.0 * income);
            let mut c = context(Mode::Default);
            c.army = army;
            assert_eq!(plan.economy.population_cap(&c, slot).unwrap(), cap);
        }
    }
}
