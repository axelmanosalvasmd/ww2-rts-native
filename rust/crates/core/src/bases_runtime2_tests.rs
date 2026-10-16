//! Purchase, coastal construction, cache and research intent checks.
use super::*;
use crate::production::Denial;

fn reject_queue(
    p: &mut ProductionState,
    w: &mut EconomyState,
    c: &Context<'_>,
    faction: Faction,
    kind: &str,
    expected: Denial,
) {
    let before = (p.clone(), w.clone());
    assert_eq!(
        p.queue_unit(c, w, 0, faction, kind, None),
        Err(Error::Denied(expected)),
        "{kind}"
    );
    assert_eq!((p.clone(), w.clone()), before);
}

#[test]
fn base17_buy_denial_order_and_queued_unique_limits() {
    for (kind, faction, foreign) in [
        ("tiger", Faction::Germany, Faction::Usa),
        ("churchill", Faction::Uk, Faction::Germany),
    ] {
        let (mut p, mut w) = component();
        let mut units = vec![view(1, 0, "motorpool"), view(2, 0, kind)];
        units.extend((3..26).map(|id| view(id, 0, "rifle")));
        w.players[0].mp = 0.;
        w.players[0].fuel = 0.;
        reject_queue(
            &mut p,
            &mut w,
            &context(&units),
            foreign,
            kind,
            Denial::Blocked,
        );
        reject_queue(
            &mut p,
            &mut w,
            &context(&units),
            faction,
            "worldbase",
            Denial::Blocked,
        );
        p = ProductionState::new(vec![TechState::new(1, 0, 0, 0).unwrap(); 3]).unwrap();
        let mut c = context(&units);
        c.naval = false;
        reject_queue(&mut p, &mut w, &c, faction, "destroyer", Denial::Needs);
        reject_queue(&mut p, &mut w, &c, faction, kind, Denial::Tier);
        p = ProductionState::new(vec![TechState::new(3, 0, 0, 0).unwrap(); 3]).unwrap();
        reject_queue(&mut p, &mut w, &c, faction, kind, Denial::Mp);
        w.players[0].mp = 10000.;
        reject_queue(&mut p, &mut w, &c, faction, kind, Denial::Fuel);
        w.players[0].fuel = 1000.;
        reject_queue(&mut p, &mut w, &c, faction, kind, Denial::Pop);
        units.truncate(2);
        reject_queue(&mut p, &mut w, &context(&units), faction, kind, Denial::Max);
        units.truncate(1);
        let charge = economy::price_of(Mode::Classic, kind).unwrap();
        p.queue_unit(&context(&units), &mut w, 0, faction, kind, Some(1))
            .unwrap();
        assert_eq!(w.players[0].mp, 10000. - charge.mp);
        assert_eq!(w.players[0].fuel, 1000. - charge.fuel);
        assert_eq!(p.building(1).unwrap().jobs()[0].charge, charge);
        reject_queue(&mut p, &mut w, &context(&units), faction, kind, Denial::Max);
    }
    let (p, mut w) = component();
    let units = [view(1, 0, "hq")];
    let mut c = context(&units);
    c.mode.kind = Mode::Conquest;
    w.players[0].mp = 0.;
    for (kind, expected) in [("engineer", Denial::Needs), ("worldbase", Denial::Blocked)] {
        let before = w.clone();
        assert_eq!(
            p.prepare_recruit(&c, &mut w, 0, Faction::Usa, kind, None)
                .err(),
            Some(Error::Denied(expected))
        );
        assert_eq!(w, before);
    }
    w.players[0].mp = 1000.;
    let purchase = p
        .prepare_recruit(&c, &mut w, 0, Faction::Usa, "rifle", Some(1))
        .unwrap()
        .commit();
    assert_eq!(purchase.request.owner, 0);
    assert_eq!(purchase.request.building, 1);
    assert_eq!(
        w.players[0].mp,
        1000. - economy::price_of(Mode::Conquest, "rifle").unwrap().mp
    );
}

fn coastal_match(naval: bool, tech: bool) -> Match {
    let rows = (0..128)
        .map(|_| format!("{}{}{}", ".".repeat(24), "W".repeat(40), ".".repeat(64)))
        .collect::<Vec<_>>();
    let coast = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Coastal bases", "w":128,"h":128,"naval":naval,"rows":rows,
            "spawns":[{"x":8,"y":8},{"x":112,"y":112}],
            "points":[{"id":"neutral","x":64,"y":64,"mp":0,"vp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut opts = options(Mode::Conquest, tech);
    opts.army = ArmySize::Endless;
    Match::new(&coast, opts).unwrap()
}

#[test]
fn base39_coastal_shipyard_builds_and_launches_its_boats() {
    for (field, value) in [
        ("cost", 150.),
        ("hpPer", 1500.),
        ("size", 3.),
        ("vision", 30.),
        ("buildTime", 30.),
    ] {
        catalog_number("shipyard", field, value);
    }
    assert_eq!(economy::tier_of("shipyard").unwrap(), 2);
    assert_eq!(
        catalog_strings("shipyard", "makes"),
        ["lcvp", "gunboat", "destroyer"]
    );
    for naval in [false, true] {
        let mut m = coastal_match(naval, false);
        assert!(
            m.world
                .game
                .flags
                .iter()
                .any(|flags| flags & (crate::game::LAND | crate::game::SHOAL) == 0)
        );
        let worker = own(&m, 0, "rifle");
        let mp = m.start.economy.players[0].mp;
        let next = m.next_unit_id;
        let inland = json!({"t":"build","kind":"shipyard","ids":[worker],"x":30,"z":36});
        denied(&mut m, 0, inland, if naval { "coast" } else { "blocked" });
        assert_eq!(m.next_unit_id, next);
        assert_eq!(m.start.economy.players[0].mp, mp);
        let command = json!({"t":"build","kind":"shipyard","ids":[worker],"x":44,"z":28});
        if !naval {
            denied(&mut m, 0, command, "blocked");
            assert_eq!(m.start.economy.players[0].mp, mp);
            continue;
        }
        assert_eq!(dispatch(&mut m, 0, command), None);
        let shipyard = next;
        assert_eq!(m.world.game.units[&shipyard].hp, 375.);
        assert_eq!(m.world.game.units[&shipyard].built, Some(0.));
        assert_eq!(m.start.economy.players[0].mp, mp - 150.);
        assert_eq!(m.production.building(shipyard).unwrap().jobs().len(), 0);
        denied(
            &mut m,
            0,
            json!({"t":"buy","unit":"lcvp","from":shipyard}),
            "needs",
        );
        for _ in 0..900 {
            m.advance_tick().unwrap();
            if m.world.game.units[&shipyard].built == Some(1.) {
                break;
            }
        }
        assert_eq!(m.world.game.units[&shipyard].built, Some(1.));
        assert_eq!(m.world.game.units[&shipyard].hp, 1500.);
        for kind in ["lcvp", "gunboat", "destroyer"] {
            let paid = m.start.economy.players[0].mp;
            let completed = m.recorded_buy(0, kind, Some(shipyard)).unwrap();
            let boat = &m.world.game.units[&completed.unit];
            assert_eq!(boat.owner, 0);
            assert_eq!(m.rules.kinds[boat.kind].name, kind);
            assert_ne!(
                m.world.game.flags
                    [(boat.z / 2.).floor() as usize * 128 + (boat.x / 2.).floor() as usize]
                    & crate::game::LAND,
                crate::game::LAND,
                "{kind} launched at {:?}",
                boat.at()
            );
            assert_eq!(
                m.start.economy.players[0].mp,
                paid - economy::price_of(Mode::Conquest, kind).unwrap().mp
            );
        }
    }
    let mut m = coastal_match(true, true);
    let worker = own(&m, 0, "rifle");
    let command = json!({"t":"build","kind":"shipyard","ids":[worker],"x":44,"z":28});
    let before = m.start.economy.players[0].mp;
    denied(&mut m, 0, command.clone(), "tier");
    assert_eq!(m.start.economy.players[0].mp, before);
    assert_eq!(dispatch(&mut m, 0, json!({"t":"tech","kind":"tier"})), None);
    for _ in 0..1201 {
        m.advance_tick().unwrap();
    }
    assert_eq!(m.production.player(0).unwrap().tech().tier(), 2);
    assert_eq!(dispatch(&mut m, 0, command), None);
}

fn nearby_cache(m: &mut Match, worker: u32) -> u32 {
    let at = m.world.game.units[&worker].at();
    let before = m.start.economy.players[0].mp;
    for dz in -2..=2 {
        for dx in -2..=2 {
            let x = crate::math::round(at.x / 2.) * 2. + f64::from(dx) * 2.;
            let z = crate::math::round(at.z / 2.) * 2. + f64::from(dz) * 2.;
            if crate::math::hypot(x - at.x, z - at.z) > 4.5 {
                continue;
            }
            let id = m.next_unit_id;
            if dispatch(
                m,
                0,
                json!({"t":"build","kind":"supplycache","ids":[worker],"x":x,"z":z}),
            )
            .is_none()
            {
                return id;
            }
            assert_eq!(m.start.economy.players[0].mp, before);
            assert_eq!(m.next_unit_id, id);
        }
    }
    panic!("no legal nearby cache site for {at:?}");
}

fn assert_construction_snapshot(m: &Match, worker: u32, site: u32) {
    let cache = crate::recipient_snapshot::SnapshotCache::new(m).unwrap();
    let actor = &m.world.game.units[&worker];
    let at = m.world.game.units[&site].at();
    for slot in 0..m.world.game.players.len() {
        let snapshot = cache.recipient(slot).unwrap();
        let units = snapshot.fields["units"].as_array().unwrap();
        let row = units.iter().find(|row| row[0] == worker);
        if slot == actor.owner {
            assert_eq!(row.unwrap()[12].as_u64().unwrap() & 128, 128);
            let plan = snapshot.fields["plans"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row[0] == worker)
                .unwrap();
            assert_eq!(
                &plan.as_array().unwrap()[..4],
                &[json!(worker), json!(8), json!(at.x), json!(at.z)]
            );
            let site_row = units.iter().find(|row| row[0] == site).unwrap();
            assert_eq!(
                site_row[14],
                json!(m.world.game.units[&site].built.unwrap())
            );
        } else {
            assert!(
                snapshot.fields["plans"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|row| row[0] != worker)
            );
            if !m.world.game.players[slot].sees(worker) {
                assert!(row.is_none());
            }
        }
    }
}

#[test]
fn shipyard_construction_survives_ticks_and_all_seat_snapshot_delivery() {
    use crate::recipient_snapshot::{DeliveryState, SelectedSeat};
    let mut m = coastal_match(true, false);
    let worker = own(&m, 0, "rifle");
    let site = m.next_unit_id;
    let mut delivery = DeliveryState::new(&m).unwrap();
    assert_eq!(
        dispatch(
            &mut m,
            0,
            json!({"t":"build","kind":"shipyard","ids":[worker],"x":44,"z":28})
        ),
        None
    );
    assert_construction_snapshot(&m, worker, site);
    let selected = (0..m.world.game.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    for _ in 0..902 {
        let report = m.advance_tick().unwrap();
        let building = m.world.game.units[&worker].build;
        if building {
            assert_construction_snapshot(&m, worker, site);
        }
        let frame = delivery
            .capture_selected_with_tick_events(&mut m, &selected, &report.events)
            .unwrap()
            .unwrap();
        for seat in &selected {
            assert_eq!(frame.recipient(seat.slot).unwrap().into_json()["t"], "s");
        }
        if !building {
            break;
        }
    }
    assert_eq!(m.world.game.units[&site].built, Some(1.));
    assert!(!m.world.game.units[&worker].build);
    assert_eq!(m.commands.units[&worker].build_target, None);
    assert!(m.tick_state.fault().is_none());
}

#[test]
fn base45_paid_cache_requires_logistics_and_serves_only_when_finished() {
    for (field, value) in [
        ("cost", 60.),
        ("hpPer", 400.),
        ("size", 2.),
        ("vision", 12.),
        ("buildTime", 12.),
    ] {
        catalog_number("supplycache", field, value);
    }
    let mut off = ordinary(false);
    let worker = own(&off, 0, "rifle");
    let mp = off.start.economy.players[0].mp;
    denied(
        &mut off,
        0,
        json!({"t":"build","kind":"supplycache","ids":[worker],"x":24,"z":24}),
        "blocked",
    );
    assert_eq!(off.start.economy.players[0].mp, mp);
    off.recorded_buy(0, "rifle", None).unwrap();
    assert!(off.start.economy.players[0].mp < 60.);
    denied(
        &mut off,
        0,
        json!({"t":"build","kind":"supplycache","ids":[worker],"x":24,"z":24}),
        "mp",
    );
    let mut opts = options(Mode::Conquest, false);
    opts.logistics = true;
    let mut m = Match::new(&map(), opts).unwrap();
    let worker = own(&m, 0, "rifle");
    let mp = m.start.economy.players[0].mp;
    let truck = m.test_supply_truck(0);
    let cache = nearby_cache(&mut m, worker);
    assert_construction_snapshot(&m, worker, cache);
    assert_eq!(m.start.economy.players[0].mp, mp - 60.);
    assert_eq!(m.world.game.units[&cache].hp, 100.);
    assert_eq!(m.world.game.units[&cache].cells.as_ref().unwrap().len(), 4);
    let convoy = m.convoys.clone();
    let control = m.commands.units[&truck].clone();
    let unit = m.world.game.units[&truck].clone();
    let wallets = m.start.economy.clone();
    assert!(!m.supply_convoys(0, &[truck], cache).unwrap());
    assert_eq!(m.convoys, convoy);
    assert_eq!(m.commands.units[&truck], control);
    assert_eq!(m.world.game.units[&truck].path, unit.path);
    assert_eq!(m.world.game.units[&truck].world_goal, unit.world_goal);
    assert_eq!(m.start.economy, wallets);
    assert_eq!(
        m.advance_tick().unwrap_err().reason,
        "reserve and convoy tick phases are not connected"
    );
    for _ in 0..241 {
        m.world.game.tick += 1;
        let mut frame = m.begin_construction_tick().unwrap();
        m.step_construction_worker(worker, &mut frame).unwrap();
        m.finish_construction_tick(frame).unwrap();
        if m.world.game.units[&worker].build {
            assert_construction_snapshot(&m, worker, cache);
        }
    }
    assert_eq!(m.world.game.units[&cache].built, Some(1.));
    assert_eq!(m.world.game.units[&cache].hp, 400.);
    assert!(
        !m.convoys
            .as_ref()
            .unwrap()
            .stores
            .iter()
            .any(|s| s.entity == Some(cache))
    );
    assert!(m.supply_convoys(0, &[truck], cache).unwrap());
    assert_eq!(m.start.economy.players[0].mp, mp - 60.);
}

fn research_context<'a>(units: &'a [UnitView<'static>], mode: Mode) -> Context<'a> {
    let mut c = context(units);
    c.mode.kind = mode;
    c.world = Some(crate::economy_state::WorldContext { regions: &[] });
    c
}

#[test]
fn base49_tiers_charge_pause_resume_and_survive_hq_loss() {
    for mode in [Mode::Conquest, Mode::Classic, Mode::World] {
        let (_, mut w) = component();
        let mut p = ProductionState::new(vec![TechState::new(1, 0, 0, 0).unwrap(); 3]).unwrap();
        let mut units = [view(1, 0, "hq"), view(2, 1, "hq")];
        for (tier, mp, mun, time) in [(2, 250., 50., 60.), (3, 400., 100., 90.)] {
            let mut c = context(&units);
            c.mode.kind = mode;
            c.world = Some(crate::economy_state::WorldContext { regions: &[] });
            let mun = if mode.is_construction() { mun } else { 0. };
            let before = w.players[0].clone();
            w.players[0].mun = mun - if mun > 0. { 1. } else { 0. };
            if mun > 0. {
                assert_eq!(
                    p.request_research(&c, &mut w, 0, Technology::Tier),
                    Err(Error::Denied(Denial::Mun))
                );
            }
            w.players[0].mun = before.mun;
            units[0].built = 0.5;
            let c = research_context(&units, mode);
            let unchanged = (p.clone(), w.clone());
            assert_eq!(
                p.request_research(&c, &mut w, 0, Technology::Tier),
                Err(Error::Denied(Denial::Needs))
            );
            assert_eq!((p.clone(), w.clone()), unchanged);
            units[0].built = 1.;
            let c = research_context(&units, mode);
            let cost = p.request_research(&c, &mut w, 0, Technology::Tier).unwrap();
            assert_eq!((cost.mp, cost.mun, cost.time), (mp, mun, time));
            assert_eq!(w.players[0].mp, before.mp - mp);
            assert_eq!(w.players[0].mun, before.mun - mun);
            let paid = w.clone();
            assert_eq!(
                p.request_research(&c, &mut w, 0, Technology::Tier),
                Err(Error::Denied(Denial::Max))
            );
            assert_eq!(w, paid);
            p.advance_research(&c, &w, 10.).unwrap();
            units[0].built = 0.5;
            let c = research_context(&units, mode);
            p.advance_research(&c, &w, 200.).unwrap();
            assert_eq!(p.player(0).unwrap().jobs()[0].progress, 10.);
            units[0].built = 1.;
            let c = research_context(&units, mode);
            p.advance_research(&c, &w, time - 10.).unwrap();
            assert_eq!(p.player(0).unwrap().tech().tier(), tier);
            units[0].hp = 0.;
            let c = research_context(&units, mode);
            p.advance_research(&c, &w, 200.).unwrap();
            assert_eq!(p.player(0).unwrap().tech().tier(), tier);
            units[0].hp = 3000.;
        }
    }
    let mut m = ordinary(true);
    for _ in 0..1400 {
        m.advance_tick().unwrap();
    }
    let hq = own(&m, 0, "hq");
    assert_eq!(dispatch(&mut m, 0, json!({"t":"tech","kind":"tier"})), None);
    let recruit = m.recorded_buy(0, "rifle", Some(hq)).unwrap();
    assert_eq!(recruit.purchase.request.building, hq);
    assert_eq!(m.world.game.units[&recruit.unit].owner, 0);
    assert_eq!(m.production.player(0).unwrap().jobs().len(), 1);
    for _ in 0..20 {
        m.advance_tick().unwrap();
    }
    assert!(m.production.player(0).unwrap().jobs()[0].progress > 0.);
}

#[test]
fn base50_armory_lines_tier_capacity_prices_and_retained_progress() {
    for mode in [Mode::Conquest, Mode::Classic, Mode::World] {
        let (_, mut w) = component();
        let mut p = ProductionState::new(vec![TechState::new(3, 0, 0, 0).unwrap(); 3]).unwrap();
        let mut units = [
            view(1, 0, "armory"),
            view(2, 0, "armory"),
            view(3, 0, "armory"),
            view(4, 1, "armory"),
        ];
        for (level, mp, mun, time) in [
            (1, 100., 25., 30.),
            (2, 175., 50., 45.),
            (3, 250., 75., 60.),
        ] {
            let mut c = context(&units);
            c.mode.kind = mode;
            c.world = Some(crate::economy_state::WorldContext { regions: &[] });
            for line in [Technology::Infantry, Technology::Guns, Technology::Armor] {
                let before = w.players[0].clone();
                let cost = p.request_research(&c, &mut w, 0, line).unwrap();
                let mun = if mode.is_construction() { mun } else { 0. };
                assert_eq!((cost.mp, cost.mun, cost.time), (mp, mun, time));
                assert_eq!(w.players[0].mp, before.mp - mp);
                assert_eq!(w.players[0].mun, before.mun - mun);
            }
            p.advance_research(&c, &w, 5.).unwrap();
            units[1].built = 0.5;
            units[2].hp = 0.;
            let c = research_context(&units, mode);
            p.advance_research(&c, &w, 5.).unwrap();
            assert_eq!(
                p.player(0)
                    .unwrap()
                    .jobs()
                    .iter()
                    .map(|j| j.progress)
                    .collect::<Vec<_>>(),
                [10., 5., 5.]
            );
            units[1].built = 1.;
            units[2].hp = 3000.;
            let c = research_context(&units, mode);
            p.advance_research(&c, &w, time).unwrap();
            assert!(p.player(0).unwrap().jobs().is_empty());
            for line in [Technology::Infantry, Technology::Guns, Technology::Armor] {
                assert_eq!(p.player(0).unwrap().tech().level(line), level);
            }
        }

        let c = research_context(&units, mode);
        let paid = w.clone();
        for line in [Technology::Infantry, Technology::Guns, Technology::Armor] {
            assert_eq!(
                p.request_research(&c, &mut w, 0, line),
                Err(Error::Denied(Denial::Max))
            );
        }
        assert_eq!(w, paid);
        let units = [view(1, 0, "armory"), view(2, 1, "armory")];
        let mut c = context(&units);
        c.mode.kind = mode;
        c.world = Some(crate::economy_state::WorldContext { regions: &[] });
        let mut p = ProductionState::new(vec![TechState::new(1, 0, 0, 0).unwrap(); 3]).unwrap();
        p.request_research(&c, &mut w, 0, Technology::Infantry)
            .unwrap();
        let paid = w.clone();
        assert_eq!(
            p.request_research(&c, &mut w, 0, Technology::Guns),
            Err(Error::Denied(Denial::QueueFull))
        );
        p.advance_research(&c, &w, 30.).unwrap();
        assert_eq!(
            p.request_research(&c, &mut w, 0, Technology::Infantry),
            Err(Error::Denied(Denial::Tier))
        );
        assert_eq!(w, paid);
        let mut p = ProductionState::new(vec![TechState::new(2, 2, 0, 0).unwrap(); 3]).unwrap();
        assert_eq!(
            p.request_research(&c, &mut w, 0, Technology::Infantry),
            Err(Error::Denied(Denial::Tier))
        );
        assert_eq!(w, paid);
    }
}
