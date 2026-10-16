//! Recruitment and Classic training through canonical Match publication.
//! Pinned prototype: shared/sim.js:350-390, 1065-1097, 3793-3797, 5352.
//! Hosted aviation remains held on its event delivery adapter.
use crate::{
    catalog::{Catalog, Value},
    economy::{self, Faction, Mode},
    economy_state::ArmySize,
    game::{PlaneState, Point},
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig, RecruitError},
    production::{Denial, Error},
    recipient_snapshot::SnapshotCache,
    weather::WeatherChoice,
};
use serde_json::json;

fn scene(mode: Mode, faction: Faction) -> Match {
    let rows = vec![format!("{}{}{}", ".".repeat(60), "W".repeat(12), ".".repeat(56)); 128];
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Production lifecycle", "w":128, "h":128, "naval":true,
            "rows":rows,
            "spawns":[{"x":8,"y":8},{"x":112,"y":112},{"x":8,"y":112}],
            "defend":[0],
            "points":[{"x":100,"y":64,"mp":0,"vp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    let options = MatchOptions {
        seed: 701,
        mode,
        players: (0..2)
            .map(|slot| PlayerConfig {
                name: format!("P{slot}"),
                team: slot,
                faction,
            })
            .collect(),
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: Some(217),
        living_weather: false,
        map_key: None,
        tech: false,
        supply: false,
        logistics: false,
        sandbox: false,
    };
    let mut state = match mode {
        Mode::Default | Mode::Conquest => Match::new(&map, options),
        Mode::Assault => Match::new_assault(&map, options, 0),
        Mode::Annihilation => Match::new_annihilation(&map, options),
        Mode::Classic => Match::new_classic(&map, options),
        Mode::Horde => Match::new_horde(&map, options),
        _ => unreachable!(),
    }
    .unwrap();
    // Initial resources are supplied scene facts. Every birth still uses the real wallet owner.
    for wallet in &mut state.start.economy.players {
        wallet.mp = 10000.;
        wallet.fuel = 5000.;
    }
    state
}

fn own(state: &Match, owner: usize, kind: &str) -> u32 {
    state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == owner && state.rules.kinds[u.kind].name == kind)
        .unwrap()
        .id
}

fn facility(state: &mut Match, owner: usize, kind: &str, x: usize, y: usize) -> u32 {
    let id = state.next_unit_id;
    state
        .place_starting_building(owner, kind, y * state.world.game.w + x)
        .unwrap();
    let unit = &state.world.game.units[&id];
    assert_eq!(unit.cells.as_ref().unwrap().len(), 9);
    assert!(
        unit.cells
            .as_ref()
            .unwrap()
            .iter()
            .all(|&cell| { state.building_cells[&(cell as usize)].unit == id })
    );
    id
}

fn all_seat_snapshots(state: &Match) {
    let cache = SnapshotCache::new(state).unwrap();
    for slot in 0..state.world.game.players.len() {
        cache.recipient(slot).unwrap();
    }
}

fn ordinary_ticks(state: &mut Match, count: usize) {
    for _ in 0..count {
        state.advance_tick().unwrap();
        assert!(state.tick_state.fault().is_none());
        all_seat_snapshots(state);
    }
}

#[test]
fn base18_standalone_aircraft_purchase_parks_at_maker_and_survives_recipient_ticks() {
    for mode in [Mode::Default, Mode::Conquest] {
        let mut state = scene(mode, Faction::Usa);
        let airfield = facility(&mut state, 0, "airfield", 20, 20);
        for (name, ammo) in [("fighter", 60.), ("attacker", 8.), ("bomber", 2.)] {
            let wallet = state.start.economy.players[0].clone();
            let spent = state.story[0].mp_spent;
            let id = state.next_unit_id;
            let command = crate::match_commands::Command::from_json(
                &json!({"t":"buy","unit":name,"from":airfield}),
            )
            .unwrap();
            assert_eq!(state.execute_command(0, &command, false).unwrap(), None);
            let unit = &state.world.game.units[&id];
            assert_eq!(state.rules.kinds[unit.kind].name, name);
            assert_eq!(unit.air, Some(PlaneState::Base));
            assert_eq!(unit.at(), state.world.game.units[&airfield].at());
            assert!(unit.path.is_empty());
            assert!(unit.world_goal.is_none());
            assert!(!unit.drift);
            let aircraft = state.aviation.planes[&id];
            assert_eq!(
                (aircraft.fuel, aircraft.ammo, aircraft.timer),
                (50., ammo, 0.)
            );
            assert!(aircraft.mission.is_none());
            let price = economy::price_of(mode, name).unwrap().mp;
            assert_eq!(state.start.economy.players[0].mp, wallet.mp - price);
            assert_eq!(state.start.economy.players[0].fuel, wallet.fuel);
            assert_eq!(state.story[0].mp_spent, spent + price);
            assert!(
                state
                    .production
                    .building(airfield)
                    .unwrap()
                    .jobs()
                    .is_empty()
            );
            let cache = SnapshotCache::new(&state).unwrap();
            let owner = cache.recipient(0).unwrap();
            assert!(
                owner.fields["air"]
                    .as_array()
                    .unwrap()
                    .contains(&json!([id, 0, 50., ammo, 0.]))
            );
            assert!(
                !cache.recipient(1).unwrap().fields["air"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row[0] == id)
            );
            ordinary_ticks(&mut state, 4);
        }
    }
}

#[test]
fn base18_four_skirmish_modes_require_finished_live_accessible_makers() {
    for mode in [
        Mode::Default,
        Mode::Conquest,
        Mode::Assault,
        Mode::Annihilation,
    ] {
        let mut state = scene(mode, Faction::Usa);
        let barracks = own(&state, 0, "barracks");
        let first = state.world.game.units[&barracks].clone();
        let before = (
            state.next_unit_id,
            state.start.economy.clone(),
            state.production.clone(),
        );
        for (built, hp) in [(0.5, first.hp), (1., 0.)] {
            let maker = state.world.game.units.get_mut(&barracks).unwrap();
            maker.built = Some(built);
            maker.hp = hp;
            assert_eq!(
                state.recorded_buy(0, "mg", Some(barracks)).unwrap_err(),
                RecruitError::Admission(Error::Denied(Denial::Needs))
            );
            assert_eq!(
                (
                    state.next_unit_id,
                    state.start.economy.clone(),
                    state.production.clone()
                ),
                before
            );
        }
        *state.world.game.units.get_mut(&barracks).unwrap() = first;
        let second = facility(&mut state, 0, "barracks", 20, 20);
        let destination = Point { x: 70., z: 50. };
        state.building_rallies.insert(second, Some(destination));
        let bought = state.recorded_buy(0, "rifle", Some(second)).unwrap();
        assert_eq!(bought.purchase.request.building, second);
        let unit = &state.world.game.units[&bought.unit];
        assert_eq!(unit.world_goal, Some(destination));
        assert!(unit.drift);
        assert!(unit.ghost > state.world.game.tick as f64);
        assert_eq!(unit.owner, 0);
        assert!(matches!(
            state.recorded_buy(0, "mg", Some(u32::MAX)),
            Err(RecruitError::Admission(Error::Denied(Denial::Needs)))
        ));
        let fallback = state.recorded_buy(0, "mg", None).unwrap();
        assert_eq!(fallback.purchase.request.building, barracks);
    }
}

#[test]
fn base18_horde_shared_airfield_publishes_the_buyers_plane_and_charges_only_the_buyer() {
    let mut state = scene(Mode::Horde, Faction::Usa);
    let airfield = facility(&mut state, 0, "airfield", 28, 20);
    let provider_wallet = state.start.economy.players[0].clone();
    let buyer_mp = state.start.economy.players[1].mp;
    let bought = state.recorded_buy(1, "fighter", Some(airfield)).unwrap();
    assert_eq!(bought.purchase.request.building, airfield);
    assert_eq!(state.world.game.units[&bought.unit].owner, 1);
    assert_eq!(
        state.world.game.units[&bought.unit].at(),
        state.world.game.units[&airfield].at()
    );
    assert_eq!(state.start.economy.players[0], provider_wallet);
    assert_eq!(
        state.start.economy.players[1].mp,
        buyer_mp - economy::price_of(Mode::Horde, "fighter").unwrap().mp
    );
    assert!(state.aviation.planes.contains_key(&bought.unit));
    assert!(state.world.game.units[&bought.unit].world_goal.is_none());
    assert!(state.world.game.units[&bought.unit].path.is_empty());
    assert!(state.aviation.planes[&bought.unit].mission.is_none());
}

#[test]
fn base21_classic_training_publishes_all_catalog_kinds_at_the_live_building_turn() {
    let catalog = Catalog::compiled().unwrap();
    for name in [
        "engineer",
        "rifle",
        "conscript",
        "mg",
        "flak",
        "mortar",
        "sniper",
        "medic",
        "ranger",
        "at",
        "halftrack",
        "lorry",
        "lcvp",
        "gunboat",
        "destroyer",
        "armoredcar",
        "flaktrack",
        "fighter",
        "attacker",
        "bomber",
        "rocket",
        "tank",
        "medium",
        "tankdestroyer",
        "tiger",
        "churchill",
        "commando",
        "howitzer",
        "flamer",
    ] {
        let definition = catalog.unit(name).unwrap();
        let faction = definition
            .get("faction")
            .and_then(Value::number)
            .map_or(Faction::Usa, |value| {
                Faction::try_from(value as u8).unwrap()
            });
        let mut state = scene(Mode::Classic, faction);
        let maker = if ["engineer", "rifle"].contains(&name) {
            own(&state, 0, "hq")
        } else {
            let kind = if ["fighter", "attacker", "bomber"].contains(&name) {
                "airfield"
            } else if ["lcvp", "gunboat", "destroyer"].contains(&name) {
                "shipyard"
            } else if [
                "conscript",
                "mg",
                "flak",
                "mortar",
                "sniper",
                "medic",
                "ranger",
                "commando",
                "flamer",
            ]
            .contains(&name)
            {
                "barracks"
            } else {
                "motorpool"
            };
            facility(
                &mut state,
                0,
                kind,
                if kind == "shipyard" { 57 } else { 20 },
                20,
            )
        };
        let id = state.next_unit_id;
        let count = state.world.game.units.len();
        let price = economy::price_of(Mode::Classic, name).unwrap();
        let wallet = state.start.economy.players[0].clone();
        let job = state.recorded_world_buy(0, name, Some(maker)).unwrap();
        assert_eq!(
            state.world.game.units.len(),
            count,
            "{name} queues without a birth"
        );
        assert_eq!(state.next_unit_id, id);
        assert_eq!(state.start.economy.players[0].mp, wallet.mp - price.mp);
        assert_eq!(
            state.start.economy.players[0].fuel,
            wallet.fuel - price.fuel
        );
        let duration = definition.get("train").and_then(Value::number).unwrap();
        assert!(
            state
                .step_training(maker, duration - 0.125)
                .unwrap()
                .is_none()
        );
        assert_eq!(state.production.building(maker).unwrap().jobs()[0].id, job);
        all_seat_snapshots(&state);
        ordinary_ticks(&mut state, 2);
        assert!(
            !state.world.game.units.contains_key(&id),
            "{name} still training"
        );
        ordinary_ticks(&mut state, 1);
        assert_eq!(state.next_unit_id, id + 1, "{name} exactly one birth");
        let unit = &state.world.game.units[&id];
        assert_eq!(unit.owner, 0);
        assert_eq!(state.rules.kinds[unit.kind].name, name);
        assert!(state.production.building(maker).unwrap().jobs().is_empty());
        assert_eq!(state.production.building(maker).unwrap().progress(), 0.);
        if state.rules.kinds[unit.kind].air {
            assert_eq!(unit.air, Some(PlaneState::Base));
            assert_eq!(unit.at(), state.world.game.units[&maker].at());
            assert!(unit.world_goal.is_none());
            assert!(state.aviation.planes.contains_key(&id));
        } else {
            assert_ne!(unit.at(), state.world.game.units[&maker].at());
            assert!(unit.ghost > state.world.game.tick as f64);
        }
        ordinary_ticks(&mut state, 3);
    }
}

#[test]
fn base21_training_pauses_preserves_fifo_and_rolls_back_failed_aircraft_publication() {
    let mut state = scene(Mode::Classic, Faction::Usa);
    let hq = own(&state, 0, "hq");
    let first = state.recorded_world_buy(0, "rifle", Some(hq)).unwrap();
    let second = state.recorded_world_buy(0, "engineer", Some(hq)).unwrap();
    let paid = state.start.economy.players[0].clone();
    state.world.game.units.get_mut(&hq).unwrap().built = Some(0.5);
    assert!(state.step_training(hq, 30.).unwrap().is_none());
    state.world.game.units.get_mut(&hq).unwrap().built = Some(1.);
    let hp = state.world.game.units[&hq].hp;
    state.world.game.units.get_mut(&hq).unwrap().hp = 0.;
    assert!(state.step_training(hq, 30.).unwrap().is_none());
    state.world.game.units.get_mut(&hq).unwrap().hp = hp;
    state.elimination.as_mut().unwrap().sudden_death = true;
    assert!(state.step_training(hq, 30.).unwrap().is_none());
    state.elimination.as_mut().unwrap().sudden_death = false;
    assert_eq!(state.production.building(hq).unwrap().progress(), 0.);
    let rally = Point { x: 70., z: 50. };
    state.building_rallies.insert(hq, Some(rally));
    let born = state.step_training(hq, 100.).unwrap().unwrap();
    assert_eq!(born.completion.job_id, first);
    assert_eq!(state.world.game.units[&born.unit].world_goal, Some(rally));
    assert!(state.world.game.units[&born.unit].drift);
    assert_eq!(state.production.building(hq).unwrap().progress(), 0.);
    assert_eq!(state.production.building(hq).unwrap().jobs()[0].id, second);
    assert_eq!(
        state.start.economy.players[0], paid,
        "completion never charges twice"
    );
    ordinary_ticks(&mut state, 3);

    let airfield = facility(&mut state, 0, "airfield", 20, 20);
    state
        .recorded_world_buy(0, "fighter", Some(airfield))
        .unwrap();
    let next = state.next_unit_id;
    let queued = state.production.clone();
    let paid = state.start.economy.clone();
    state.next_unit_id = hq;
    assert!(state.step_training(airfield, 30.).is_err());
    assert_eq!(state.production, queued);
    assert_eq!(state.start.economy, paid);
    state.next_unit_id = next;
    let plane = state.step_training(airfield, 30.).unwrap().unwrap();
    assert_eq!(plane.unit, next);
    ordinary_ticks(&mut state, 4);
}

#[test]
fn aircraft_purchase_and_training_keep_bound_room_delivery_guard_without_payment() {
    for mode in [Mode::Conquest, Mode::Classic] {
        let mut state = scene(mode, Faction::Usa);
        let airfield = facility(&mut state, 0, "airfield", 20, 20);
        state.restrict_to_ordinary_snapshots().unwrap();
        let before = (
            state.start.economy.clone(),
            state.production.clone(),
            state.story.clone(),
            state.next_unit_id,
            state.world.game.units.len(),
        );
        if mode == Mode::Classic {
            assert_eq!(
                state
                    .recorded_world_buy(0, "fighter", Some(airfield))
                    .unwrap_err(),
                crate::match_runtime::ProductionStepError::Placement(
                    crate::produced_units::Error::UnsupportedAir
                )
            );
        } else {
            assert_eq!(
                state
                    .recorded_buy(0, "fighter", Some(airfield))
                    .unwrap_err(),
                RecruitError::Placement(crate::produced_units::Error::UnsupportedAir)
            );
        }
        assert_eq!(
            (
                state.start.economy.clone(),
                state.production.clone(),
                state.story.clone(),
                state.next_unit_id,
                state.world.game.units.len()
            ),
            before
        );
        ordinary_ticks(&mut state, 4);
    }
}

#[test]
fn base21_logistics_training_stages_full_vehicle_reserves_before_fifo_publication() {
    let mut state = scene(Mode::Classic, Faction::Usa);
    // The dedicated server test exercises constructor initialization and full ticks.
    state.options.logistics = true;
    state.initialize_logistics().unwrap();
    let maker = facility(&mut state, 0, "motorpool", 20, 20);
    for _ in 0..2 {
        state
            .recorded_world_buy(0, "halftrack", Some(maker))
            .unwrap();
    }
    let jobs = state.production.building(maker).unwrap().jobs().to_vec();
    assert!(state.step_training(maker, 21.95).unwrap().is_none());
    let id = state.next_unit_id;
    let birth = state.step_training(maker, 0.05).unwrap().unwrap();
    assert_eq!(birth.unit, id);
    let u = &state.world.game.units[&id];
    assert_eq!(state.rules.kinds[u.kind].name, "halftrack");
    let reserves = state.logistics.get(id).unwrap();
    // Prototype logistics.js:4,46-52 and sim.js:251-253.
    assert_eq!(reserves.ammo, reserves.ammo_max);
    assert_eq!(reserves.ammo_max, 225.);
    assert_eq!(reserves.provisions, 120.);
    assert_eq!(reserves.fuel, Some(180.));
    assert_eq!(reserves.emergency, Some(60.));
    assert!(!reserves.forced && !reserves.stranded && !reserves.exhausted);
    let maker_state = state.production.building(maker).unwrap();
    assert_eq!(maker_state.jobs(), &jobs[1..]);
    assert_eq!(maker_state.progress(), 0.);
    let second = state.step_training(maker, 22.).unwrap().unwrap().unit;
    assert!(state.logistics.get(second).is_some());
    assert!(state.production.building(maker).unwrap().jobs().is_empty());
    for c in state.commands.units.values_mut() {
        c.hold_fire = true;
        c.auto_cast = false;
    }
    ordinary_ticks(&mut state, 4);
}
