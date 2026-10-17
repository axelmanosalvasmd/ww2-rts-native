use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, Denial},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::{DeliveryState, SelectedSeat, SnapshotCache},
    weather::WeatherChoice,
};
use serde_json::json;

fn scene(horde: bool) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"BASE-24 shared facilities", "w":60,"h":60,
            "rows":vec![".".repeat(60);60],
            "spawns":[{"x":30,"y":5},{"x":10,"y":54},{"x":50,"y":54}],
            "defend":[0], "points": if horde {
                vec![json!({"x":30,"y":30})]
            } else {
                vec![json!({"x":30,"y":5,"vp":5}); 9]
            }
        }))
        .unwrap(),
    )
    .unwrap();
    let options = MatchOptions {
        seed: 701,
        mode: if horde { Mode::Horde } else { Mode::Conquest },
        players: (0..2)
            .map(|slot| PlayerConfig {
                name: format!("P{slot}"),
                team: slot,
                faction: if slot == 0 {
                    Faction::Usa
                } else {
                    Faction::Germany
                },
            })
            .collect(),
        shuffle: false,
        army: ArmySize::Large,
        weather: WeatherChoice::Map,
        weather_seed: Some(1),
        living_weather: false,
        map_key: None,
        tech: false,
        supply: false,
        logistics: false,
        sandbox: false,
    };
    let mut m = if horde {
        Match::new_horde(&map, options)
    } else {
        Match::new(&map, options)
    }
    .unwrap();
    m.world.game.reveal = true;
    m.start.economy.players[0].mp = 5000.0;
    m.start.economy.players[1].mp = 5000.0;
    m
}

fn building(m: &Match, name: &str) -> u32 {
    m.world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == name)
        .unwrap()
        .id
}

fn command(m: &mut Match, owner: usize, command: Command) -> Option<Denial> {
    m.execute_command(owner, &command, false).unwrap()
}

#[test]
fn base24_shared_rally_drives_buyer_owned_paid_recruitment() {
    let mut m = scene(true);
    let maker = building(&m, "barracks");
    let at = Point { x: 85.0, z: 32.0 };
    assert_eq!(
        command(
            &mut m,
            1,
            Command::Rally {
                ids: vec![],
                id: Some(i64::from(maker)),
                at: Some(at),
            }
        ),
        None
    );
    assert_eq!(m.building_rallies[&maker], Some(at));
    let wallets = m.start.economy.clone();
    let stories = m.story.clone();
    let id = m.next_unit_id;
    assert_eq!(
        command(
            &mut m,
            1,
            Command::Buy {
                kind: "rifle".into(),
                from: Some(maker),
            }
        ),
        None
    );
    let born = &m.world.game.units[&id];
    assert_eq!(born.owner, 1);
    assert_eq!(m.world.game.units[&maker].owner, 0);
    assert_eq!(m.recruit_metadata[&id].building, maker);
    assert_eq!(born.world_goal, Some(at));
    assert_eq!(born.amove, None);
    assert!(!born.path.is_empty());
    assert!(born.drift && m.commands.units[&id].rally_drift);
    assert_eq!(m.start.economy.players[0], wallets.players[0]);
    assert_eq!(m.start.economy.players[2], wallets.players[2]);
    assert_eq!(m.start.economy.players[1].mp, wallets.players[1].mp - 100.0);
    assert_eq!(m.story[0], stories[0]);
    assert_eq!(m.story[1].mp_spent, stories[1].mp_spent + 100.0);
    assert_eq!(m.story[2], stories[2]);
    m.start.economy.players[1].mp = 0.0;
    let wallets = m.start.economy.clone();
    let stories = m.story.clone();
    let next = m.next_unit_id;
    assert_eq!(
        command(
            &mut m,
            1,
            Command::Buy {
                kind: "rifle".into(),
                from: Some(maker),
            }
        ),
        Some(Denial::Manpower)
    );
    assert_eq!(
        m.start.economy, wallets,
        "the producer owner cannot pay for its ally"
    );
    assert_eq!(m.story, stories);
    assert_eq!(m.next_unit_id, next);
    assert_eq!(
        command(
            &mut m,
            2,
            Command::Rally {
                ids: vec![i64::from(maker)],
                id: None,
                at: Some(Point { x: 8.0, z: 8.0 }),
            }
        ),
        Some(Denial::Unseen)
    );
    assert_eq!(m.building_rallies[&maker], Some(at));
    let cache = SnapshotCache::new(&m).unwrap();
    for slot in 0..m.world.game.players.len() {
        assert_eq!(
            cache.recipient(slot).unwrap().fields["mode"]["kind"],
            "horde"
        );
    }
    drop(cache);
    let mut delivery = DeliveryState::new(&m).unwrap();
    let selected = (0..m.world.game.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let report = m.advance_tick().unwrap();
    let frame = delivery
        .capture_selected_with_tick_events(&mut m, &selected, &report.events)
        .unwrap()
        .unwrap();
    for seat in &selected {
        let snapshot = frame.recipient(seat.slot).unwrap().into_json();
        assert_eq!(snapshot["t"], "s");
        assert_eq!(snapshot["mode"]["kind"], "horde");
    }
    assert!(m.tick_state.fault().is_none());
}

#[test]
fn base24_shared_prerequisite_assist_and_owner_only_cancellation() {
    let mut m = scene(true);
    let maker = building(&m, "barracks");
    let builder = m
        .recruit(1, "rifle", Some(maker), false, None)
        .unwrap()
        .unit;
    let helper = m
        .recruit(0, "rifle", Some(maker), false, None)
        .unwrap()
        .unit;
    assert!(
        !m.world
            .game
            .units
            .values()
            .any(|u| { u.owner == 1 && m.rules.kinds[u.kind].name == "barracks" })
    );
    let wallets = m.start.economy.clone();
    let site = m.next_unit_id;
    m.world.game.units.get_mut(&maker).unwrap().built = Some(0.5);
    assert_eq!(
        command(
            &mut m,
            1,
            Command::Build {
                ids: vec![i64::from(builder)],
                kind: "motorpool".into(),
                at: Some(Point { x: 85.0, z: 25.0 }),
                direction: None,
                queue: false,
            }
        ),
        Some(Denial::Needs),
        "an unfinished shared prerequisite is insufficient"
    );
    assert_eq!(m.next_unit_id, site);
    assert_eq!(m.start.economy, wallets);
    m.world.game.units.get_mut(&maker).unwrap().built = Some(1.0);
    assert_eq!(
        command(
            &mut m,
            1,
            Command::Build {
                ids: vec![i64::from(builder)],
                kind: "motorpool".into(),
                at: Some(Point { x: 85.0, z: 25.0 }),
                direction: None,
                queue: false,
            }
        ),
        None,
        "the shared finished Barracks supplies the prerequisite"
    );
    assert_eq!(m.world.game.units[&site].owner, 1);
    assert_eq!(m.start.economy.players[0], wallets.players[0]);
    assert_eq!(m.start.economy.players[1].mp, wallets.players[1].mp - 200.0);
    let paid = m.start.economy.clone();
    assert_eq!(
        command(
            &mut m,
            0,
            Command::Assist {
                ids: vec![i64::from(helper)],
                target: i64::from(site),
                at: None,
                queue: false,
            }
        ),
        None
    );
    assert_eq!(m.commands.units[&helper].build_target, Some(site));
    assert!(m.world.game.units[&helper].build);
    assert_eq!(m.start.economy, paid, "assisting charges neither commander");
    assert_eq!(
        m.execute_assist(2, &[i64::from(helper)], i64::from(site), false)
            .unwrap(),
        Some(Denial::Unseen)
    );
    let before = m.world.game.units[&site].built.unwrap();
    let at = m.world.game.units[&site].at();
    let unit = m.world.game.units.get_mut(&helper).unwrap();
    unit.x = at.x + 4.0;
    unit.z = at.z;
    m.grid.update(helper, unit);
    let mut frame = m.begin_construction_tick().unwrap();
    m.step_construction_worker(helper, &mut frame).unwrap();
    m.finish_construction_tick(frame).unwrap();
    assert_eq!(m.world.game.units[&site].built, Some(before + 0.05 / 45.0));
    assert_eq!(m.start.economy, paid);
    let story = m.story.clone();
    assert_eq!(
        m.cancel_construction(0, site).unwrap(),
        Some(Denial::Unseen)
    );
    assert!(m.world.game.units.contains_key(&site));
    assert_eq!(m.start.economy, paid);
    assert_eq!(m.story, story);
    assert_eq!(m.cancel_construction(1, site).unwrap(), None);
    assert!(!m.world.game.units.contains_key(&site));
    assert_eq!(m.start.economy.players[0], paid.players[0]);
    assert_eq!(m.start.economy.players[1].mp, paid.players[1].mp + 150.0);
    assert_eq!(m.story[1].mp_spent, story[1].mp_spent - 150.0);
    assert!(!m.building_rallies.contains_key(&site));
    assert!(!m.commands.units.contains_key(&site));
    assert!(
        m.grid
            .radius(&m.world.game.units, (at.x, at.z), 1.0, |_, _| true)
            .iter()
            .all(|&id| id != site)
    );
    let mut frame = m.begin_construction_tick().unwrap();
    m.step_construction_worker(helper, &mut frame).unwrap();
    m.finish_construction_tick(frame).unwrap();
    assert_eq!(m.commands.units[&helper].build_target, None);
    assert!(!m.world.game.units[&helper].build);
}

#[test]
fn base24_ordinary_match_finishes_with_every_seat_snapshot() {
    let mut m = scene(false);
    for _ in 0..1000 {
        m.advance_tick().unwrap();
        let cache = SnapshotCache::new(&m).unwrap();
        for slot in 0..m.world.game.players.len() {
            cache.recipient(slot).unwrap();
        }
        assert!(m.tick_state.fault().is_none());
        if m.finish.is_some() {
            break;
        }
    }
    assert_eq!(m.finish.unwrap().winner, 0);
    assert!(m.start.points.iter().all(|p| p.owner == Some(0)));
    assert!(m.start.economy.players[0].vp >= m.start.win_vp);
    for _ in 0..8 {
        m.advance_tick().unwrap();
        let cache = SnapshotCache::new(&m).unwrap();
        for slot in 0..m.world.game.players.len() {
            cache.recipient(slot).unwrap();
        }
        assert!(m.tick_state.fault().is_none());
    }
}
