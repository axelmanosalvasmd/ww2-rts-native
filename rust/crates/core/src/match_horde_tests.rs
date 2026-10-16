use super::*;
use crate::weather::WeatherChoice;
use serde_json::json;

fn scene(defenders: usize, army: ArmySize) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Horde intent", "w":60,"h":60,"rows":vec![".".repeat(60);60],
            "spawns":[{"x":30,"y":5},{"x":10,"y":54},{"x":50,"y":54}],
            "defend":[0], "points":[{"x":30,"y":30},{"x":35,"y":30,"mp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new_horde(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Horde,
            players: (0..defenders)
                .map(|i| PlayerConfig {
                    name: format!("P{i}"),
                    team: i as i32,
                    faction: if i == 0 {
                        Faction::Usa
                    } else {
                        Faction::Germany
                    },
                })
                .collect(),
            shuffle: false,
            army,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: true,
            supply: true,
            logistics: true,
            sandbox: false,
        },
    )
    .unwrap()
}
fn force_wave(m: &mut Match, wave: u32) {
    let h = m.horde.as_mut().unwrap();
    h.wave = wave - 1;
    h.time_left = 0.0;
    h.active = false;
    h.profile_wave = 0;
}
fn living_ground(m: &Match) -> Vec<u32> {
    let slot = m.horde.as_ref().unwrap().slot;
    m.world
        .game
        .units
        .values()
        .filter(|u| u.owner == slot && u.air.is_none() && u.hp > 0.0)
        .map(|u| u.id)
        .collect()
}

#[test]
fn net50_ending_a_break_starts_one_wave_on_the_next_horde_phase_only() {
    let mut m = scene(1, ArmySize::Standard);
    let before = m.horde.clone().unwrap();
    let actors = m.world.game.units.len();
    assert!(m.end_horde_break().unwrap());
    let mut expected = before;
    expected.time_left = 0.0;
    assert_eq!(m.horde.as_ref(), Some(&expected));
    assert_eq!(m.world.game.units.len(), actors);
    assert!(
        m.end_horde_break().unwrap(),
        "repeated requests do not spawn"
    );
    m.step_horde(0.05).unwrap();
    let active = m.horde.clone().unwrap();
    assert_eq!(active.wave, 1);
    assert!(active.active);
    assert!(!m.end_horde_break().unwrap());
    assert_eq!(m.horde.as_ref(), Some(&active));
    for _ in 0..4 {
        m.step_horde(0.05).unwrap();
        assert_eq!(m.horde.as_ref().unwrap().wave, 1);
    }
    assert!(
        m.validate_tick_state().is_err(),
        "full Horde hosting stays guarded"
    );
    assert!(m.preflight_ordinary_snapshot().is_err());
}

#[test]
fn horde_setup_builds_one_fortified_shared_base_and_internal_enemy() {
    let mut m = scene(2, ArmySize::Large);
    let h = m.horde.as_ref().unwrap();
    assert_eq!(
        (
            h.slot,
            h.defenders,
            h.wave,
            h.active,
            h.time_left,
            h.budget,
            h.left
        ),
        (2, 2, 0, false, 45.0, 0.0, 0.0)
    );
    assert_eq!(h.profile_seed, m.start.environment_seed);
    assert_eq!(h.gates.len(), 2);
    assert_eq!(
        m.start
            .players
            .iter()
            .map(|p| p.config.team)
            .collect::<Vec<_>>(),
        vec![0, 0, 1]
    );
    assert_eq!(m.start.players[2].config.faction, Faction::Ussr);
    assert_eq!(m.start.players[0].spawn, m.start.players[1].spawn);
    assert!(!m.options.supply && !m.options.logistics);
    assert_eq!(m.start.points.len(), 1);
    assert_eq!(m.start.economy.players[2].mp, 0.0);
    assert_eq!(m.production.player(0).unwrap().tech().tier(), 2);
    assert_eq!(m.production.player(2).unwrap().tech().tier(), 3);
    for name in ["bunker", "hq", "barracks"] {
        let units = m
            .world
            .game
            .units
            .values()
            .filter(|u| m.rules.kinds[u.kind].name == name)
            .collect::<Vec<_>>();
        assert_eq!(units.len(), 1, "one shared {name}");
        assert_eq!(units[0].owner, 0);
    }
    assert!(m.world.game.chars.contains(&b'T') && m.world.game.chars.contains(&b'#'));
    assert!(
        m.world
            .game
            .units
            .values()
            .any(|u| m.rules.kinds[u.kind].name == "flakpos")
    );
    assert!(m.world.game.units.values().all(|u| u.owner != 2));
    for (gate, spawn) in h
        .gates
        .iter()
        .zip([Point { x: 21.0, z: 109.0 }, Point { x: 101.0, z: 109.0 }])
    {
        assert!(math::hypot(gate.x - spawn.x, gate.z - spawn.z) > 12.0);
        assert!(gate.x > 0.0 && gate.x < 120.0 && gate.z > 0.0 && gate.z < 120.0);
    }
    m.step_horde(0.05).unwrap();
    assert_eq!(m.horde.as_ref().unwrap().wave, 0);
    assert_eq!(m.horde.as_ref().unwrap().time_left, 44.95);
    assert!(
        m.validate_tick_state().is_err(),
        "whole Horde tick stays guarded"
    );
    assert!(
        m.validate_aviation().is_err(),
        "Horde aircraft combat phases stay guarded"
    );
    let five = scene(5, ArmySize::Standard);
    assert_eq!(
        five.start.players.len(),
        6,
        "five defenders share the three-spawn map"
    );
    assert_eq!(five.horde.as_ref().unwrap().defenders, 5);
    assert!(
        five.start.players[..5]
            .iter()
            .all(|p| p.spawn == five.start.players[0].spawn)
    );
    assert_eq!(
        five.world
            .game
            .units
            .values()
            .filter(|u| five.rules.kinds[u.kind].name == "bunker")
            .count(),
        1
    );
}
#[test]
fn mode43_break_starts_announced_seeded_wave_and_retains_two_profiles() {
    let mut a = scene(2, ArmySize::Massive);
    let mut b = scene(2, ArmySize::Massive);
    let seed = a.horde.as_ref().unwrap().profile_seed;
    let announced = a.horde.as_ref().unwrap().next_profile;
    for m in [&mut a, &mut b] {
        m.world.game.tick = 1;
        m.step_horde(44.0).unwrap();
        assert_eq!(m.horde.as_ref().unwrap().wave, 0);
        m.step_horde(1.0).unwrap();
        let h = m.horde.as_ref().unwrap();
        assert_eq!((h.wave, h.active, h.time_left), (1, true, 0.0));
        assert_eq!(h.profile, announced);
        assert_eq!(h.profile_history, vec![announced]);
        assert_eq!(h.total_budget, 3600.0);
        assert!(h.budget < h.total_budget);
        assert!(!h.reserve.is_empty());
    }
    assert_eq!(a.horde, b.horde, "same announced seed gives same purchases");
    assert_eq!(horde_budget(1, 2, 3.0).unwrap(), 1800.0);
    assert_eq!(horde_budget(2, 2, 3.0).unwrap(), 2250.0);
    assert_eq!(horde_budget(10, 2, 3.0).unwrap(), 13411.04507446289);
    assert_eq!(
        horde_budget(10000, 5, 3.0).unwrap(),
        9_007_199_254_740_991.0
    );
    assert_ne!(
        a.horde.as_ref().unwrap().purchase_seed,
        seed.wrapping_add(1)
    );
    for wave in 2..=3 {
        force_wave(&mut a, wave);
        a.step_horde(0.05).unwrap();
    }
    assert_eq!(a.horde.as_ref().unwrap().profile_history.len(), 2);
    assert_eq!(
        a.horde.as_ref().unwrap().profile_history[1],
        a.horde.as_ref().unwrap().profile
    );
}
#[test]
fn mode45_profiles_obey_eligibility_seed_history_and_break_preparation() {
    use HordeProfile::{Armor as A, Infantry as I, Mixed as M, Siege as S};
    // Closed expected sequence from the pinned profile formula, profile seed 12345.
    let expected = [M, I, M, S, M, I, S, A, I, A, M, I, M, A, I, S, A, I, I, S];
    let mut history = Vec::new();
    for (index, expected) in expected.into_iter().enumerate() {
        let wave = index as u32 + 1;
        let p = horde_profile(
            wave,
            horde_budget(wave, 1, 1.0).unwrap(),
            12345 + wave,
            &history,
            None,
        )
        .unwrap();
        assert_eq!(p, expected, "wave {wave}");
        assert!(!(history.len() >= 2 && history[history.len() - 2..].iter().all(|old| *old == p)));
        history.push(p);
    }
    assert_eq!(horde_profile(1, 300.0, 1, &[], Some(A)).unwrap(), M);
    assert_eq!(horde_profile(2, 300.0, 1, &[], Some(S)).unwrap(), M);
    assert_eq!(horde_profile(3, 179.0, 1, &[], Some(S)).unwrap(), M);
    assert_eq!(horde_profile(3, 180.0, 1, &[], Some(S)).unwrap(), S);
    assert_eq!(horde_profile(5, 220.0, 1, &[], Some(A)).unwrap(), A);
    assert_eq!(horde_profile(1, 79.0, 1, &[], Some(I)).unwrap(), M);
    assert_eq!(horde_profile(1, 0.0, 1, &[M, M], None).unwrap(), M);
    let mut m = scene(1, ArmySize::Standard);
    let initial = m.horde.as_ref().unwrap().next_profile;
    let bunker = m.horde.as_ref().unwrap().bunker;
    {
        let h = m.horde.as_mut().unwrap();
        h.wave = 1;
        h.active = true;
        h.budget = 0.0;
    }
    m.step_horde(0.05).unwrap();
    let h = m.horde.as_ref().unwrap();
    assert!(!h.active);
    assert_eq!(h.profile_wave, 2);
    assert_eq!(
        h.next_profile,
        horde_profile(
            2,
            375.0,
            h.profile_seed.wrapping_add(2),
            &h.profile_history,
            None
        )
        .unwrap()
    );
    assert!(m.world.game.units.contains_key(&bunker));
    assert!(matches!(initial, M | I));
}
#[test]
fn mode46_complete_unlock_weights_and_late_heavy_profile_bias() {
    let expected = [
        ("rifle", 1, 4.0),
        ("conscript", 1, 4.0),
        ("mg", 3, 2.0),
        ("mortar", 3, 1.0),
        ("medic", 4, 1.0),
        ("armoredcar", 5, 1.0),
        ("tank", 5, 2.0),
        ("at", 5, 1.0),
        ("flamer", 6, 1.0),
        ("halftrack", 6, 1.0),
        ("medium", 8, 2.0),
        ("tankdestroyer", 8, 1.0),
        ("rocket", 8, 1.0),
        ("ranger", 9, 1.0),
        ("sniper", 10, 1.0),
        ("howitzer", 10, 1.0),
        ("commando", 11, 1.0),
        ("tiger", 12, 1.0),
        ("churchill", 14, 1.0),
    ];
    for wave in 1..=18 {
        let pool = horde_pool(wave, HordeProfile::Mixed).unwrap();
        let wanted = expected
            .iter()
            .filter(|(_, from, _)| wave >= *from)
            .collect::<Vec<_>>();
        assert_eq!(pool.len(), wanted.len());
        for (unit, (name, from, base)) in pool.iter().zip(wanted) {
            assert_eq!((&unit.kind, unit.from), (&name.to_string(), *from));
            let lean = f64::from(wave.saturating_sub(12)) / 6.0;
            assert_eq!(unit.weight, *base * math::pow(unit.cost / 200.0, lean));
        }
        for (profile, defining) in [
            (
                HordeProfile::Infantry,
                &["rifle", "conscript", "mg", "flamer"][..],
            ),
            (
                HordeProfile::Armor,
                &["tank", "medium", "tiger", "armoredcar", "tankdestroyer"][..],
            ),
            (HordeProfile::Siege, &["mortar", "rocket"][..]),
        ] {
            let themed = horde_pool(wave, profile).unwrap();
            for (unit, plain) in themed.iter().zip(&pool) {
                assert_eq!(
                    unit.weight,
                    plain.weight
                        * if defining.contains(&unit.kind.as_str()) {
                            5.0
                        } else {
                            1.0
                        }
                );
            }
        }
    }
    let late = horde_pool(18, HordeProfile::Mixed).unwrap();
    assert_eq!(
        late.iter().map(|u| u.weight).collect::<Vec<_>>(),
        vec![
            2.0, 1.6, 1.5, 0.9, 0.6, 1.1, 3.0, 1.0, 0.9, 0.9, 3.8, 1.6, 1.25, 1.0, 0.8, 1.5, 0.95,
            2.8, 2.4
        ]
    );
    // Horde entry ignores both the Churchill's UK faction and its one-unit cap.
    let mut m = scene(2, ArmySize::Standard);
    assert_eq!(m.start.players[2].config.faction, Faction::Ussr);
    {
        let h = m.horde.as_mut().unwrap();
        h.wave = 18;
        h.active = true;
        h.budget = 0.0;
        h.reserve = vec!["churchill".into(), "churchill".into()];
    }
    m.world.game.tick = 10;
    m.step_horde(0.05).unwrap();
    let units = living_ground(&m);
    assert_eq!(units.len(), 2);
    assert!(
        units
            .iter()
            .all(|id| m.rules.kinds[m.world.game.units[id].kind].name == "churchill")
    );
}
#[test]
fn mode47_profile_purchases_bound_work_memory_and_unaffordable_remainder() {
    // Seed 1 draws 0.6270739405881613. Cumulative 4/4/2/1 picks
    // conscript; a uniform choice among the same four types would pick MG.
    let unequal = horde_pool(3, HordeProfile::Mixed).unwrap();
    let mut seed = 1;
    assert_eq!(
        pick(&unequal, 1000.0, &mut seed, None).unwrap().kind,
        "conscript"
    );
    assert_eq!(seed, 1831565814);
    let mut m = scene(1, ArmySize::Standard);
    let h = m.horde.as_mut().unwrap();
    h.wave = 8;
    h.profile = HordeProfile::Siege;
    h.budget = 10000.0;
    h.total_budget = 10000.0;
    h.purchase_seed = 12353;
    let pool = horde_pool(8, HordeProfile::Siege).unwrap();
    h.fill_reserve(&pool);
    assert_eq!(h.reserve.len(), 32);
    let defining_cost: f64 = h
        .reserve
        .iter()
        .map(|name| pool.iter().find(|u| &u.kind == name).unwrap())
        .filter(|u| ["mortar", "rocket"].contains(&u.kind.as_str()))
        .map(|u| u.cost)
        .sum();
    assert_eq!(h.profile_spent, defining_cost);
    assert!(
        defining_cost >= 4500.0,
        "defining share is bought before the general pool"
    );
    let mut quota_spent = 0.0;
    let quota_end = h
        .reserve
        .iter()
        .position(|name| {
            assert!(h.profile.types().contains(&name.as_str()));
            quota_spent += pool.iter().find(|u| &u.kind == name).unwrap().cost;
            quota_spent >= h.total_budget * h.profile.share()
        })
        .expect("the defining purchases reach their quota");
    assert!(
        h.reserve[quota_end + 1..]
            .iter()
            .any(|name| !h.profile.types().contains(&name.as_str())),
        "purchases return to the general pool after the defining quota is reached"
    );
    let spent: f64 = h
        .reserve
        .iter()
        .map(|name| pool.iter().find(|u| &u.kind == name).unwrap().cost)
        .sum();
    assert_eq!(h.budget, 10000.0 - spent);
    h.budget = 1e12;
    h.total_budget = 1e12;
    for _ in 0..8 {
        h.fill_reserve(&pool);
    }
    assert_eq!(h.reserve.len(), 240);
    let seed = h.purchase_seed;
    let budget = h.budget;
    h.fill_reserve(&pool);
    assert_eq!(h.reserve.len(), 240);
    assert_eq!((h.purchase_seed, h.budget), (seed, budget));
    h.reserve.clear();
    h.budget = 79.0;
    h.fill_reserve(&pool);
    assert!(h.reserve.is_empty());
    assert_eq!(h.budget, 0.0);
    // A profile with no affordable defining unit must still buy the rest of its pool.
    h.profile = HordeProfile::Siege;
    h.profile_spent = 0.0;
    h.total_budget = 100.0;
    h.budget = 100.0;
    h.fill_reserve(&pool);
    assert_eq!(h.reserve.len(), 1);
    assert!(["rifle", "conscript"].contains(&h.reserve[0].as_str()));
    assert_eq!(h.profile_spent, 0.0);
}
#[test]
fn horde_vehicle_entry_refreshes_clearance_after_adjacent_tank_trap() {
    let mut m = scene(1, ArmySize::Standard);
    let slot = m.horde.as_ref().unwrap().slot;
    let gate = Point { x: 61.0, z: 91.0 };
    let target = Point { x: 61.0, z: 61.0 };
    let first = m
        .spawn_horde_ground(slot, "tank", gate, target, 0, 3.0)
        .unwrap();
    let unit = m.world.game.units.remove(&first).unwrap();
    m.grid.remove_unit(first);
    let profile = &m.rules.kinds[unit.kind];
    let trap = (unit.z / CELL) as usize * m.world.game.w + (unit.x / CELL) as usize + 1;
    assert_eq!(m.world.game.chars[trap], b'.');
    let before = m.world.versions;
    m.world.set_cell(trap, b'Y').unwrap();
    assert!(m.world.versions.vehicle > before.vehicle);
    assert!(m.world.versions.obstruction > before.obstruction);
    assert!(
        !crate::vehicle_navigation::destination_clear(
            terrain_view(&m.world.game),
            unit.at(),
            profile.radius,
            profile.block(),
        ),
        "the adjacent trap overlaps the original entry's vehicle clearance"
    );
    let next = m
        .spawn_horde_ground(slot, "tank", gate, target, 0, 3.0)
        .unwrap();
    let next = &m.world.game.units[&next];
    assert!(
        crate::vehicle_navigation::destination_clear(
            terrain_view(&m.world.game),
            next.at(),
            m.rules.kinds[next.kind].radius,
            m.rules.kinds[next.kind].block(),
        ),
        "the later entry must clear the new tank trap after warming the placement cache"
    );
    assert_ne!(next.at(), unit.at());
}
#[test]
fn mode48_reserve_enters_last_first_on_tenth_ticks_with_stars_and_field_cap() {
    assert_eq!(
        [1, 9, 10, 14, 18, 40].map(|w| horde_stars(w).unwrap()),
        [0, 0, 1, 2, 3, 3]
    );
    for (wave, xp_per_cost) in [(9, 0.0), (10, 1.0), (14, 2.5), (18, 5.0)] {
        let mut m = scene(1, ArmySize::Standard);
        {
            let h = m.horde.as_mut().unwrap();
            h.wave = wave;
            h.active = true;
            h.budget = 0.0;
            h.reserve = vec!["rifle".into(), "conscript".into(), "medic".into()];
        }
        m.world.game.tick = 9;
        m.step_horde(0.05).unwrap();
        assert!(living_ground(&m).is_empty());
        m.world.game.tick = 10;
        let before = m.next_unit_id;
        m.step_horde(0.05).unwrap();
        let ids = living_ground(&m);
        assert_eq!(ids, vec![before, before + 1]);
        assert_eq!(
            m.rules.kinds[m.world.game.units[&before].kind].name,
            "medic"
        );
        assert_eq!(
            m.rules.kinds[m.world.game.units[&(before + 1)].kind].name,
            "conscript"
        );
        assert_eq!(m.horde.as_ref().unwrap().reserve, vec!["rifle"]);
        let bunker = m.world.game.units[&m.horde.as_ref().unwrap().bunker].at();
        for id in ids {
            let u = &m.world.game.units[&id];
            assert_eq!(u.amove, Some(bunker));
            assert!(
                !u.path.is_empty(),
                "entry carries an actual attack-move path"
            );
            assert!(u.ghost > 10.0);
            let cost = number(
                Catalog::compiled()
                    .unwrap()
                    .unit(&m.rules.kinds[u.kind].name)
                    .unwrap(),
                "cost",
            )
            .unwrap();
            assert_eq!(m.combat.get(id).unwrap().xp, xp_per_cost * cost);
        }
    }
    for (defenders, cap) in [(1, 60), (5, 240)] {
        let mut m = scene(defenders, ArmySize::Standard);
        let h = m.horde.as_ref().unwrap();
        let (slot, gate, target) = (h.slot, h.gates[0], m.world.game.units[&h.bunker].at());
        for _ in 0..cap - 1 {
            m.spawn_horde_ground(slot, "rifle", gate, target, 0, 3.0)
                .unwrap();
        }
        {
            let h = m.horde.as_mut().unwrap();
            h.wave = 1;
            h.active = true;
            h.budget = 0.0;
            h.reserve = vec!["rifle".into(); 3];
        }
        m.world.game.tick = 10;
        m.step_horde(0.05).unwrap();
        assert_eq!(living_ground(&m).len(), cap);
        assert_eq!(m.horde.as_ref().unwrap().reserve.len(), 2);
        m.world.game.tick = 20;
        m.step_horde(0.05).unwrap();
        assert_eq!(living_ground(&m).len(), cap);
        assert_eq!(m.horde.as_ref().unwrap().reserve.len(), 2);
    }
}
#[test]
fn mode49_left_counts_live_ground_reserve_and_budget_then_clears_planes_and_heals() {
    let mut m = scene(1, ArmySize::Standard);
    let (slot, bunker, gate) = {
        let h = m.horde.as_ref().unwrap();
        (h.slot, h.bunker, h.gates[0])
    };
    let target = m.world.game.units[&bunker].at();
    let ground = m
        .spawn_horde_ground(slot, "rifle", gate, target, 0, 3.0)
        .unwrap();
    let dead = m
        .spawn_horde_ground(slot, "rifle", gate, target, 0, 3.0)
        .unwrap();
    m.world.game.units.get_mut(&dead).unwrap().hp = 0.0;
    let plane = m.spawn_aircraft(slot, "attacker").unwrap();
    m.world.game.units.get_mut(&bunker).unwrap().hp = 1000.0;
    {
        let h = m.horde.as_mut().unwrap();
        h.wave = 1;
        h.active = true;
        h.budget = 81.0;
        h.total_budget = 81.0;
        h.reserve = vec!["rifle".into(); 240];
    }
    m.world.game.tick = 1;
    m.step_horde(0.05).unwrap();
    assert_eq!(
        m.horde.as_ref().unwrap().left,
        243.0,
        "one ground, 240 reserve, ceil(81/80); dead and planes excluded"
    );
    m.world.game.units.get_mut(&ground).unwrap().hp = 0.0;
    {
        let h = m.horde.as_mut().unwrap();
        h.budget = 0.0;
        h.reserve.clear();
    }
    m.step_horde(0.05).unwrap();
    let h = m.horde.as_ref().unwrap();
    assert_eq!(
        (h.active, h.left, h.time_left, h.profile_wave),
        (false, 0.0, 45.0, 2)
    );
    assert_eq!(m.world.game.units[&bunker].hp, 1300.0);
    for id in [ground, dead, plane] {
        assert!(!m.world.game.units.contains_key(&id));
        assert!(!m.commands.units.contains_key(&id));
        assert!(m.abilities.get(id).is_none() && m.recovery.get(id).is_none());
    }
    assert!(!m.world.game.units.values().any(|u| u.owner == slot));
    assert!(m.aviation.planes.is_empty());
    m.step_horde(1.0).unwrap();
    assert_eq!(
        (
            m.horde.as_ref().unwrap().active,
            m.horde.as_ref().unwrap().wave
        ),
        (false, 1)
    );
    // The next clear caps the repair at the bunker maximum.
    {
        let h = m.horde.as_mut().unwrap();
        h.active = true;
        h.budget = 0.0;
    }
    m.world.game.units.get_mut(&bunker).unwrap().hp = 2900.0;
    m.step_horde(0.05).unwrap();
    assert_eq!(m.world.game.units[&bunker].hp, 3000.0);
}
#[test]
fn mode50_support_wallet_replaced_at_each_wave_scaled_by_defenders_and_army() {
    for (defenders, army, income) in [
        (1, ArmySize::Standard, 1.0),
        (3, ArmySize::Large, 3.0),
        (5, ArmySize::Massive, 6.0),
    ] {
        let mut m = scene(defenders, army);
        m.world.game.tick = 1;
        for wave in 5..=12 {
            force_wave(&mut m, wave);
            let slot = m.horde.as_ref().unwrap().slot;
            m.start.economy.players[slot].mp = 999999.0;
            m.step_horde(0.05).unwrap();
            let expected = if wave < 6 {
                0.0
            } else {
                100.0 + 150.0 * f64::from(wave - 5) * defenders as f64 * income
            };
            assert_eq!(
                m.start.economy.players[slot].mp, expected,
                "wave {wave}, defenders {defenders}"
            );
        }
    }
}
#[test]
fn mode51_planes_start_at_ten_scale_to_eight_every_third_fighter_and_do_not_count_left() {
    for defenders in [1, 2, 3, 5] {
        let mut m = scene(defenders, ArmySize::Standard);
        for wave in [9, 10, 12, 13, 16, 19, 31] {
            let old: Vec<_> = m
                .world
                .game
                .units
                .values()
                .filter(|u| u.air.is_some())
                .map(|u| u.id)
                .collect();
            for id in old {
                m.world.game.units.remove(&id);
                m.aviation.remove(id);
            }
            force_wave(&mut m, wave);
            m.world.game.tick = 1;
            m.step_horde(0.05).unwrap();
            let planes = m
                .world
                .game
                .units
                .values()
                .filter(|u| u.air.is_some())
                .collect::<Vec<_>>();
            let count = if wave < 10 {
                0
            } else {
                ((1 + (wave - 10) / 3) as usize * defenders.div_ceil(2)).min(8)
            };
            assert_eq!(planes.len(), count, "wave {wave}, defenders {defenders}");
            for (i, u) in planes.iter().enumerate() {
                assert_eq!(u.owner, defenders);
                assert_eq!(
                    m.rules.kinds[u.kind].name,
                    if i % 3 == 2 { "fighter" } else { "attacker" }
                );
                assert!(m.aviation.planes.contains_key(&u.id));
            }
            let h = m.horde.as_ref().unwrap();
            let cheapest = horde_pool(wave, h.profile)
                .unwrap()
                .iter()
                .map(|u| u.cost)
                .fold(f64::INFINITY, f64::min);
            assert_eq!(
                h.left,
                living_ground(&m).len() as f64
                    + h.reserve.len() as f64
                    + (h.budget / cheapest).ceil()
            );
        }
    }
}

#[test]
fn mode43_and47_seeded_ordinary_wave_has_the_announced_exact_purchase_sequence() {
    let mut m = scene(2, ArmySize::Standard);
    m.world.game.tick = 1;
    let h = m.horde.as_mut().unwrap();
    h.profile_seed = 12345;
    h.prepare_profile(1.0).unwrap();
    assert_eq!(h.next_profile, HordeProfile::Mixed);
    h.time_left = 0.0;
    // Other purpose streams must not replace or advance the captured purchase seed.
    m.start.random.next_u64(crate::random::StreamId::Combat);
    m.start.environment_seed = 456;
    m.step_horde(0.05).unwrap();
    let h = m.horde.as_ref().unwrap();
    assert_eq!(h.profile, HordeProfile::Mixed);
    assert_eq!(
        h.reserve,
        [
            "rifle",
            "conscript",
            "rifle",
            "rifle",
            "conscript",
            "conscript"
        ]
    );
    assert_eq!(h.purchase_seed, 2399472632);
    assert_eq!(h.total_budget, 600.0);
    assert_eq!(
        h.budget, 0.0,
        "the unaffordable 60 MP remainder is exhausted"
    );
    assert_eq!(h.left, 6.0);
}

#[test]
fn mode42_defenders_recruit_from_shared_facilities_with_their_own_wallet_and_army() {
    let mut m = scene(2, ArmySize::Large);
    let maker = m
        .world
        .game
        .units
        .values()
        .find(|u| m.rules.kinds[u.kind].name == "barracks")
        .unwrap()
        .id;
    assert_eq!(m.world.game.units[&maker].owner, 0);
    let wallets = m.start.economy.clone();
    let story = m.story.clone();
    let next = m.next_unit_id;
    let made = m.recruit(1, "rifle", Some(maker), false, None).unwrap();
    assert_eq!(made.unit, next);
    assert_eq!(made.purchase.request.building, maker);
    assert_eq!(made.purchase.request.owner, 1);
    assert_eq!(m.world.game.units[&made.unit].owner, 1);
    assert_eq!(m.world.game.units[&maker].owner, 0);
    assert_eq!(m.recruit_metadata[&made.unit].building, maker);
    assert_eq!(m.start.economy.players[0], wallets.players[0]);
    assert_eq!(m.start.economy.players[2], wallets.players[2]);
    assert_eq!(m.start.economy.players[1].mp, wallets.players[1].mp - 100.0);
    assert_eq!(m.story[0], story[0]);
    assert_eq!(m.story[2], story[2]);
    assert_eq!(m.story[1].mp_spent, story[1].mp_spent + 100.0);
    assert!(m.combat.get(made.unit).is_some() && m.firing.get(made.unit).is_some());
    assert!(m.abilities.get(made.unit).is_some() && m.recovery.get(made.unit).is_some());
    assert!(m.commands.units.contains_key(&made.unit));
    // Give the scripted enemy enough MP, so its refusal proves facility access.
    m.start.economy.players[2].mp = 1000.0;
    let wallets = m.start.economy.clone();
    let story = m.story.clone();
    let next = m.next_unit_id;
    let units = m.world.game.units.len();
    assert!(matches!(
        m.recruit(2, "rifle", Some(maker), false, None),
        Err(crate::match_runtime::RecruitError::Admission(
            crate::production::Error::Denied(crate::production::Denial::Needs)
        ))
    ));
    assert_eq!(m.start.economy, wallets);
    assert_eq!(m.story, story);
    assert_eq!(m.next_unit_id, next);
    assert_eq!(m.world.game.units.len(), units);
    // A broken mode identity cannot reuse the trusted shared-start marker.
    m.world.game.mode = None;
    assert!(m.recruit(1, "rifle", Some(maker), false, None).is_err());
    assert_eq!(m.start.economy, wallets);
}

#[test]
fn mode42_gates_follow_sixteen_metres_of_medium_route_including_multiple_legs() {
    let start = Point { x: 0.0, z: 0.0 };
    let route = [
        Point { x: 6.0, z: 0.0 },
        Point { x: 6.0, z: 8.0 },
        Point { x: 6.0, z: 20.0 },
    ];
    assert_eq!(along_route(start, &route, 16.0), Point { x: 6.0, z: 10.0 });
    assert_eq!(
        along_route(start, &route[..2], 16.0),
        Point { x: 6.0, z: 8.0 }
    );
    assert_eq!(along_route(start, &[], 16.0), start);
    let m = scene(2, ArmySize::Standard);
    let h = m.horde.as_ref().unwrap();
    let target = m.world.game.units[&h.bunker].at();
    for (&gate, spawn) in h
        .gates
        .iter()
        .zip([Point { x: 21.0, z: 109.0 }, Point { x: 101.0, z: 109.0 }])
    {
        let route = m.horde_path("medium", spawn, target).unwrap();
        let expected = along_route(spawn, &route, 16.0);
        assert!(
            math::hypot(gate.x - expected.x, gate.z - expected.z) < 1e-8,
            "native tank route prefix remains the gate after shared bases are placed"
        );
    }
    // Infantry can cross tank traps; a medium tank must route around the band.
    let mut rows = vec![vec![b'.'; 60]; 60];
    for row in &mut rows[50..53] {
        row[..18].fill(b'Y');
    }
    let rows: Vec<_> = rows
        .into_iter()
        .map(|row| String::from_utf8(row).unwrap())
        .collect();
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Horde vehicle gate", "w":60,"h":60,"rows":rows,
            "spawns":[{"x":30,"y":5},{"x":10,"y":54},{"x":50,"y":54}],
            "defend":[0], "points":[{"x":30,"y":30}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut options = m.options.clone();
    options.players.pop();
    let blocked = Match::new_horde(&map, options).unwrap();
    let h = blocked.horde.as_ref().unwrap();
    let target = blocked.world.game.units[&h.bunker].at();
    let spawn = Point { x: 21.0, z: 109.0 };
    let medium_route = blocked.horde_path("medium", spawn, target).unwrap();
    let rifle_route = blocked.horde_path("rifle", spawn, target).unwrap();
    assert!(!medium_route.is_empty() && !rifle_route.is_empty());
    let medium_gate = along_route(spawn, &medium_route, 16.0);
    let rifle_gate = along_route(spawn, &rifle_route, 16.0);
    assert!(
        math::hypot(medium_gate.x - rifle_gate.x, medium_gate.z - rifle_gate.z) > 4.0,
        "the obstacle distinguishes the first sixteen metres of a medium route from a rifle route"
    );
    assert!(
        math::hypot(h.gates[0].x - medium_gate.x, h.gates[0].z - medium_gate.z) < 1e-8,
        "the authored gate follows the medium route, rather than the infantry shortcut"
    );
}
