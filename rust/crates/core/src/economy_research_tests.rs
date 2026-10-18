//! Research intent through canonical Match admission, ticks and recipient views.
use crate::{
    economy::{Faction, Mode, TechState, Technology},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig, ProductionStepError},
    production::{Denial, Error, ProductionState},
    recipient_snapshot::SnapshotCache,
    weather::WeatherChoice,
};
use serde_json::json;

const ORDINARY: [Mode; 5] = [
    Mode::Default,
    Mode::Conquest,
    Mode::Assault,
    Mode::Annihilation,
    Mode::Classic,
];
const LINES: [Technology; 3] = [Technology::Infantry, Technology::Guns, Technology::Armor];

fn options(mode: Mode) -> MatchOptions {
    MatchOptions {
        seed: 0x4950,
        mode,
        players: (0..2)
            .map(|slot| PlayerConfig {
                name: format!("Research {slot}"),
                team: slot,
                faction: Faction::Usa,
            })
            .collect(),
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: Some(1),
        living_weather: false,
        map_key: None,
        tech: true,
        supply: false,
        logistics: false,
        sandbox: false,
    }
}

fn scene(mode: Mode) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Research modes", "w":128, "h":128,
            "rows":vec![".".repeat(128);128],
            "spawns":[{"x":8,"y":8},{"x":119,"y":119}],
            "defend":[0], "assaultTime":900,
            "points":[{"x":60,"y":60,"mp":1,"vp":0},
                      {"x":68,"y":68,"mp":1,"vp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    let opts = options(mode);
    let mut m = match mode {
        Mode::Assault => Match::new_assault(&map, opts, 0),
        Mode::Annihilation => Match::new_annihilation(&map, opts),
        Mode::Classic => Match::new_classic(&map, opts),
        Mode::Horde => Match::new_horde(&map, opts),
        Mode::Tutorial => Match::new_tutorial(&map, opts),
        _ => Match::new(&map, opts),
    }
    .unwrap();
    for wallet in &mut m.start.economy.players {
        wallet.mp = 10000.0;
        wallet.mun = 1000.0;
    }
    m
}

fn own(m: &Match, owner: usize, name: &str) -> Vec<u32> {
    m.world
        .game
        .units
        .values()
        .filter(|u| u.owner == owner && m.rules.kinds[u.kind].name == name)
        .map(|u| u.id)
        .collect()
}

fn add_lab(m: &mut Match, owner: usize, name: &str, column: usize) -> u32 {
    let id = m.next_unit_id;
    m.place_starting_building(owner, name, 30 * m.world.game.w + column)
        .unwrap();
    id
}

fn set_tech(m: &mut Match, tech: TechState) {
    m.production = ProductionState::new(vec![tech; m.start.players.len()]).unwrap();
    for unit in m.world.game.units.values() {
        if m.rules.kinds[unit.kind].building {
            m.production.initialize_building(unit.id).unwrap();
        }
    }
}

fn snapshots(m: &Match) {
    let cache = SnapshotCache::new(m).unwrap();
    for slot in 0..m.start.players.len() {
        let view = cache.recipient(slot).unwrap();
        if m.options.tech {
            assert_eq!(
                view.fields["tech"]["tier"],
                json!(m.production.player(slot).unwrap().tech().tier())
            );
        } else {
            assert!(!view.fields.contains_key("tech"));
        }
    }
}

fn tick(m: &mut Match, count: usize) {
    for _ in 0..count {
        m.advance_tick().unwrap();
        assert!(m.tick_state.fault().is_none());
    }
    snapshots(m);
}

fn deny(m: &mut Match, slot: usize, kind: Technology, denial: Denial) {
    let before = (m.production.clone(), m.start.economy.clone());
    assert_eq!(
        m.request_research(slot, kind),
        Err(ProductionStepError::Production(Error::Denied(denial)))
    );
    assert_eq!((m.production.clone(), m.start.economy.clone()), before);
}

fn complete(m: &mut Match, kind: Technology, level: u8) {
    for _ in 0..1900 {
        if m.production.player(0).unwrap().tech().level(kind) == level {
            snapshots(m);
            return;
        }
        m.advance_tick().unwrap();
    }
    panic!(
        "research {kind:?} did not reach {level} in {:?}",
        m.options.mode
    );
}

#[test]
fn base49_ordinary_mode_tiers_use_live_hqs_keep_progress_and_survive_hq_loss() {
    for mode in ORDINARY {
        let mut m = scene(mode);
        let hqs = own(&m, 0, "hq");
        assert!(!hqs.is_empty(), "{mode:?}");
        for (tier, mp, mun, time) in [(2, 250.0, 50.0, 60.0), (3, 400.0, 100.0, 90.0)] {
            for &hq in &hqs {
                m.world.game.units.get_mut(&hq).unwrap().built = Some(0.5);
            }
            deny(&mut m, 0, Technology::Tier, Denial::Needs);
            tick(&mut m, 3);
            for &hq in &hqs {
                m.world.game.units.get_mut(&hq).unwrap().built = Some(1.0);
            }
            if mode == Mode::Classic {
                m.start.economy.players[0].mun = mun - 1.0;
                deny(&mut m, 0, Technology::Tier, Denial::Mun);
                m.start.economy.players[0].mun = 1000.0;
            }
            let wallet = m.start.economy.players[0].clone();
            let cost = m.request_research(0, Technology::Tier).unwrap();
            let mun = if mode == Mode::Classic { mun } else { 0.0 };
            assert_eq!((cost.mp, cost.mun, cost.time), (mp, mun, time));
            assert_eq!(m.start.economy.players[0].mp, wallet.mp - mp);
            assert_eq!(m.start.economy.players[0].mun, wallet.mun - mun);
            deny(&mut m, 0, Technology::Tier, Denial::Max);
            if tier == 2 && mode != Mode::Classic {
                let recruit = m.recorded_buy(0, "rifle", Some(hqs[0])).unwrap();
                assert_eq!(recruit.purchase.request.building, hqs[0]);
                assert_eq!(m.world.game.units[&recruit.unit].owner, 0);
                assert_eq!(m.production.player(0).unwrap().jobs().len(), 1);
            }
            tick(&mut m, 4);
            let progress = m.production.player(0).unwrap().jobs()[0].progress;
            assert!(progress > 0.0);
            for &hq in &hqs {
                m.world.game.units.get_mut(&hq).unwrap().built = Some(0.5);
            }
            tick(&mut m, 8);
            assert_eq!(m.production.player(0).unwrap().jobs()[0].progress, progress);
            for &hq in &hqs {
                m.world.game.units.get_mut(&hq).unwrap().built = Some(1.0);
            }
            complete(&mut m, Technology::Tier, tier);
            assert!(m.production.player(0).unwrap().jobs().is_empty());
        }
        deny(&mut m, 0, Technology::Tier, Denial::Max);
        for hq in hqs {
            m.world.game.units.get_mut(&hq).unwrap().hp = 0.0;
        }
        tick(&mut m, 4);
        assert!(own(&m, 0, "hq").is_empty());
        assert_eq!(m.production.player(0).unwrap().tech().tier(), 3);
    }
}

#[test]
fn base49_classic_same_hq_trains_while_tier_research_is_pending() {
    let mut m = scene(Mode::Classic);
    let hq = own(&m, 0, "hq")[0];
    m.request_research(0, Technology::Tier).unwrap();
    let job = m.recorded_world_buy(0, "rifle", Some(hq)).unwrap();
    assert_eq!(m.production.building(hq).unwrap().jobs()[0].id, job);
    let before = own(&m, 0, "rifle").len();
    for _ in 0..1000 {
        m.advance_tick().unwrap();
        if own(&m, 0, "rifle").len() > before {
            break;
        }
    }
    assert_eq!(own(&m, 0, "rifle").len(), before + 1);
    assert!(m.production.building(hq).unwrap().jobs().is_empty());
    let research = m.production.player(0).unwrap();
    assert_eq!(research.tech().tier(), 1);
    assert_eq!(research.jobs().len(), 1);
    assert!(research.jobs()[0].progress > 0.0);
    tick(&mut m, 4);
    complete(&mut m, Technology::Tier, 2);
}

#[test]
fn base50_ordinary_modes_enforce_armory_access_capacity_prices_and_retained_progress() {
    for mode in ORDINARY {
        let mut m = scene(mode);
        set_tech(&mut m, TechState::new(3, 0, 0, 0).unwrap());
        let foreign = add_lab(&mut m, 1, "armory", 40);
        deny(&mut m, 0, Technology::Infantry, Denial::Needs);
        let labs = [
            add_lab(&mut m, 0, "armory", 20),
            add_lab(&mut m, 0, "armory", 25),
            add_lab(&mut m, 0, "armory", 30),
        ];
        for &id in &labs[1..] {
            m.world.game.units.get_mut(&id).unwrap().built = Some(0.5);
        }
        for (level, mp, mun, time) in [
            (1, 100.0, 25.0, 30.0),
            (2, 175.0, 50.0, 45.0),
            (3, 250.0, 75.0, 60.0),
        ] {
            for (index, line) in LINES.into_iter().enumerate() {
                if mode == Mode::Classic {
                    m.start.economy.players[0].mun = mun - 1.0;
                    deny(&mut m, 0, line, Denial::Mun);
                    m.start.economy.players[0].mun = 1000.0;
                }
                let wallet = m.start.economy.players[0].clone();
                let cost = m.request_research(0, line).unwrap();
                let mun = if mode == Mode::Classic { mun } else { 0.0 };
                assert_eq!((cost.mp, cost.mun, cost.time), (mp, mun, time));
                assert_eq!(m.start.economy.players[0].mp, wallet.mp - mp);
                assert_eq!(m.start.economy.players[0].mun, wallet.mun - mun);
                deny(&mut m, 0, line, Denial::Max);
                if let Some(&next) = LINES.get(index + 1) {
                    deny(&mut m, 0, next, Denial::QueueFull);
                    m.world.game.units.get_mut(&labs[index + 1]).unwrap().built = Some(1.0);
                }
            }
            tick(&mut m, 4);
            let progress = m.production.player(0).unwrap().jobs()[0].progress;
            for &id in &labs[1..] {
                m.world.game.units.get_mut(&id).unwrap().built = Some(0.5);
            }
            tick(&mut m, 4);
            let jobs = m.production.player(0).unwrap().jobs();
            assert!(jobs[0].progress > progress);
            assert_eq!(jobs[1].progress, progress);
            assert_eq!(jobs[2].progress, progress);
            assert_eq!(m.world.game.units[&foreign].built, Some(1.0));
            for &id in &labs[1..] {
                m.world.game.units.get_mut(&id).unwrap().built = Some(1.0);
            }
            complete(&mut m, Technology::Armor, level);
            assert!(m.production.player(0).unwrap().jobs().is_empty());
            for line in LINES {
                assert_eq!(m.production.player(0).unwrap().tech().level(line), level);
            }
            for &id in &labs[1..] {
                m.world.game.units.get_mut(&id).unwrap().built = Some(0.5);
            }
        }
        for line in LINES {
            deny(&mut m, 0, line, Denial::Max);
        }
    }
}

#[test]
fn base50_ordinary_armory_next_level_requires_matching_player_tier() {
    for mode in ORDINARY {
        let mut m = scene(mode);
        add_lab(&mut m, 0, "armory", 20);
        for tier in [1, 2] {
            set_tech(&mut m, TechState::new(tier, tier, tier, tier).unwrap());
            for line in LINES {
                deny(&mut m, 0, line, Denial::Tier);
            }
            tick(&mut m, 3);
        }
    }
}

#[test]
fn base49_base50_horde_research_uses_shared_defender_labs_and_private_progress() {
    let mut m = scene(Mode::Horde);
    let attacker = m.horde.as_ref().unwrap().slot;
    assert_eq!(attacker, 2);
    assert!(!own(&m, 0, "hq").is_empty());
    assert!(own(&m, 1, "hq").is_empty());
    let lab = add_lab(&mut m, 0, "armory", 30);
    for owner in 0..2 {
        let cost = m.request_research(owner, Technology::Tier).unwrap();
        assert_eq!((cost.mp, cost.mun, cost.time), (400.0, 0.0, 90.0));
        m.request_research(owner, Technology::Infantry).unwrap();
        deny(&mut m, owner, Technology::Guns, Denial::QueueFull);
    }
    deny(&mut m, attacker, Technology::Infantry, Denial::Needs);
    m.step_research(5.0).unwrap();
    m.world.game.units.get_mut(&lab).unwrap().built = Some(0.5);
    m.step_research(10.0).unwrap();
    for owner in 0..2 {
        let player = m.production.player(owner).unwrap();
        assert_eq!(player.jobs()[0].progress, 15.0);
        assert_eq!(player.jobs()[1].progress, 5.0);
    }
    m.world.game.units.get_mut(&lab).unwrap().built = Some(1.0);
    m.step_research(85.0).unwrap();
    for owner in 0..2 {
        assert_eq!(m.production.player(owner).unwrap().tech().tier(), 3);
        assert_eq!(
            m.production
                .player(owner)
                .unwrap()
                .tech()
                .level(Technology::Infantry),
            1
        );
        assert!(m.production.player(owner).unwrap().jobs().is_empty());
    }
    assert!(m.production.player(attacker).unwrap().jobs().is_empty());
    assert_eq!(
        m.production
            .player(attacker)
            .unwrap()
            .tech()
            .level(Technology::Infantry),
        0
    );
    m.validate_tick_state().unwrap();
    for _ in 0..4 {
        m.advance_tick().unwrap();
        let cache = crate::recipient_snapshot::SnapshotCache::new(&m).unwrap();
        for slot in 0..m.start.players.len() {
            cache.recipient(slot).unwrap();
        }
        assert!(m.tick_state.fault().is_none());
    }
}

#[test]
fn base49_base50_tutorial_disables_research_without_charging_any_seat() {
    let mut m = scene(Mode::Tutorial);
    assert!(!m.options.tech);
    for owner in 0..m.start.players.len() {
        for kind in [
            Technology::Tier,
            Technology::Infantry,
            Technology::Guns,
            Technology::Armor,
        ] {
            deny(&mut m, owner, kind, Denial::Blocked);
        }
    }
    tick(&mut m, 8);
    for owner in 0..m.start.players.len() {
        assert!(m.production.player(owner).unwrap().jobs().is_empty());
    }
}
