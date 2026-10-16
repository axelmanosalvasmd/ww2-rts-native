//! Training requests only. Placement and live Match publication are separate checks.
use ww2_core::{
    catalog::{Catalog, Value},
    economy::{self, Mode, TechState},
    economy_state::{ArmySize, EconomyState, ModeContext, PlayerEconomy},
    production::{Context, JobStatus, ProductionState, UnitView},
};

fn context<'a>(units: &'a [UnitView<'a>], sudden_death: bool) -> Context<'a> {
    Context {
        mode: ModeContext {
            kind: Mode::Classic,
            ..Default::default()
        },
        army: ArmySize::Standard,
        tech_enabled: true,
        naval: true,
        sudden_death,
        units,
        drops: &[],
        world: None,
    }
}

fn state() -> (ProductionState, EconomyState) {
    (
        ProductionState::new(vec![TechState::new(3, 0, 0, 0).unwrap()]).unwrap(),
        EconomyState {
            players: vec![PlayerEconomy {
                mp: 10000.,
                fuel: 1000.,
                ..Default::default()
            }],
        },
    )
}

fn building(kind: &str) -> UnitView<'_> {
    UnitView {
        id: 1,
        owner: Some(0),
        kind,
        hp: 100.,
        built: 1.,
    }
}

#[test]
fn base21_all_train_times_complete_only_the_head_and_emit_the_requested_kind() {
    // Pinned shared/sim.js:350-355,381,1065-1075.
    let cases = [
        ("hq", "engineer", 12.),
        ("hq", "rifle", 15.),
        ("barracks", "conscript", 12.),
        ("barracks", "mg", 18.),
        ("barracks", "flak", 20.),
        ("barracks", "mortar", 20.),
        ("barracks", "sniper", 20.),
        ("barracks", "medic", 15.),
        ("barracks", "ranger", 20.),
        ("motorpool", "at", 22.),
        ("motorpool", "halftrack", 22.),
        ("motorpool", "lorry", 15.),
        ("shipyard", "lcvp", 20.),
        ("shipyard", "gunboat", 25.),
        ("shipyard", "destroyer", 75.),
        ("motorpool", "armoredcar", 25.),
        ("motorpool", "flaktrack", 30.),
        ("airfield", "fighter", 30.),
        ("airfield", "attacker", 35.),
        ("airfield", "bomber", 45.),
        ("motorpool", "rocket", 30.),
        ("motorpool", "tank", 35.),
        ("motorpool", "medium", 40.),
        ("motorpool", "tankdestroyer", 40.),
        ("motorpool", "tiger", 50.),
        ("motorpool", "churchill", 50.),
        ("barracks", "commando", 22.),
        ("motorpool", "howitzer", 30.),
        ("barracks", "flamer", 20.),
    ];
    for (maker, kind, seconds) in cases {
        let catalog = Catalog::compiled().unwrap();
        assert_eq!(
            catalog
                .unit(kind)
                .unwrap()
                .get("train")
                .and_then(Value::number),
            Some(seconds),
            "{kind}"
        );
        assert!(
            catalog
                .unit(maker)
                .unwrap()
                .get("makes")
                .unwrap()
                .array()
                .unwrap()
                .iter()
                .any(|value| value.string() == Some(kind))
        );
        let (mut production, wallets) = state();
        let charge = economy::price_of(Mode::Classic, kind).unwrap();
        let first = production.record_paid(1, kind, charge).unwrap();
        let second = production.record_paid(1, kind, charge).unwrap();
        let units = [building(maker)];
        let c = context(&units, false);
        assert!(
            production
                .advance(&c, &wallets, seconds - 0.25)
                .unwrap()
                .units
                .is_empty()
        );
        let rows = production.building(1).unwrap().rows();
        assert_eq!(rows[0].status, JobStatus::Active);
        assert_eq!(rows[1].status, JobStatus::Waiting);
        let out = production.advance(&c, &wallets, 0.5).unwrap().units;
        assert_eq!(out.len(), 1, "{kind}");
        assert_eq!(
            (out[0].building, out[0].owner, out[0].job_id),
            (1, 0, first)
        );
        assert_eq!(out[0].kind, kind);
        let b = production.building(1).unwrap();
        assert_eq!(b.progress(), 0.);
        assert_eq!(b.rows()[0].status, JobStatus::Active);
        assert_eq!(b.jobs()[0].id, second);
        assert!(
            production
                .advance(&c, &wallets, seconds - 0.25)
                .unwrap()
                .units
                .is_empty()
        );
        let out = production.advance(&c, &wallets, 0.25).unwrap().units;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].job_id, second);
        assert_eq!(out[0].kind, kind);
        assert!(production.building(1).unwrap().jobs().is_empty());
        assert_eq!(production.building(1).unwrap().progress(), 0.);
    }
}

#[test]
fn base21_unfinished_and_sudden_death_freeze_training_and_transition_clears_without_refund() {
    let (mut production, wallets) = state();
    let charge = economy::price_of(Mode::Classic, "rifle").unwrap();
    production.record_paid(1, "rifle", charge).unwrap();
    production.record_paid(1, "rifle", charge).unwrap();
    let mut units = [building("hq")];
    assert!(
        production
            .advance(&context(&units, false), &wallets, 4.)
            .unwrap()
            .units
            .is_empty()
    );
    let before = production.building(1).unwrap().clone();
    units[0].built = 0.5;
    assert!(
        production
            .advance(&context(&units, false), &wallets, 100.)
            .unwrap()
            .units
            .is_empty()
    );
    assert_eq!(production.building(1).unwrap(), &before);
    units[0].built = 1.;
    assert!(
        production
            .advance(&context(&units, true), &wallets, 100.)
            .unwrap()
            .units
            .is_empty()
    );
    assert_eq!(production.building(1).unwrap(), &before);
    let balance = (wallets.players[0].mp, wallets.players[0].fuel);
    production.clear_queues();
    assert!(production.building(1).unwrap().jobs().is_empty());
    assert_eq!(production.building(1).unwrap().progress(), 0.);
    assert!(
        production
            .advance(&context(&units, false), &wallets, 100.)
            .unwrap()
            .units
            .is_empty()
    );
    assert_eq!((wallets.players[0].mp, wallets.players[0].fuel), balance);
}
