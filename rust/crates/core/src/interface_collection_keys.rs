use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    environment::{Context as EnvironmentContext, DamageContext},
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    recipient_snapshot::DeliveryState,
    weather::WeatherChoice,
};
use serde_json::json;

fn scene() -> Match {
    scene_with_house(false)
}

fn scene_with_house(house: bool) -> Match {
    let mut rows = vec![".".repeat(60); 60];
    if house {
        rows[25].replace_range(25..26, "B");
    }
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Collection ordering", "w":60,"h":60,
            "rows":rows,
            "spawns":[{"x":4,"y":4},{"x":55,"y":55}],
            "points":[{"x":30,"y":30}]
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Default,
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
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}

#[test]
fn if22_hospital_placement_order_survives_transactions_recovery_ticks_and_snapshots() {
    let mut m = scene();
    let high = 31 * 60 + 31;
    let low = 29 * 60 + 29;
    for cell in [high, low] {
        m.world.set_cell(cell, b'A').unwrap();
    }
    m.world.aid.insert(high, 0);
    m.world.aid.insert(low, 1);
    m.world.aid.insert(high, 1);
    let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let frame = m.prepare_recovery(&ids).unwrap();
    assert_eq!(
        frame.tents,
        vec![
            (1, Point { x: 63.0, z: 63.0 }),
            (1, Point { x: 59.0, z: 59.0 })
        ]
    );

    // Removing terrain retires its hospital; rebuilding appends it after survivors.
    m.world.set_cell(high, b'.').unwrap();
    m.world.set_cell(high, b'A').unwrap();
    m.world.aid.insert(high, 0);
    let mut draft = m.world.begin_transaction().unwrap();
    draft.world.aid.insert(low, 0);
    draft.prepare_publication(&mut m.world).unwrap().commit();
    let frame = m.prepare_recovery(&ids).unwrap();
    assert_eq!(
        frame.tents,
        vec![
            (0, Point { x: 59.0, z: 59.0 }),
            (0, Point { x: 63.0, z: 63.0 })
        ]
    );

    let id = ids
        .iter()
        .copied()
        .find(|id| {
            let u = &m.world.game.units[id];
            u.owner == 0 && m.rules.kinds[u.kind].name == "rifle"
        })
        .unwrap();
    let unit = m.world.game.units.get_mut(&id).unwrap();
    unit.x = 61.0;
    unit.z = 61.0;
    unit.path.clear();
    unit.hp = 30.0;
    unit.hold_pos = true;
    m.grid.rebuild(&m.world.game.units);
    m.recovery.get_mut(id).unwrap().reinforcement = 10.0;
    m.start.economy.players[0].mp = 1000.0;
    let frame = m.prepare_recovery(&ids).unwrap();
    m.reinforce(&frame, 0.05).unwrap();
    assert!(m.world.game.units[&id].hp > 30.0);

    // A short VP goal runs the ordinary finish phase while hospitals remain live.
    m.start.win_vp = 1.0;
    m.start.points[0].owner = Some(0);
    m.start.points[0].vp = 1.0;
    let mut delivery = DeliveryState::new(&m).unwrap();
    let mut ending_ticks = 0;
    for _ in 0..80 {
        m.advance_tick().unwrap();
        assert_eq!(m.tick_state.fault(), None);
        let frame = delivery
            .capture_with_lists(&mut m, &[true; 2], &[false; 2], &[])
            .unwrap();
        for slot in 0..2 {
            assert!(frame.recipient(slot).unwrap().into_json().is_object());
        }
        if m.finish.is_some() {
            ending_ticks += 1;
            if ending_ticks == 4 {
                break;
            }
        }
    }
    assert_eq!(
        ending_ticks, 4,
        "the ordinary match reaches its ending hold"
    );
    assert_eq!(m.finish.unwrap().winner, 0);
    assert_eq!(
        m.world
            .aid
            .iter()
            .map(|(&c, &owner)| (c, owner))
            .collect::<Vec<_>>(),
        vec![(low, 0), (high, 0)]
    );
}

#[test]
fn if22_fire_spread_uses_insertion_order_and_defers_new_fires_until_next_pass() {
    let mut m = scene();
    let centers = [40 * 60 + 40, 15 * 60 + 15];
    let mut neighbors = Vec::new();
    for c in centers {
        let mut around = Vec::new();
        for dy in -1..=1 {
            for dx in -1..=1 {
                let cell = (c as i64 + dy * 60 + dx) as usize;
                m.world.set_cell(cell, b'H').unwrap();
                if dx != 0 || dy != 0 {
                    around.push(cell);
                }
            }
        }
        neighbors.push(around);
    }
    m.world.fires.insert(centers[0], 1.0);
    m.world.fires.insert(centers[1], 30.0);
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let mut context = EnvironmentContext {
        tick: 0,
        seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &order,
        grid: &m.grid,
        rain: 0.0,
        wet: 0.0,
        wind_angle: 0.0,
        wind_strength: 0.0,
    };
    m.environment.burn(&mut m.world, 5.0, &mut context).unwrap();
    let expected = std::iter::once((centers[1], 25.0))
        .chain(neighbors.into_iter().flatten().map(|cell| (cell, 14.0)))
        .collect::<Vec<_>>();
    assert_eq!(
        m.world
            .fires
            .iter()
            .map(|(&c, &left)| (c, left))
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn if22_flood_work_keeps_first_insertion_order_without_duplicate_neighbors() {
    let mut m = scene();
    m.world.take_soak();
    for c in [1000, 300, 1000] {
        m.world.set_level(c, 1).unwrap();
    }
    let expected = vec![1000, 999, 1001, 940, 1060, 300, 299, 301, 240, 360];
    assert_eq!(m.world.take_soak(), expected);
    assert!(m.world.soak().is_empty());
    m.world.set_level(1000, 0).unwrap();
    assert_eq!(m.world.soak(), &[1000, 999, 1001, 940, 1060]);
}

#[test]
fn if22_world_training_visits_newborn_in_the_same_live_unit_pass() {
    let regions = (0..64)
        .map(|index| {
            let x = index % 8 * 64;
            let y = index / 8 * 64;
            let mut region = json!({"id":index,"x":x+32,"y":y+32,"bounds":[x,y,x+64,y+64]});
            if index == 0 {
                region["home"] = json!(0);
            }
            if index == 63 {
                region["home"] = json!(1);
            }
            region
        })
        .collect::<Vec<_>>();
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Live training order", "w":512,"h":512,"rows":vec![".".repeat(512);512],
            "spawns":[{"x":32,"y":32},{"x":480,"y":480}],"points":[],
            "world":{"size":"huge","total":64,"regionCells":64,"regions":regions}
        }))
        .unwrap(),
    )
    .unwrap();
    let mut options = scene().options;
    options.mode = Mode::World;
    let mut m = Match::new_world(&map, options, crate::random::MatchRandom::new(701)).unwrap();
    let maker = m
        .world
        .game
        .units
        .iter()
        .find_map(|(&id, unit)| {
            (unit.owner == 0 && m.rules.kinds[unit.kind].name == "hq").then_some(id)
        })
        .unwrap();
    m.start.economy.players[0].mp = 10000.0;
    assert_eq!(
        m.execute_command(
            0,
            &crate::match_commands::Command::Buy {
                kind: "engineer".into(),
                from: Some(maker),
            },
            false
        )
        .unwrap(),
        None
    );
    let duration = Catalog::compiled()
        .unwrap()
        .unit("engineer")
        .unwrap()
        .get("train")
        .unwrap()
        .number()
        .unwrap();
    assert!(m.step_training(maker, duration - 0.025).unwrap().is_none());
    let newborn = m.next_unit_id;
    let first = m.advance_tick().unwrap();
    assert!(!first.prior.contains_key(&newborn));
    assert!(first.unit_order.contains(&newborn));
    assert!(first.unit_order.windows(2).all(|ids| ids[0] < ids[1]));
    assert_eq!(
        m.world.game.units[&newborn].sprint.to_bits(),
        (-0.05_f64).to_bits()
    );
    assert_eq!(m.next_unit_id, newborn + 1);
    assert!(m.production.building(maker).unwrap().jobs().is_empty());
    m.advance_tick().unwrap();
    assert_eq!(
        m.world.game.units[&newborn].sprint.to_bits(),
        (-0.1_f64).to_bits()
    );
    assert_eq!(m.tick_state.fault(), None);
}

#[test]
fn if22_structure_contacts_deduplicate_and_evict_the_oldest_inserted_key() {
    let mut m = scene_with_house(true);
    let cell = 25 * 60 + 25;
    let section = m.world.sections[&cell].id.clone();
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let mut context = EnvironmentContext {
        tick: 0,
        seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &order,
        grid: &m.grid,
        rain: 0.0,
        wet: 0.0,
        wind_angle: 0.0,
        wind_strength: 0.0,
    };
    let skip = BTreeSet::new();
    let mut hit = DamageContext::plain(&skip);
    for index in 0..2048 {
        let key = format!("contact:{index}");
        let mut hit = DamageContext::plain(&skip);
        hit.contact_id = Some(&key);
        assert_eq!(
            m.environment
                .damage_section(&mut m.world, cell, 0.001, &mut context, &hit)
                .unwrap()
                .0,
            0.001
        );
    }
    assert_eq!(m.environment.contacts().len(), 2048);
    hit.contact_id = Some("contact:0");
    assert_eq!(
        m.environment
            .damage_section(&mut m.world, cell, 0.001, &mut context, &hit)
            .unwrap()
            .0,
        0.0
    );
    hit.contact_id = Some("contact:2048");
    assert_eq!(
        m.environment
            .damage_section(&mut m.world, cell, 0.001, &mut context, &hit)
            .unwrap()
            .0,
        0.001
    );
    assert_eq!(m.environment.contacts()[0], format!("contact:1:{section}"));
    hit.contact_id = Some("contact:0");
    assert_eq!(
        m.environment
            .damage_section(&mut m.world, cell, 0.001, &mut context, &hit)
            .unwrap()
            .0,
        0.001
    );
    assert_eq!(m.environment.contacts()[0], format!("contact:2:{section}"));
    assert_eq!(
        m.environment.contacts().last().unwrap(),
        &format!("contact:0:{section}")
    );
}
