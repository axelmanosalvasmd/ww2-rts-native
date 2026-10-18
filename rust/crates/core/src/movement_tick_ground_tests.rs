use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, MoveOrder},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::{DeliveryState, SelectedSeat},
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};
use serde_json::json;

fn scene(name: &str) -> (Match, u32, usize) {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Movement ground intent", "w":64,"h":64,
            "rows":vec![".".repeat(64);64],
            "spawns":[{"x":5,"y":5},{"x":58,"y":58}],
            "points":[{"x":32,"y":32}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(
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
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    let id = m.next_unit_id;
    let staged = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        0,
        m.rules.kind(name).unwrap(),
        id,
        m.world.game.units.len() as u32,
        m.start.players[0].spawn,
        Some(Point { x: 51.0, z: 51.0 }),
    )
    .unwrap();
    prepare_publication(
        PublicationParts {
            units: &mut m.world.game.units,
            grid: &mut m.grid,
            controls: &mut m.commands.units,
            recruitment: &mut m.recruit_metadata,
            next_id: &mut m.next_unit_id,
        },
        &m.rules,
        2,
        vec![staged],
    )
    .unwrap()
    .publish_with(|u, _| {
        m.combat.initialize(u.id);
        m.firing.initialize(u.id);
        m.abilities.initialize(u.id);
        m.recovery.initialize(u.id);
    });
    for control in m.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_retreat = false;
    }
    let cell = m.world.game.cell_of(51.0, 51.0).unwrap();
    (m, id, cell)
}

fn order(m: &mut Match, id: u32) {
    assert_eq!(
        m.execute_command(
            0,
            &Command::Move {
                orders: vec![MoveOrder {
                    unit: i64::from(id),
                    at: Point { x: 57.0, z: 51.0 },
                }],
                control_ids: None,
                attack_move: false,
                queue: false,
                together: false,
                face: None,
            },
            false,
        )
        .unwrap(),
        None,
    );
}

fn tick(m: &mut Match, delivery: &mut DeliveryState) {
    let report = m.advance_tick().unwrap();
    assert!(
        report
            .phases
            .contains(&crate::match_tick::TickPhase::GroundEffects)
    );
    assert!(
        !report
            .unavailable
            .contains(&crate::match_tick::TickPhase::GroundEffects)
    );
    let selected = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frame = delivery
        .capture_selected_with_tick_events(m, &selected, &report.events)
        .unwrap()
        .unwrap();
    for slot in 0..2 {
        assert_eq!(frame.recipient(slot).unwrap().into_json()["t"], "s");
    }
    assert!(m.tick_state.fault().is_none());
}

#[test]
fn mov71_ordinary_movement_crushes_wears_raises_dust_and_triggers_only_hostile_mines() {
    // The tank starts inside the contacted cell. A real movement block, rather
    // than a synthetic contact call, must apply the ground consequence.
    for (ch, into) in [(b'H', b'.'), (b'#', b'+'), (b'X', b'.'), (b'T', b'+')] {
        let (mut m, id, cell) = scene("tank");
        m.world.set_cell(cell, ch).unwrap();
        let mut delivery = DeliveryState::new(&m).unwrap();
        order(&mut m, id);
        let before = m.world.game.units[&id].at();
        tick(&mut m, &mut delivery);
        assert_ne!(m.world.game.units[&id].at(), before);
        assert_eq!(m.world.game.chars[cell], into, "crush {ch}");
        assert_eq!(m.world.game.cell_hp[cell], 0.0);
        for _ in 0..3 {
            tick(&mut m, &mut delivery);
        }
    }

    for (kind, scale) in [("tank", 1.0), ("truck", 0.5)] {
        for (ch, mud) in [(b'.', 1.0), (b'M', 0.5)] {
            for (wet, low) in [(0.0, false), (0.5, false), (0.2, true)] {
                let (mut m, id, cell) = scene(kind);
                m.world.set_cell(cell, ch).unwrap();
                m.world.game.wet = Some(wet);
                if low {
                    m.world.set_level(cell, -1).unwrap();
                }
                let wet = wet * if low { 2.0 } else { 1.0 };
                let mut delivery = DeliveryState::new(&m).unwrap();
                order(&mut m, id);
                let before = m.world.game.units[&id].at();
                let initial = m.world.game.wear.as_ref().unwrap()[cell];
                tick(&mut m, &mut delivery);
                let after = &m.world.game.units[&id];
                let moved = crate::math::hypot(after.x - before.x, after.z - before.z);
                assert!(moved > 0.0);
                assert_eq!(m.world.game.cell_of(after.x, after.z), Some(cell));
                let expected = f64::from(initial) + moved * 0.05 * scale * mud * (1.0 + 3.0 * wet);
                assert!(
                    (f64::from(m.world.game.wear.as_ref().unwrap()[cell]) - expected).abs() < 1e-6
                );
                assert_eq!(after.dust.is_some(), wet < 0.3 && ch != b'M');
                for _ in 0..3 {
                    tick(&mut m, &mut delivery);
                }
            }
        }
    }

    let (mut ford, id, cell) = scene("tank");
    ford.world.set_cell(cell, b'F').unwrap();
    let mut delivery = DeliveryState::new(&ford).unwrap();
    order(&mut ford, id);
    let before = ford.world.game.units[&id].at();
    tick(&mut ford, &mut delivery);
    assert_ne!(ford.world.game.units[&id].at(), before);
    assert_eq!(ford.world.game.units[&id].dust, None);
    for _ in 0..3 {
        tick(&mut ford, &mut delivery);
    }

    let (mut m, id, cell) = scene("tank");
    m.world.game.wear.as_mut().unwrap()[cell] = 0.999999;
    let mut delivery = DeliveryState::new(&m).unwrap();
    order(&mut m, id);
    tick(&mut m, &mut delivery);
    assert_eq!(m.world.game.chars[cell], b'M');
    assert_eq!(m.world.game.wear.as_ref().unwrap()[cell], 0.0);
    assert_eq!(m.world.game.units[&id].dust, None);
    for _ in 0..3 {
        tick(&mut m, &mut delivery);
    }

    for layer in [0, 1] {
        let (mut m, id, cell) = scene("tank");
        m.world.set_cell(cell, b'N').unwrap();
        m.world.mines.insert(cell, layer);
        let hp = m.world.game.units[&id].hp;
        let mut delivery = DeliveryState::new(&m).unwrap();
        order(&mut m, id);
        let before = m.world.game.units[&id].at();
        tick(&mut m, &mut delivery);
        assert_ne!(m.world.game.units[&id].at(), before);
        if layer == 0 {
            assert_eq!(m.world.game.chars[cell], b'N');
            assert_eq!(m.world.game.units[&id].hp, hp);
        } else {
            assert_eq!(m.world.game.chars[cell], b'+');
            assert!(!m.world.mines.contains_key(&cell));
            assert!(m.world.game.units[&id].hp < hp);
        }
        let hp = m.world.game.units[&id].hp;
        for _ in 0..3 {
            tick(&mut m, &mut delivery);
            assert_eq!(m.world.game.units[&id].hp, hp, "mine must not repeat");
        }
    }
}

#[test]
fn mov71_lethal_mine_contact_keeps_the_dust_raised_before_the_explosion() {
    let (mut m, id, cell) = scene("tank");
    m.world.set_cell(cell, b'N').unwrap();
    m.world.mines.insert(cell, 1);
    m.world.game.tick = 1;
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.hp = 1.;
    let mut before = u.pose();
    before.x -= 0.1;
    m.apply_moved_ground_contact(id, before, None).unwrap();
    assert!(m.world.game.units[&id].hp <= 0.);
    assert_eq!(m.world.game.units[&id].dust, Some(1.));
    assert_eq!(m.world.game.chars[cell], b'+');
    assert!(!m.world.mines.contains_key(&cell));
}
