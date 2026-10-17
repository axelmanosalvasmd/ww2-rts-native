//! BASE-53: prototype shared/sim.js:975-1002 and the three constructor callers.
use super::*;
use crate::{
    economy::Faction, economy_state::ArmySize, match_runtime::PlayerConfig,
    recipient_snapshot::SnapshotCache, weather::WeatherChoice,
};
use serde_json::json;
use std::collections::BTreeSet;

fn map() -> AuthoritativeMap {
    AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Fortified starts", "w":120, "h":81,
            "rows":vec![".".repeat(120);81],
            "heights":vec!["0".repeat(120);81],
            "spawns":[{"x":18,"y":40},{"x":101,"y":40}],
            "defend":[0], "points":[{"x":60,"y":40,"mp":2,"vp":1}]
        }))
        .unwrap(),
    )
    .unwrap()
}

fn options(mode: Mode) -> MatchOptions {
    MatchOptions {
        seed: 701,
        mode,
        players: vec![
            PlayerConfig {
                name: "West".into(),
                team: 0,
                faction: Faction::Usa,
            },
            PlayerConfig {
                name: "East".into(),
                team: 1,
                faction: Faction::Germany,
            },
        ],
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: Some(1),
        living_weather: false,
        map_key: None,
        tech: false,
        supply: true,
        logistics: false,
        sandbox: false,
    }
}

fn shell() -> Match {
    let map = map();
    let options = options(Mode::Assault);
    let rules = crate::rules::Rules::compiled().unwrap();
    let start = StartPlan::prepare_assault(&map, &options, &rules, 0).unwrap();
    Match::skirmish_shell(&map, options, rules, start).unwrap()
}

fn actors<'a>(m: &'a Match, owner: usize, kind: &str) -> Vec<&'a crate::game::Unit> {
    m.world
        .game
        .units
        .values()
        .filter(|u| u.owner == owner && m.rules.kinds[u.kind].name == kind)
        .collect()
}

fn terrain_cells(m: &Match, ch: u8) -> BTreeSet<(usize, usize)> {
    m.world
        .game
        .chars
        .iter()
        .enumerate()
        .filter(|(_, value)| **value == ch)
        .map(|(c, _)| (c % m.world.game.w, c / m.world.game.w))
        .collect()
}

#[test]
fn base53_fortified_start_has_exact_arcs_gaps_bunker_and_four_finished_corners() {
    let mut m = shell();
    assert_eq!(m.start.players[0].spawn, Point { x: 37.0, z: 81.0 });
    m.fortify_assault(0).unwrap();
    // With a due-east bearing, these are the 15-cell and 17-cell arcs over
    // [-0.9, 0.9]. Literal cells also protect the floating-step endpoints.
    assert_eq!(
        terrain_cells(&m, b'T'),
        BTreeSet::from([
            (27, 28),
            (28, 29),
            (29, 29),
            (29, 30),
            (31, 32),
            (31, 33),
            (32, 34),
            (32, 35),
            (32, 36),
            (33, 37),
            (33, 38),
            (33, 39),
            (33, 40),
            (33, 41),
            (33, 42),
            (33, 43),
            (32, 44),
            (32, 45),
            (32, 46),
            (31, 47),
            (31, 48),
            (29, 50),
            (28, 51),
            (29, 51),
        ])
    );
    assert_eq!(
        terrain_cells(&m, b'#'),
        BTreeSet::from([
            (29, 27),
            (30, 28),
            (31, 29),
            (31, 30),
            (32, 30),
            (32, 31),
            (33, 31),
            (34, 34),
            (34, 35),
            (34, 36),
            (35, 36),
            (35, 37),
            (35, 38),
            (35, 39),
            (35, 40),
            (35, 41),
            (35, 42),
            (35, 43),
            (34, 44),
            (35, 44),
            (34, 45),
            (34, 46),
            (33, 48),
            (32, 49),
            (33, 49),
            (31, 50),
            (32, 50),
            (31, 51),
            (30, 52),
            (29, 53),
        ])
    );
    for (x, z) in [(30, 31), (30, 49), (33, 32), (34, 33), (34, 47)] {
        assert_eq!(
            m.world.game.chars[z * 120 + x],
            b'.',
            "movement gap {x},{z}"
        );
    }
    let bunker = actors(&m, 0, "bunker");
    assert_eq!(bunker.len(), 1);
    assert_eq!(bunker[0].at(), Point { x: 53.0, z: 81.0 });
    assert_eq!(
        (bunker[0].rot, m.commands.units[&bunker[0].id].aim),
        (0.0, 0.0)
    );
    assert_eq!(bunker[0].hp, 3000.0);
    let flaks = actors(&m, 0, "flakpos");
    assert_eq!(
        flaks
            .iter()
            .map(|u| (u.x as i32, u.z as i32))
            .collect::<Vec<_>>(),
        vec![(48, 96), (48, 64), (18, 90), (18, 70)]
    );
    for flak in flaks {
        assert_eq!((flak.built, flak.hp), (Some(1.0), 1500.0));
        let cells = flak.cells.as_ref().unwrap();
        assert_eq!(cells.len(), 4);
        for &cell in cells {
            assert_eq!(m.world.game.chars[cell as usize], b'K');
            assert_eq!(m.building_cells[&(cell as usize)].unit, flak.id);
        }
        assert!(m.production.building(flak.id).is_some());
    }
}

#[test]
fn base53_fortified_start_preserves_ground_and_uses_ordered_flak_fallbacks() {
    let mut displaced = shell();
    displaced.world.set_cell(40 * 120 + 26, b'W').unwrap();
    displaced.fortify_assault(0).unwrap();
    let bunker = actors(&displaced, 0, "bunker")[0];
    assert_eq!(bunker.at(), Point { x: 55.0, z: 81.0 });
    assert_eq!(bunker.rot, 0.0);
    assert_eq!(displaced.world.game.chars[40 * 120 + 26], b'W');
    // A blocked or steep first corner must skip the next swing too, because
    // that footprint is too close to the bunker. The outward swing fits.
    for steep in [false, true] {
        let mut m = shell();
        m.world.set_cell(40 * 120 + 33, b'R').unwrap();
        m.world.set_cell(40 * 120 + 35, b'W').unwrap();
        if steep {
            m.world.set_level(47 * 120 + 23, 3).unwrap();
        } else {
            m.world.set_cell(47 * 120 + 23, b'W').unwrap();
        }
        m.fortify_assault(0).unwrap();
        assert_eq!(m.world.game.chars[40 * 120 + 33], b'R');
        assert_eq!(m.world.game.chars[40 * 120 + 35], b'W');
        let flaks = actors(&m, 0, "flakpos");
        assert_eq!(flaks.len(), 4);
        assert_eq!(flaks[0].at(), Point { x: 44.0, z: 98.0 });
    }
    let mut m = shell();
    // Close all six radius-10 candidates. Radius 11 precedes radius 9.
    for (x, z) in [(23, 47), (25, 45), (21, 48), (27, 42), (27, 38), (25, 33)] {
        m.world.set_cell(z * 120 + x, b'W').unwrap();
    }
    m.fortify_assault(0).unwrap();
    assert_eq!(actors(&m, 0, "flakpos")[0].at(), Point { x: 50.0, z: 98.0 });
}

#[test]
fn base53_fortified_start_uses_tighter_ring_or_abandons_an_impossible_corner() {
    for pad in [false, true] {
        let mut m = shell();
        for c in 0..m.world.game.chars.len() {
            m.world.set_cell(c, b'W').unwrap();
        }
        for (x, z) in [(18, 40), (26, 40)] {
            m.world.set_cell(z * 120 + x, b'.').unwrap();
        }
        if pad {
            // The sole 2x2 pad fits the first radius-8 corner. Every larger
            // ring has at least one water cell in each candidate footprint.
            for (x, z) in [(22, 45), (23, 45), (22, 46), (23, 46)] {
                m.world.set_cell(z * 120 + x, b'.').unwrap();
            }
        }
        m.fortify_assault(0).unwrap();
        let flaks = actors(&m, 0, "flakpos");
        assert_eq!(flaks.len(), usize::from(pad));
        if pad {
            assert_eq!(flaks[0].at(), Point { x: 46.0, z: 92.0 });
        }
        assert_eq!(actors(&m, 0, "bunker")[0].at(), Point { x: 53.0, z: 81.0 });
    }
}

#[test]
fn base53_mode_constructors_fortify_defenders_all_annihilation_seats_and_shared_horde_base() {
    for mode in [Mode::Assault, Mode::Annihilation, Mode::Horde] {
        let map = map();
        let mut m = match mode {
            Mode::Assault => Match::new_assault(&map, options(mode), 0),
            Mode::Annihilation => Match::new_annihilation(&map, options(mode)),
            Mode::Horde => Match::new_horde(&map, options(mode)),
            _ => unreachable!(),
        }
        .unwrap();
        for owner in 0..m.start.players.len() {
            let fortified = owner == 0 || mode == Mode::Annihilation;
            assert_eq!(actors(&m, owner, "bunker").len(), usize::from(fortified));
            assert_eq!(
                actors(&m, owner, "flakpos").len(),
                if fortified { 4 } else { 0 }
            );
        }
        assert!(m.world.game.chars.contains(&b'T'));
        assert!(m.world.game.chars.contains(&b'#'));
        if mode == Mode::Horde {
            assert_eq!(m.start.players[0].spawn, m.start.players[1].spawn);
            for _ in 0..4 {
                m.step_horde(0.05).unwrap();
            }
        }
        {
            for _ in 0..4 {
                m.advance_tick().unwrap();
                let cache = SnapshotCache::new(&m).unwrap();
                for owner in 0..m.start.players.len() {
                    cache.recipient(owner).unwrap();
                }
            }
            assert!(m.tick_state.fault().is_none());
        }
    }
}
