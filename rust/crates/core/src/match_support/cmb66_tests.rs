use super::*;
use crate::{match_aircraft_ticks::tests::scene, random::StreamId};

fn strike(kind: SupportKind) -> (Strike, SupportTiming) {
    (
        Strike {
            live: false,
            kind: if kind == SupportKind::Recon {
                StrikeKind::Recon
            } else {
                StrikeKind::Other
            },
            owner: 1,
            x: 40.0,
            z: 40.0,
            dir: 0.7,
            runtime: None,
        },
        SupportTiming {
            kind,
            delay: 0.0,
            left: 20.0,
            next: 0.0,
        },
    )
}
fn reserves(m: &mut Match, id: u32, name: &str) {
    m.logistics
        .initialize_unit(
            crate::logistics::Context {
                enabled_override: Some(true),
                scenario: false,
                mode: crate::logistics::Mode::Conquest,
            },
            crate::logistics::UnitFacts {
                id,
                kind: name,
                owner: Some(m.world.game.units[&id].owner),
                owner_exists: true,
                guard_home: false,
                riding: false,
            },
            Some(&crate::logistics::Definition::from_catalog(name).unwrap()),
        )
        .unwrap();
}
fn cover() -> FighterCover {
    FighterCover {
        team: 0,
        owner: 0,
        at: Point { x: 40.0, z: 40.0 },
        radius: 20.0,
        remaining: 10.0,
    }
}

#[test]
fn cmb66_fighter_cover_is_consumed_before_flak_and_recon_bypasses_it() {
    let mut m = scene(&[("flak", 0, 40.0, 40.0)]);
    reserves(&mut m, 1, "flak");
    let ammo = m.logistics.get(1).unwrap().ammo;
    m.support.covers.push(cover());
    let (s, mut timing) = strike(SupportKind::Bombing);
    let shells = number(definition(SupportKind::Bombing).unwrap(), "shells").unwrap();
    let mut expected = m.start.random.clone();
    let kept = (expected.next_fraction(StreamId::OffMap) * shells).floor();
    assert!(m.intercept_support(&s, &mut timing, &[1]).unwrap());
    assert_eq!(timing.left, kept);
    assert!(timing.left >= 0.0 && timing.left < shells);
    assert_eq!(m.support.covers[0].remaining, 0.0);
    assert_eq!(m.logistics.get(1).unwrap().ammo, ammo);
    assert!(matches!(
        m.support.events.last(),
        Some(SupportEvent::ShotDown { fighters: true, .. })
    ));
    assert_eq!(m.story[0].planes_downed, 1.0);

    let mut recon = scene(&[]);
    recon.support.covers.push(cover());
    let (s, mut timing) = strike(SupportKind::Recon);
    assert!(!recon.intercept_support(&s, &mut timing, &[]).unwrap());
    assert_eq!(recon.support.covers[0].remaining, 10.0);
    assert_eq!(timing.left, 20.0);

    for excluded in ["same team", "expired", "outside"] {
        let mut m = scene(&[]);
        let mut zone = cover();
        match excluded {
            "same team" => zone.team = m.world.game.players[1].team,
            "expired" => zone.remaining = 0.,
            "outside" => zone.at.x = 61.,
            _ => unreachable!(),
        }
        let remaining = zone.remaining;
        m.support.covers.push(zone);
        let (s, mut timing) = strike(SupportKind::Dive);
        assert!(!m.intercept_support(&s, &mut timing, &[]).unwrap());
        assert_eq!(m.support.covers[0].remaining, remaining);
        assert!(m.support.events.is_empty());
    }
}

#[test]
fn cmb66_flak_checks_ammo_setup_completion_hold_fire_alliance_range_and_rolls() {
    for (name, ammo, still, built, hold, owner, x, attempts) in [
        ("flak", 0.0, 10.0, 1.0, false, 0, 40.0, 0),
        ("flak", 1.0, 0.0, 1.0, false, 0, 40.0, 0),
        ("flakpos", 1.0, 10.0, 0.5, false, 0, 40.0, 0),
        ("flakpos", 1.0, 10.0, 1.0, false, 0, 40.0, 1),
        ("flak", 1.0, 10.0, 1.0, true, 0, 40.0, 0),
        ("flak", 1.0, 10.0, 1.0, false, 1, 40.0, 0),
        ("flak", 1.0, 10.0, 1.0, false, 0, 100.0, 0),
        ("flak", 1.0, 10.0, 1.0, false, 0, 40.0, 1),
        ("flaktrack", 1.0, 0.0, 1.0, false, 0, 40.0, 1),
        ("flaktrack", 0.0, 10.0, 1.0, false, 0, 40.0, 0),
    ] {
        let mut m = scene(&[(name, owner, x, 40.0)]);
        if name != "flakpos" {
            reserves(&mut m, 1, name);
            m.logistics.get_mut(1).unwrap().ammo = ammo;
        }
        let u = m.world.game.units.get_mut(&1).unwrap();
        u.still = Some(still);
        if name == "flakpos" {
            u.built = Some(built);
        }
        m.commands.units.get_mut(&1).unwrap().hold_fire = hold;
        let (s, mut timing) = strike(SupportKind::Recon);
        let mut expected = m.start.random.clone();
        let chance = Catalog::compiled()
            .unwrap()
            .unit(name)
            .unwrap()
            .get("aa")
            .unwrap()
            .get("chance")
            .unwrap()
            .number()
            .unwrap();
        assert_eq!(chance, if name == "flakpos" { 0.45 } else { 0.35 });
        let success = attempts == 1 && expected.next_fraction(StreamId::OffMap) < chance;
        let kept = if success {
            expected.next_fraction(StreamId::OffMap) * timing.left
        } else {
            timing.left
        };
        assert_eq!(m.intercept_support(&s, &mut timing, &[1]).unwrap(), success);
        assert_eq!(timing.left, kept);
        if name == "flakpos" {
            assert!(
                m.logistics.get(1).is_none(),
                "emplacements have unlimited ammunition"
            );
        } else {
            assert_eq!(m.logistics.get(1).unwrap().ammo, ammo - attempts as f64);
        }
        assert_eq!(
            m.support
                .events
                .iter()
                .filter(|e| matches!(e, SupportEvent::Flak { unit: 1, .. }))
                .count(),
            attempts as usize
        );
    }
    for blocked in ["dead", "forced", "allied"] {
        let mut m = scene(&[("flak", 0, 40.0, 40.0)]);
        reserves(&mut m, 1, "flak");
        m.logistics.get_mut(1).unwrap().ammo = 1.0;
        if blocked == "dead" {
            m.world.game.units.get_mut(&1).unwrap().hp = 0.0;
        } else if blocked == "forced" {
            m.logistics.get_mut(1).unwrap().forced = true;
        } else {
            // A different owner on the incoming plane's team is allied too.
            m.world.game.players[0].team = m.world.game.players[1].team;
        }
        let (s, mut timing) = strike(SupportKind::Recon);
        assert!(!m.intercept_support(&s, &mut timing, &[1]).unwrap());
        assert_eq!(m.logistics.get(1).unwrap().ammo, 1.0);
        assert!(m.support.events.is_empty());
    }
    let mut stranded = scene(&[("flak", 0, 40.0, 40.0)]);
    reserves(&mut stranded, 1, "flak");
    let record = stranded.logistics.get_mut(1).unwrap();
    record.ammo = 1.0;
    record.forced = true;
    record.stranded = true;
    let (s, mut timing) = strike(SupportKind::Recon);
    stranded.intercept_support(&s, &mut timing, &[1]).unwrap();
    assert_eq!(stranded.logistics.get(1).unwrap().ammo, 0.0);
    assert!(
        stranded
            .support
            .events
            .iter()
            .any(|event| matches!(event, SupportEvent::Flak { unit: 1, .. }))
    );
    // Exercise both outcomes from the seeded native stream, and stop after the first successful gun.
    let mut outcomes = [false; 2];
    for seed in 1..=32 {
        let mut m = scene(&[("flak", 0, 40.0, 40.0), ("flak", 0, 40.0, 40.0)]);
        m.start.random = crate::random::MatchRandom::new(seed);
        reserves(&mut m, 1, "flak");
        reserves(&mut m, 2, "flak");
        m.logistics.get_mut(1).unwrap().ammo = 1.0;
        m.logistics.get_mut(2).unwrap().ammo = 1.0;
        let (s, mut timing) = strike(SupportKind::Recon);
        let mut expected = m.start.random.clone();
        let chance = Catalog::compiled()
            .unwrap()
            .unit("flak")
            .unwrap()
            .get("aa")
            .unwrap()
            .get("chance")
            .unwrap()
            .number()
            .unwrap();
        let first_down = expected.next_fraction(StreamId::OffMap) < chance;
        outcomes[usize::from(first_down)] = true;
        m.intercept_support(&s, &mut timing, &[1, 2]).unwrap();
        assert_eq!(m.logistics.get(1).unwrap().ammo, 0.0);
        assert_eq!(
            m.logistics.get(2).unwrap().ammo,
            if first_down { 1.0 } else { 0.0 }
        );
    }
    assert_eq!(outcomes, [true, true]);
}

#[test]
fn mov67_paradrop_publication_keeps_roster_poses_and_all_grid_memberships_current() {
    let mut m = scene(&[]);
    let (s, _) = strike(SupportKind::Para);
    let first = m.next_unit_id;
    m.drop_support(&s, 0.0).unwrap();
    // Pinned sim.js:409,5261: three squads on the six-metre ring at direction 0.7.
    let expected = [
        ("rifle", Point { x: 45.0, z: 43.0 }),
        ("rifle", Point { x: 35.0, z: 43.0 }),
        ("mg", Point { x: 41.0, z: 35.0 }),
    ];
    assert_eq!(m.next_unit_id - first, 3);
    for (index, (kind, at)) in expected.iter().enumerate() {
        let id = first + index as u32;
        let unit = &m.world.game.units[&id];
        assert_eq!(m.rules.kinds[unit.kind].name, *kind);
        assert_eq!(unit.owner, s.owner);
        assert_eq!(unit.at(), *at);
        assert_eq!(
            m.world.game.flags[m.world.game.cell_of(unit.x, unit.z).unwrap()] & crate::game::MOVE,
            0
        );
        assert!(m.combat.get(id).is_some() && m.firing.get(id).is_some());
    }
    assert_eq!(m.support.events.len(), 3);
    crate::unit_placement::tests::assert_current(&m.world.game, &m.grid);
}
