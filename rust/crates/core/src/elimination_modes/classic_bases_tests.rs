use super::*;

fn command(m: &mut Match, owner: usize, value: serde_json::Value) -> Option<Denial> {
    m.execute_command(owner, &Command::from_json(&value).unwrap(), false)
        .unwrap()
}

fn relocate(m: &mut Match, id: u32, at: Point) {
    let unit = m.world.game.units.get_mut(&id).unwrap();
    unit.x = at.x;
    unit.z = at.z;
    unit.path.clear();
    m.grid.update(id, unit);
}

fn paid_site(m: &mut Match, kind: &str) -> (u32, u32) {
    m.options.tech = false;
    m.start.economy.players[0].mp = 5000.0;
    m.start.economy.players[0].fuel = 5000.0;
    let worker = ids(m, 0, "engineer")[0];
    relocate(m, worker, Point { x: 40.0, z: 40.0 });
    m.advance_tick().unwrap();
    let site = m.next_unit_id;
    assert_eq!(
        command(
            m,
            0,
            json!({"t":"build","kind":kind,"ids":[worker],"x":44,"z":40})
        ),
        None
    );
    let at = m.world.game.units[&site].at();
    relocate(
        m,
        worker,
        Point {
            x: at.x - 4.5,
            z: at.z,
        },
    );
    (worker, site)
}

#[test]
fn base29_classic_paid_site_uses_only_nearby_crew_and_unlocks_training_and_prerequisites() {
    // Prototype sim.js:5375-5380,5587-5603: capture crew in reach, then finish it once.
    for crew in [1, 2] {
        let mut m = scene(Mode::Classic, &[0, 1]);
        let (worker, site) = paid_site(&mut m, "barracks");
        assert_eq!(m.world.game.units[&site].built, Some(0.0));
        assert_eq!(m.world.game.units[&site].hp, 1500.0 * 0.25);
        let at = m.world.game.units[&site].at();
        let near = add(
            &mut m,
            0,
            "engineer",
            Point {
                x: at.x + 4.5,
                z: at.z,
            },
        );
        let far = add(
            &mut m,
            0,
            "engineer",
            Point {
                x: at.x,
                z: at.z + 9.0,
            },
        );
        let workers = if crew == 2 {
            vec![worker, near, far]
        } else {
            vec![worker, far]
        };
        assert_eq!(
            command(&mut m, 0, json!({"t":"assist","ids":workers,"id":site})),
            None
        );
        assert_eq!(
            command(&mut m, 0, json!({"t":"buy","unit":"mg","from":site})),
            Some(Denial::Needs)
        );
        assert_eq!(
            command(
                &mut m,
                0,
                json!({"t":"build","kind":"motorpool","ids":[worker],"x":56,"z":40})
            ),
            Some(Denial::Needs)
        );
        let hp = m.world.game.units[&site].hp;
        m.advance_tick().unwrap();
        let rate = math::pow(f64::from(crew), 0.72) / 30.0 * m.rules.movement.tick;
        assert!((m.world.game.units[&site].built.unwrap() - rate).abs() < 1e-12);
        assert!((m.world.game.units[&site].hp - hp - 1500.0 * 0.75 * rate).abs() < 1e-9);
        assert!(m.production.building(site).unwrap().jobs().is_empty());
        // Complete through the runner, rather than changing the site's completion flag.
        for _ in 0..700 {
            m.advance_tick().unwrap();
            if m.world.game.units[&site].built == Some(1.0) {
                break;
            }
        }
        assert_eq!(m.world.game.units[&site].built, Some(1.0));
        assert!((m.world.game.units[&site].hp - 1500.0).abs() < 1e-9);
        assert_eq!(m.story[0].built, 1.0);
        assert_eq!(
            command(&mut m, 0, json!({"t":"buy","unit":"mg","from":site})),
            None
        );
        assert_eq!(
            command(
                &mut m,
                0,
                json!({"t":"build","kind":"motorpool","ids":[worker],"x":56,"z":40})
            ),
            None
        );
        let progress = m.production.building(site).unwrap().progress();
        m.advance_tick().unwrap();
        assert!(m.production.building(site).unwrap().progress() > progress);
        assert!(m.tick_state.fault().is_none());
    }
}

#[test]
fn base29_unfinished_classic_pillbox_does_not_fire_at_a_visible_enemy() {
    let mut m = scene(Mode::Classic, &[0, 1]);
    let (worker, site) = paid_site(&mut m, "pillbox");
    assert_eq!(command(&mut m, 0, json!({"t":"stop","ids":[worker]})), None);
    let at = m.world.game.units[&site].at();
    let target = add(
        &mut m,
        1,
        "rifle",
        Point {
            x: at.x + 15.0,
            z: at.z,
        },
    );
    m.commands.units.get_mut(&target).unwrap().hold_fire = true;
    m.commands.units.get_mut(&worker).unwrap().hold_fire = true;
    for _ in 0..40 {
        let report = m.advance_tick().unwrap();
        assert_eq!(m.world.game.units[&site].built, Some(0.0));
        assert_eq!(m.firing.get(site).unwrap().target_id, 0);
        assert!(report.events.iter().all(|event| !matches!(event,
            crate::match_tick::TickEvent::Firing(crate::match_firing::LaunchEvent::Flight { shooter, .. }) if *shooter == site)));
    }
    assert!(m.world.game.players[0].sees(target));
    assert!(m.world.game.units[&site].shot_at.is_none());
    // A completed control establishes that this target is actually shootable from the site.
    m.world.game.units.get_mut(&site).unwrap().built = Some(1.0);
    let mut fired = false;
    for _ in 0..40 {
        let report = m.advance_tick().unwrap();
        fired |= report.events.iter().any(|event| matches!(event,
            crate::match_tick::TickEvent::Firing(crate::match_firing::LaunchEvent::Flight { shooter, .. }) if *shooter == site));
    }
    assert!(fired);
    assert!(m.tick_state.fault().is_none());
}

#[test]
fn base56_classic_deadline_stops_sites_clears_paid_jobs_and_decays_every_producer() {
    let mut m = scene(Mode::Classic, &[0, 1]);
    let (worker, site) = paid_site(&mut m, "barracks");
    for (index, kind) in ["motorpool", "airfield", "shipyard"]
        .into_iter()
        .enumerate()
    {
        m.place_starting_building(0, kind, (30 + index * 7) * m.world.game.w + 30)
            .unwrap();
    }
    let hq = ids(&m, 0, "hq")[0];
    assert_eq!(
        command(&mut m, 0, json!({"t":"buy","unit":"rifle","from":hq})),
        None
    );
    assert_eq!(
        command(&mut m, 0, json!({"t":"buy","unit":"rifle","from":hq})),
        None
    );
    let job = m.production.building(hq).unwrap().jobs()[1].id;
    m.elimination.as_mut().unwrap().time_left = Some(m.rules.movement.tick);
    let mp_spent = m.story[0].mp_spent;
    let before_deadline = m.start.economy.players[0].mp;
    m.advance_tick().unwrap();
    assert!(m.elimination_sudden_death());
    assert!(
        (m.start.economy.players[0].mp
            - before_deadline
            - m.start.economy.players[0].inc * m.rules.movement.tick)
            .abs()
            < 1e-9
    );
    assert_eq!(m.elimination.as_ref().unwrap().time_left, Some(0.0));
    assert!(m.production.building(hq).unwrap().jobs().is_empty());
    assert_eq!(m.production.building(hq).unwrap().progress(), 0.0);
    assert_eq!(m.story[0].mp_spent, mp_spent);
    let built = m.world.game.units[&site].built;
    let before = m
        .world
        .game
        .units
        .iter()
        .filter(|(_, u)| produces(u.kind, &m.rules).unwrap())
        .map(|(&id, u)| (id, u.hp))
        .collect::<Vec<_>>();
    let wallet = m.start.economy.players[0].mp;
    let income = m.start.economy.players[0].inc;
    m.advance_tick().unwrap();
    assert_eq!(m.world.game.units[&site].built, built);
    // Income continues. Clearing paid jobs creates no refund.
    assert!((m.start.economy.players[0].mp - wallet - income * m.rules.movement.tick).abs() < 1e-9);
    for (id, hp) in before {
        let unit = &m.world.game.units[&id];
        let full = number(
            Catalog::compiled()
                .unwrap()
                .unit(&m.rules.kinds[unit.kind].name)
                .unwrap(),
            "hpPer",
        )
        .unwrap();
        assert!((hp - unit.hp - full * 0.01 * m.rules.movement.tick).abs() < 1e-9);
    }
    m.options.tech = true;
    for value in [
        json!({"t":"buy","unit":"rifle","from":hq}),
        json!({"t":"tech","kind":"tier"}),
        json!({"t":"cancelProduction","id":hq,"job":job}),
        json!({"t":"build","kind":"barracks","ids":[worker],"x":56,"z":40}),
    ] {
        assert_eq!(command(&mut m, 0, value), Some(Denial::SuddenDeath));
    }
    for _ in 0..3 {
        m.advance_tick().unwrap();
    }
    assert_eq!(m.world.game.units[&site].built, built);
    assert!(m.tick_state.fault().is_none());
}
