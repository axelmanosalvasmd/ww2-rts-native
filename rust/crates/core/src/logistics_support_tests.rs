use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::UnitControl,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    supply_convoys::{Store, StoreId},
    unit_spawn::initial_unit,
    weather::WeatherChoice,
};
use serde_json::json;

fn scene() -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name": "Support provisions", "w": 60, "h": 60,
            "rows": vec![".".repeat(60); 60],
            "heights": vec!["0".repeat(60); 60],
            "spawns": [{"x": 4, "y": 4}, {"x": 55, "y": 55}],
            "points": [{"x": 30, "y": 30}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut match_state = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("p{slot}"),
                    team: slot,
                    faction: Faction::try_from(slot as u8).unwrap(),
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
            logistics: true,
            sandbox: false,
        },
    )
    .unwrap();
    match_state.start.economy.players[0].mp = 5_000.0;
    match_state
}

fn owned(match_state: &Match, kind: &str) -> u32 {
    match_state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && match_state.rules.kinds[u.kind].name == kind)
        .unwrap()
        .id
}

fn add_unit(match_state: &mut Match, kind: &str, at: Point) -> u32 {
    let id = match_state.next_unit_id;
    let unit = initial_unit(
        &match_state.rules,
        Catalog::compiled().unwrap(),
        match_state.rules.kind(kind).unwrap(),
        id,
        0,
        at,
    )
    .unwrap();
    let reserve = match_state.prepare_new_unit_reserves(&unit).unwrap();
    let control = UnitControl::new(&unit, &match_state.rules).unwrap();
    match_state.commands.units.insert(id, control);
    match_state.combat.initialize(id);
    match_state.firing.initialize(id);
    match_state.abilities.initialize(id);
    match_state.recovery.initialize(id);
    if let Some(reserve) = reserve {
        match_state.logistics.insert(id, reserve);
    }
    match_state.world.game.units.insert(id, unit);
    match_state
        .grid
        .update(id, &match_state.world.game.units[&id]);
    match_state.next_unit_id += 1;
    id
}

fn store(at: Point, source: bool, provisions: f64) -> Store {
    Store {
        id: StoreId::Cache(900),
        owner: 0,
        at,
        source,
        entity: None,
        capacity: crate::logistics::Stock::default(),
        buckets: vec![(
            0,
            crate::logistics::Stock {
                ammo: 0.0,
                provisions,
                fuel: 0.0,
            },
        )],
        donations: Vec::new(),
        active: true,
        support: false,
        point: None,
    }
}

#[test]
fn log11_support_pays_source_then_local_stock_then_own_reserve() {
    let mut match_state = scene();
    let rifle = owned(&match_state, "rifle");
    let at = match_state.world.game.units[&rifle].at();
    match_state.convoys.as_mut().unwrap().stores.clear();
    match_state.logistics.get_mut(rifle).unwrap().provisions = 10.0;

    match_state
        .convoys
        .as_mut()
        .unwrap()
        .stores
        .push(store(at, true, 0.0));
    assert!(match_state.spend_support_provisions(rifle, 0, 5.0).unwrap());
    assert_eq!(match_state.logistics.get(rifle).unwrap().provisions, 10.0);

    match_state.convoys.as_mut().unwrap().stores[0].source = false;
    match_state.convoys.as_mut().unwrap().stores[0].buckets[0]
        .1
        .provisions = 7.0;
    assert!(match_state.spend_support_provisions(rifle, 0, 5.0).unwrap());
    assert_eq!(
        match_state.convoys.as_ref().unwrap().stores[0].buckets[0]
            .1
            .provisions,
        2.0
    );
    assert_eq!(match_state.logistics.get(rifle).unwrap().provisions, 10.0);

    assert!(match_state.spend_support_provisions(rifle, 0, 5.0).unwrap());
    assert_eq!(match_state.logistics.get(rifle).unwrap().provisions, 5.0);
    assert!(!match_state.spend_support_provisions(rifle, 0, 6.0).unwrap());
    assert_eq!(match_state.logistics.get(rifle).unwrap().provisions, 5.0);
}

#[test]
fn log11_reinforcement_and_vehicle_repair_pay_from_actual_recovery() {
    let mut match_state = scene();
    match_state.convoys.as_mut().unwrap().stores.clear();
    let rifle = owned(&match_state, "rifle");
    let hq = owned(&match_state, "hq");
    let at = match_state.world.game.units[&hq].at();
    let vehicle = add_unit(&mut match_state, "armoredcar", at);
    let rifle_def = Catalog::compiled().unwrap().unit("rifle").unwrap();
    let hp_per = rifle_def.get("hpPer").unwrap().number().unwrap();
    let rifle_full = match_state.world.game.units[&rifle].hp;
    match_state.world.game.units.get_mut(&rifle).unwrap().x = at.x;
    match_state.world.game.units.get_mut(&rifle).unwrap().z = at.z;
    match_state.world.game.units.get_mut(&rifle).unwrap().hp -= hp_per;
    match_state.recovery.get_mut(rifle).unwrap().reinforcement = 10.0;
    match_state.logistics.get_mut(rifle).unwrap().provisions = 8.0;
    let vehicle_full = match_state.world.game.units[&vehicle].hp;
    match_state.world.game.units.get_mut(&vehicle).unwrap().hp -= 10.0;
    match_state.logistics.get_mut(vehicle).unwrap().provisions = 8.0;

    let ids = match_state
        .world
        .game
        .units
        .keys()
        .copied()
        .collect::<Vec<_>>();
    let frame = match_state.prepare_recovery(&ids).unwrap();
    match_state
        .reinforce(&frame, match_state.rules.movement.tick)
        .unwrap();
    assert_eq!(match_state.world.game.units[&rifle].hp, rifle_full);
    assert_eq!(match_state.logistics.get(rifle).unwrap().provisions, 3.0);
    let repaired = match_state.world.game.units[&vehicle].hp - (vehicle_full - 10.0);
    assert!(repaired > 0.0);
    // Measuring the HP delta introduces another subtraction and rounding.
    assert!((8.0 - match_state.logistics.get(vehicle).unwrap().provisions - repaired / 5.0).abs() < 1e-12);
}

#[test]
fn log11_medic_healing_spends_medics_own_provisions() {
    let mut match_state = scene();
    let rifle = owned(&match_state, "rifle");
    let at = match_state.world.game.units[&rifle].at();
    let medic = add_unit(&mut match_state, "medic", at);
    let before = match_state.world.game.units[&rifle].hp - 10.0;
    match_state.world.game.units.get_mut(&rifle).unwrap().hp = before;
    match_state.logistics.get_mut(medic).unwrap().provisions = 1.0;
    match_state.world.game.tick = 10;
    let ids = match_state
        .world
        .game
        .units
        .keys()
        .copied()
        .collect::<Vec<_>>();
    match_state.heal_wounded(&ids, 0.5).unwrap();
    let healed = match_state.world.game.units[&rifle].hp - before;
    assert!(healed > 0.0);
    assert_eq!(
        1.0 - match_state.logistics.get(medic).unwrap().provisions,
        healed / 5.0
    );
}

#[test]
fn log11_engineer_repair_spends_engineers_own_provisions() {
    let mut match_state = scene();
    let hq = owned(&match_state, "hq");
    let at = match_state.world.game.units[&hq].at();
    let engineer = add_unit(&mut match_state, "engineer", at);
    match_state.logistics.get_mut(engineer).unwrap().provisions = 1.0;
    match_state.world.game.units.get_mut(&hq).unwrap().hp -= 10.0;
    let section = match_state
        .world
        .structures
        .iter()
        .find(|s| s.entity_id == Some(hq))
        .unwrap()
        .section_ids[0]
        .clone();
    match_state.world.section_by_id_mut(&section).unwrap().1.hp -= 10.0;
    match_state
        .world
        .game
        .units
        .get_mut(&engineer)
        .unwrap()
        .build = true;
    match_state
        .commands
        .units
        .get_mut(&engineer)
        .unwrap()
        .build_target = Some(hq);

    let before = match_state.world.game.units[&hq].hp;
    let mut frame = match_state.begin_construction_tick().unwrap();
    match_state
        .step_construction_worker(engineer, &mut frame)
        .unwrap();
    match_state.finish_construction_tick(frame).unwrap();
    let repaired = match_state.world.game.units[&hq].hp - before;
    assert!(repaired > 0.0);
    // Measuring the HP delta introduces another subtraction and rounding.
    assert!((1.0 - match_state.logistics.get(engineer).unwrap().provisions - repaired / 5.0).abs() < 1e-12);
}
