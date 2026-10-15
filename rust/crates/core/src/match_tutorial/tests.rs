use super::*;
use crate::{
    economy_state::ArmySize, map::AuthoritativeMap, match_commands::Stance, weather::WeatherChoice,
};

fn options(mode: Mode) -> MatchOptions {
    MatchOptions {
        seed: 1821,
        mode,
        players: vec![PlayerConfig {
            name: "me".into(),
            team: 0,
            faction: Faction::Germany,
        }],
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: None,
        living_weather: false,
        map_key: None,
        tech: false,
        supply: false,
        logistics: false,
        sandbox: false,
    }
}
fn tutorial() -> Match {
    let map = AuthoritativeMap::from_json(include_bytes!(
        "../../../server/src/authoritative-maps/tutorial.json"
    ))
    .unwrap();
    Match::new_tutorial(&map, options(Mode::Tutorial)).unwrap()
}
fn ours(m: &Match) -> Vec<i64> {
    m.world
        .game
        .units
        .values()
        .filter(|u| u.owner == 0 && u.hp > 0.0 && !m.rules.kinds[u.kind].structure)
        .map(|u| i64::from(u.id))
        .collect()
}
fn command(m: &mut Match, owner: usize, command: Command) {
    assert_eq!(
        m.execute_command(owner, &command, m.finish.is_some())
            .unwrap(),
        None
    );
}
fn go(m: &mut Match, at: Point, attack: bool) {
    let orders = ours(m)
        .into_iter()
        .map(|unit| MoveOrder { unit, at })
        .collect();
    command(
        m,
        0,
        Command::Move {
            orders,
            control_ids: None,
            attack_move: attack,
            queue: false,
            together: false,
            face: None,
        },
    );
}
fn timeout_operands(m: &Match) -> String {
    let game = &m.world.game;
    let roster = game
        .units
        .values()
        .map(|unit| {
            (
                unit,
                m.rules.kinds[unit.kind].name.as_str(),
                crate::vision::in_trench(game, &m.rules, unit),
                m.field_works.membership(unit.id),
                game.cell_of(unit.x, unit.z),
            )
        })
        .collect::<Vec<_>>();
    let mut cells = BTreeSet::new();
    for at in game.units.values().map(|unit| unit.at()).chain(
        m.tutorial_state()
            .into_iter()
            .flat_map(|state| state.step().entry().spawns)
            .map(|spawn| spawn.at),
    ) {
        if let Some(cell) = game.cell_of(at.x, at.z) {
            let (x, z) = (cell % game.w, cell / game.w);
            for row in z.saturating_sub(1)..=(z + 1).min(game.h - 1) {
                for col in x.saturating_sub(1)..=(x + 1).min(game.w - 1) {
                    cells.insert(row * game.w + col);
                }
            }
        }
    }
    let terrain = cells
        .into_iter()
        .map(|cell| {
            (
                cell,
                game.chars.get(cell).copied().map(char::from),
                game.flags.get(cell),
                game.cell_hp.get(cell),
                game.trench_front.as_ref().and_then(|v| v.get(cell)),
                game.ground.as_ref().and_then(|v| v.get(cell)),
                game.objects.as_ref().and_then(|v| v.get(cell)),
                game.house.get(cell),
                game.height.as_ref().and_then(|v| v.get(cell)),
            )
        })
        .collect::<Vec<_>>();
    format!(
        "tick={} mode={:?}/{:?} finish={:?} terrain_version={}\n\
         roster(unit, kind, in_trench, project, cell)={roster:#?}\n\
         players={:#?}\neconomy={:#?}\ncontrols={:#?}\npaid_jobs_and_projects={:#?}\n\
         combat={:#?}\nfiring={:#?}\nabilities={:#?}\nrecovery={:#?}\n\
         terrain(cell, char, flags, hp, trench_front, ground, objects, house, height)={terrain:#?}\n\
         command_queue={:#?}",
        game.tick,
        m.options.mode,
        game.mode,
        m.finish,
        game.terrain_version,
        game.players,
        m.start.economy.players,
        m.commands.units,
        m.field_works,
        m.combat,
        m.firing,
        m.abilities,
        m.recovery,
        m.commands.queue.snapshot(),
    )
}
fn until(m: &mut Match, done: impl Fn(&Match) -> bool, budget: usize, said: &mut usize) {
    let mut command_results = Vec::new();
    let mut last_report = None;
    for _ in 0..budget {
        if done(m) {
            return;
        }
        let report = m
            .advance_tick()
            .unwrap_or_else(|e| panic!("Tutorial tick {}: {}", e.tick, e.reason));
        *said += report.tutorial_lines.len();
        if !report.commands.is_empty() {
            command_results.push((report.tick, report.commands.clone()));
        }
        last_report = Some(report);
        assert!(!ours(m).is_empty(), "Tutorial lost its human army");
    }
    assert!(
        done(m),
        "Tutorial did not reach its next actual goal: {:?}\n{}\n\
         observed_tick_command_results={command_results:#?}\nlast_tick_report={last_report:#?}",
        m.tutorial_state(),
        timeout_operands(m)
    );
}
fn next(m: &mut Match, step: TutorialStep, said: &mut usize) {
    until(
        m,
        |m| m.tutorial_state().unwrap().step() == step,
        6000,
        said,
    );
}

#[test]
fn mode69_tutorial_live_bridgehead_finishes_after_progression() {
    let mut m = tutorial();
    assert_eq!(m.options.players.len(), 2);
    assert_eq!(m.options.players[1].name, "Enemy");
    assert_eq!(m.options.players[1].faction, Faction::Ussr);
    assert!(
        m.building_cells.is_empty(),
        "Tutorial does not create skirmish bases"
    );
    assert_eq!(m.world.game.units.len(), 3);
    assert!(m.world.game.units.values().all(|u| u.owner == 0));
    let mut said = 0;
    // Both the actor roster and health are constructor outputs. No pose, HP,
    // point owner, phase, tape or expected frame is installed by this test.
    go(&mut m, Point { x: 31.0, z: 45.0 }, false);
    next(&mut m, TutorialStep::Orchard, &mut said);
    let sentries = m.tutorial_state().unwrap().spawned().to_vec();
    assert_eq!(sentries.len(), 2);
    for id in sentries {
        assert_eq!(m.world.game.units[&id].hp, 60.0);
        assert!(!m.commands.units[&id].auto_retreat);
    }
    go(&mut m, Point { x: 49.0, z: 45.0 }, true);
    next(&mut m, TutorialStep::Crossroads, &mut said);
    let crossroads = m.start.points[0].at;
    go(&mut m, crossroads, false);
    next(&mut m, TutorialStep::Recruit, &mut said);
    assert!(m.start.economy.players[0].mp >= 250.0);
    command(
        &mut m,
        0,
        Command::Buy {
            kind: "at".into(),
            from: None,
        },
    );
    next(&mut m, TutorialStep::Tank, &mut said);
    let tank = m.tutorial_state().unwrap().spawned()[0];
    assert!(m.world.game.units[&tank].amove.is_some());
    go(&mut m, Point { x: 83.0, z: 45.0 }, true);
    next(&mut m, TutorialStep::Trench, &mut said);
    for &id in m.tutorial_state().unwrap().spawned() {
        assert!(crate::vision::in_trench(
            &m.world.game,
            &m.rules,
            &m.world.game.units[&id]
        ));
    }
    go(&mut m, Point { x: 91.0, z: 45.0 }, true);
    next(&mut m, TutorialStep::Retreat, &mut said);
    let retreat = ours(&m)[0];
    let ids = ours(&m);
    command(
        &mut m,
        0,
        Command::Stance {
            ids,
            stance: Stance::HoldFire,
            on: true,
        },
    );
    command(&mut m, 0, Command::Retreat { ids: vec![retreat] });
    next(&mut m, TutorialStep::Artillery, &mut said);
    until(
        &mut m,
        |m| {
            let ids = m.tutorial_state().unwrap().spawned();
            ids.len() == 2
                && ids.iter().all(|id| {
                    m.world
                        .game
                        .units
                        .get(id)
                        .is_some_and(|u| u.hp > 0.0 && u.garrison.is_some())
                })
        },
        400,
        &mut said,
    );
    // Move the non-retreating survivors into observation range of the house.
    go(&mut m, Point { x: 109.0, z: 45.0 }, false);
    until(
        &mut m,
        |m| {
            m.world
                .game
                .units
                .values()
                .any(|u| u.owner == 0 && u.hp > 0.0 && u.x > 103.0)
        },
        1500,
        &mut said,
    );
    command(
        &mut m,
        0,
        Command::Support {
            kind: "artillery".into(),
            at: Point { x: 127.0, z: 35.0 },
            direction: Some(0.0),
        },
    );
    next(&mut m, TutorialStep::Bridgehead, &mut said);
    let ids = ours(&m);
    command(
        &mut m,
        0,
        Command::Stance {
            ids,
            stance: Stance::HoldFire,
            on: false,
        },
    );
    until(&mut m, |m| m.world.game.strikes.is_empty(), 1500, &mut said);
    let bridgehead = m.start.points[1].at;
    assert_ne!(m.start.points[1].owner, Some(0));
    go(&mut m, bridgehead, true);
    until(&mut m, |m| m.finish.is_some(), 6000, &mut said);
    let finish = m.finish.unwrap();
    assert_eq!(finish.winner, 0);
    assert_eq!(finish.reason, EndReason::Tutorial);
    assert_eq!(finish.end_at, bridgehead);
    assert_eq!(m.start.points[1].owner, Some(0));
    assert_eq!(m.match_result().unwrap().unwrap().mode, "tutorial");
    let report = m.completed_report(&["Commander".into()]).unwrap().unwrap();
    assert_eq!(report.result().names, vec!["Commander", "Enemy"]);
    assert_eq!(report.result().winner, 0);
    assert_eq!(report.result().reason, EndReason::Tutorial);
    assert_eq!(report.fields()["mode"], "tutorial");
    assert!(report.fields()["winVp"].is_null());
    let mut people = vec![crate::match_result::PersonHistory::default()];
    report.remember(&mut people).unwrap();
    assert_eq!(
        people[0].last_match().unwrap().outcome,
        crate::match_result::Outcome::Victory
    );
    assert_eq!(said, 18, "every step emitted both Source dialogue lines");
    let story = m.story.clone();
    let timeline = m.timeline.clone();
    for _ in 0..205 {
        m.advance_tick().unwrap();
    }
    assert_eq!(m.story, story);
    assert_eq!(m.timeline, timeline);
    assert_eq!(m.finish, Some(finish));
}

#[test]
fn tutorial_constructor_is_explicit_and_shared_ground_modes_stay_bounded() {
    let map = AuthoritativeMap::from_json(include_bytes!(
        "../../../server/src/authoritative-maps/tutorial.json"
    ))
    .unwrap();
    assert!(Match::new(&map, options(Mode::Tutorial)).is_err());
    assert!(Match::new_tutorial(&map, options(Mode::Conquest)).is_err());
    let mut empty = options(Mode::Tutorial);
    empty.players.clear();
    assert!(Match::new_tutorial(&map, empty).is_err());

    // Construct from caller options, then inspect the actual Tutorial owners.
    let mut configured = options(Mode::Tutorial);
    configured.players[0].team = 4;
    configured.shuffle = true;
    configured.tech = true;
    configured.supply = true;
    configured.logistics = true;
    let normalized = Match::new_tutorial(&map, configured).unwrap();
    assert_eq!(normalized.options.players[0].team, 0);
    assert!(!normalized.options.shuffle);
    assert!(!normalized.options.tech);
    assert!(!normalized.options.supply);
    assert!(!normalized.options.logistics);
    assert_eq!(normalized.options.mode, Mode::Tutorial);
    assert_eq!(
        normalized.world.game.mode,
        Some(crate::game::ModeKind::Tutorial)
    );
    assert!(normalized.building_cells.is_empty());
    assert!(normalized.validate_tick_state().is_ok());

    // A valid authored map with only one objective cannot host the lesson.
    let mut one_point: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../server/src/authoritative-maps/tutorial.json"
    ))
    .unwrap();
    one_point["points"].as_array_mut().unwrap().truncate(1);
    let one_point = AuthoritativeMap::from_json(&serde_json::to_vec(&one_point).unwrap()).unwrap();
    assert!(Match::new_tutorial(&one_point, options(Mode::Tutorial)).is_err());
    let mut invalid = options(Mode::Tutorial);
    invalid.players = (0..6)
        .map(|i| PlayerConfig {
            name: i.to_string(),
            team: 0,
            faction: Faction::Usa,
        })
        .collect();
    assert!(Match::new_tutorial(&map, invalid).is_err());
    let mut m = tutorial();
    assert!(m.validate_tick_state().is_ok());
    let id = ours(&m)[0];
    command(&mut m, 0, Command::Retreat { ids: vec![id] });
    m.tutorial = None;
    assert!(m.validate_tick_state().is_err());
    for mode in [Mode::Horde, Mode::Classic, Mode::Annihilation] {
        assert!(Match::new(&map, options(mode)).is_err());
    }

    // Home recruitment needs no synthetic HQ. Source ignores `from` in this
    // branch, charges only MP and places the real unit in its home region.
    let mut home = tutorial();
    let rally = Point { x: 19.0, z: 45.0 };
    home.player_rallies[0] = Some(rally);
    let wallet = home.start.economy.players[0].mp;
    let count = home.world.game.units.len();
    let next_id = home.next_unit_id;
    let bought = home.recorded_buy(0, "rifle", Some(u32::MAX)).unwrap();
    assert_eq!(bought.unit, next_id);
    assert_eq!(bought.purchase.request.building, 0);
    assert_eq!(home.world.game.units.len(), count + 1);
    assert!(home.building_cells.is_empty());
    assert_eq!(
        home.start.economy.players[0].mp,
        wallet - bought.purchase.request.spend.mp
    );
    assert_eq!(home.story[0].mp_spent, bought.purchase.request.spend.mp);
    let unit = &home.world.game.units[&bought.unit];
    assert_eq!(home.rules.kinds[unit.kind].name, "rifle");
    assert!(
        math::hypot(
            unit.x - home.start.players[0].spawn.x,
            unit.z - home.start.players[0].spawn.z
        ) < 20.0
    );
    assert!(unit.ghost > 0.0);
    assert_eq!(unit.world_goal, Some(rally));
    assert_eq!(
        home.recruit_metadata[&bought.unit].initial_rally,
        Some(crate::produced_units::RallyKind::Player)
    );
    let denied = (
        home.next_unit_id,
        home.world.game.units.len(),
        home.start.economy.players[0].mp,
    );
    assert!(matches!(
        home.recorded_buy(0, "at", None),
        Err(crate::match_runtime::RecruitError::Admission(
            crate::production::Error::Denied(crate::production::Denial::Mp)
        ))
    ));
    assert_eq!(
        (
            home.next_unit_id,
            home.world.game.units.len(),
            home.start.economy.players[0].mp
        ),
        denied
    );
    assert!(home.validate_tick_state().is_ok());

    // A real component hit supplies the injury. Recovery consumes the resulting
    // canonical health near home, paying MP through its existing tick phase.
    let target = home
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && home.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let shooter = home
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && u.id != target && home.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let source = home.world.game.units[&shooter].clone();
    let at = home.world.game.units[&target].at();
    let full = home.world.game.units[&target].hp;
    let order = home.world.game.units.keys().copied().collect::<Vec<_>>();
    let (wind_angle, wind_strength) = home.start.wind;
    let mut damage = crate::match_combat::DamageWorld {
        world: &mut home.world,
        combat: &mut home.combat,
        grid: &mut home.grid,
        rules: &home.rules,
        logistics: &home.logistics,
        production: &home.production,
        mode: home.options.mode,
        tech_enabled: home.options.tech,
        environment: &mut home.environment,
        environment_seed: &mut home.start.environment_seed,
        next_id: &mut home.next_unit_id,
        unit_order: &order,
        wind_angle,
        wind_strength,
    };
    damage
        .direct_hit(
            crate::match_combat::FlightSource {
                shooter,
                kind: source.kind,
                owner: 0,
                airborne: false,
            },
            target,
            crate::combat::Impact {
                owner: 0,
                shooter,
                weapon: crate::combat::WeaponDamage::from_catalog("rifle").unwrap(),
                damage_scale: Some(4.0),
                energy: 1.0,
                armor_piercing: false,
                ap_naval_only: false,
                ap_multiplier: 1.0,
                launch_origin: source.at(),
                at,
                direction: Point { x: 1.0, z: 0.0 },
            },
            &mut home.firing,
        )
        .unwrap();
    let injured = home.world.game.units[&target].hp;
    assert!(injured > 0.0 && injured < full);
    let paid = home.story[0].mp_spent;
    for _ in 0..200 {
        home.advance_tick().unwrap();
        if home.world.game.units[&target].hp > injured {
            break;
        }
    }
    assert!(home.world.game.units[&target].hp > injured);
    assert!(home.story[0].mp_spent > paid);

    // The original sentries are understrength constructor outputs. Walk them
    // to their real home, then verify the unattended slot never accumulates
    // reinforcement credit, even though the human home rule now uses spawns.
    let mut excluded = tutorial();
    let ids = ours(&excluded);
    command(
        &mut excluded,
        0,
        Command::Stance {
            ids,
            stance: Stance::HoldFire,
            on: true,
        },
    );
    go(&mut excluded, Point { x: 31.0, z: 45.0 }, false);
    let mut said = 0;
    next(&mut excluded, TutorialStep::Orchard, &mut said);
    let enemy = excluded.tutorial_state().unwrap().enemy();
    let sentries = excluded.tutorial_state().unwrap().spawned().to_vec();
    let ids = sentries.iter().copied().map(i64::from).collect();
    command(
        &mut excluded,
        enemy,
        Command::Stance {
            ids,
            stance: Stance::HoldFire,
            on: true,
        },
    );
    let home_at = excluded.start.players[enemy].spawn;
    command(
        &mut excluded,
        enemy,
        Command::Move {
            orders: sentries
                .iter()
                .map(|&id| MoveOrder {
                    unit: i64::from(id),
                    at: home_at,
                })
                .collect(),
            control_ids: None,
            attack_move: false,
            queue: false,
            together: false,
            face: None,
        },
    );
    until(
        &mut excluded,
        |m| {
            sentries.iter().all(|id| {
                let u = &m.world.game.units[id];
                math::hypot(u.x - home_at.x, u.z - home_at.z) < 10.0
            })
        },
        2500,
        &mut said,
    );
    for _ in 0..60 {
        excluded.advance_tick().unwrap();
        for id in &sentries {
            assert_eq!(excluded.world.game.units[id].hp, 60.0);
            assert_eq!(excluded.recovery.get(*id).unwrap().reinforcement, 0.0);
        }
    }

    let mut invalid_buy = tutorial();
    invalid_buy.tutorial = None;
    let before = (
        invalid_buy.next_unit_id,
        invalid_buy.world.game.units.len(),
        invalid_buy.start.economy.players[0].mp,
    );
    assert!(invalid_buy.recorded_buy(0, "at", None).is_err());
    assert_eq!(
        (
            invalid_buy.next_unit_id,
            invalid_buy.world.game.units.len(),
            invalid_buy.start.economy.players[0].mp
        ),
        before
    );

    // A real Tutorial purchase walks onto an authored hedge. The fault case
    // supplies malformed terrain wear, after which the committed collapse stays.
    for late_wear_error in [false, true] {
        let mut contact_options = options(Mode::Tutorial);
        contact_options.army = ArmySize::Large;
        let mut contact = Match::new_tutorial(&map, contact_options).unwrap();
        let target = Point { x: 29.0, z: 37.0 };
        let cell = contact.world.game.cell_of(target.x, target.z).unwrap();
        assert_eq!(contact.world.game.chars[cell], b'H');
        let id = contact.recorded_buy(0, "tank", None).unwrap().unit;
        command(
            &mut contact,
            0,
            Command::Move {
                orders: vec![MoveOrder {
                    unit: i64::from(id),
                    at: target,
                }],
                control_ids: None,
                attack_move: false,
                queue: false,
                together: false,
                face: None,
            },
        );
        if late_wear_error {
            contact.world.game.wear.as_mut().unwrap()[cell] = -0.25;
            contact.world.surface_wear[cell] = -0.25;
        }
        let mut reached = false;
        for _ in 0..1200 {
            let prior_dust = contact.world.game.units[&id].dust;
            let result = contact.advance_tick();
            let tick = contact.world.game.tick;
            let collapse_id = format!("collapse:{tick}:{cell}");
            let events = match &result {
                Ok(report) => &report.events,
                Err(failure) => &failure.events,
            };
            let collapses = events
                .iter()
                .filter(|event| {
                    matches!(event, crate::match_tick::TickEvent::Environment(
                        crate::environment::Effect::Collapse(c)
                    ) if c.id == collapse_id)
                })
                .count();
            if collapses == 0 {
                result.unwrap();
                continue;
            }
            reached = true;
            assert_eq!(collapses, 1);
            assert_eq!(contact.options.mode, Mode::Tutorial);
            assert_eq!(contact.world.game.mode, Some(ModeKind::Tutorial));
            assert_eq!(contact.world.game.chars[cell], b'.');
            assert!(contact.world.new_cells().iter().any(|row| row.cell == cell));
            if late_wear_error {
                let failure = result.unwrap_err();
                assert_eq!(failure.reason, "invalid ground-contact wear inputs");
                assert_eq!(contact.tick_state.fault(), Some(failure.reason.as_str()));
                assert_eq!(contact.world.game.units[&id].dust, prior_dust);
                assert_eq!(contact.world.game.wear.as_ref().unwrap()[cell], -0.25);
                let retained = format!("{:?}", contact.world);
                let random = contact.start.random.snapshot();
                let seed = contact.start.environment_seed;
                let next_id = contact.next_unit_id;
                let stopped = contact.advance_tick().unwrap_err();
                assert!(stopped.reason.starts_with("tick runner faulted:"));
                assert!(stopped.events.is_empty());
                assert_eq!(format!("{:?}", contact.world), retained);
                assert_eq!(contact.start.random.snapshot(), random);
                assert_eq!(contact.start.environment_seed, seed);
                assert_eq!(contact.next_unit_id, next_id);
            } else {
                let report = result.unwrap();
                assert!(contact.world.game.wear.as_ref().unwrap()[cell] > 0.0);
                assert_eq!(contact.world.game.units[&id].dust, Some(tick as f64));
                // The generic Environment guard accepts the actual Tutorial
                // owners and still rejects an incomplete current-unit order.
                let ids = contact.world.game.units.keys().copied().collect::<Vec<_>>();
                {
                    let mut context = crate::environment::Context {
                        tick: tick as u64,
                        seed: &mut contact.start.environment_seed,
                        next_id: &mut contact.next_unit_id,
                        unit_order: &ids,
                        grid: &contact.grid,
                        rain: contact.world.game.rain,
                        wet: contact.world.game.wet.unwrap_or(0.0),
                        wind_angle: contact.start.wind.0,
                        wind_strength: contact.start.wind.1,
                    };
                    assert!(
                        contact
                            .environment
                            .validate_context(&contact.world, &context)
                            .is_ok()
                    );
                    context.unit_order = &ids[..ids.len() - 1];
                    assert_eq!(
                        contact
                            .environment
                            .validate_context(&contact.world, &context),
                        Err(crate::environment::EnvironmentError::InvalidInput(
                            "unit order"
                        ))
                    );
                }
                // Keep the actual moved pose and prior pose. Only the contact
                // terrain and invalid weather are supplied as negative inputs.
                contact.world.set_cell(cell, b'H').unwrap();
                contact.world.game.rain = f64::NAN;
                let retained = format!("{:?}", contact.world);
                let mut effects = Vec::new();
                let dust = contact.world.game.units[&id].dust;
                let error = contact
                    .apply_moved_ground_contact_emitting(
                        id,
                        report.prior[&id],
                        dust,
                        &mut |effect| {
                            effects.push(effect);
                        },
                    )
                    .unwrap_err();
                assert_eq!(error, "moved crushing: InvalidInput(\"environment\")");
                assert!(effects.is_empty());
                assert_eq!(format!("{:?}", contact.world), retained);
            }
            break;
        }
        assert!(
            reached,
            "the purchased Tutorial tank must reach the authored hedge"
        );
    }

    // A constructor-created Tutorial performs real paid work through the same
    // recount, allocation, walking and timed Dig phases as ordinary projects.
    use crate::field_works::{FortKind, Pattern, ProjectMaterial, ProjectOrder};
    let mut project = tutorial();
    let id = ours(&project)[0];
    let before = project
        .world
        .game
        .chars
        .iter()
        .filter(|&&ch| ch == b'T')
        .count();
    command(
        &mut project,
        0,
        Command::Entrench {
            ids: vec![id],
            order: ProjectOrder::New {
                pattern: Some(Pattern::Line),
                material: ProjectMaterial::Fort(FortKind::Trench),
                from: Some(Point { x: 17.0, z: 39.0 }),
                to: Some(Point { x: 21.0, z: 39.0 }),
            },
            queue: false,
        },
    );
    let project_id = project.field_works.project_sequence();
    assert!(project_id > 0);
    assert_eq!(project.field_works.membership(id as u32), Some(project_id));
    let mut paid = false;
    for _ in 0..800 {
        let report = project.advance_tick().unwrap();
        assert!(
            report
                .phases
                .contains(&crate::match_tick::TickPhase::FieldProjects)
        );
        paid |= project.field_works.is_active(id as u32);
        if project.story[0].built > 0.0 {
            break;
        }
    }
    assert!(
        paid,
        "the real segment allocator must charge and publish a job"
    );
    assert!(project.story[0].mp_spent > 0.0);
    assert!(
        project.story[0].built > 0.0,
        "timed Dig must complete actual terrain work"
    );
    assert!(
        project
            .world
            .game
            .chars
            .iter()
            .filter(|&&ch| ch == b'T')
            .count()
            > before
    );

    // Malformed context is rejected read-only, including with no active jobs.
    for variant in 0..4 {
        let mut invalid = tutorial();
        let first = ours(&invalid)[0] as u32;
        match variant {
            0 => invalid.tutorial = None,
            1 => invalid.world.game.players[0].team = 1,
            2 => {
                invalid.world.game.flags.pop();
            }
            3 => {
                invalid.commands.units.remove(&first);
            }
            _ => unreachable!(),
        }
        let ids = invalid.world.game.units.keys().copied().collect::<Vec<_>>();
        let before = format!(
            "{:?}:{:?}:{}",
            invalid.field_works, invalid.start.economy, invalid.world.game.tick
        );
        assert!(invalid.validate_field_project_tick(&ids).is_err());
        assert_eq!(
            format!(
                "{:?}:{:?}:{}",
                invalid.field_works, invalid.start.economy, invalid.world.game.tick
            ),
            before
        );
    }
}

#[test]
fn mode93_live_tick_samples_conquest_floors_and_tutorial_territories() {
    use crate::match_end::MatchSample;
    let mut t = tutorial();
    for _ in 0..201 {
        t.advance_tick().unwrap();
    }
    assert_eq!(
        t.timeline,
        vec![
            MatchSample::Territories {
                t: 0.0,
                points: vec![0.0, 0.0],
                structures: vec![0.0, 0.0]
            },
            MatchSample::Territories {
                t: 10.0,
                points: vec![0.0, 0.0],
                structures: vec![0.0, 0.0]
            },
        ]
    );
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&serde_json::json!({
            "name":"Story clocks", "w":40,"h":40,"rows":vec![".".repeat(40);40],
            "spawns":[{"x":5,"y":5},{"x":33,"y":33}],
            "points":[{"x":19,"y":8},{"x":19,"y":30}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut config = options(Mode::Conquest);
    config.players.push(PlayerConfig {
        name: "other".into(),
        team: 2,
        faction: Faction::Usa,
    });
    let mut c = Match::new(&map, config).unwrap();
    // Declared initial scene scores exercise fractional aggregation and sparse teams.
    c.start.economy.players[0].vp = 0.75;
    c.start.economy.players[1].vp = 2.75;
    for _ in 0..201 {
        c.advance_tick().unwrap();
    }
    assert_eq!(
        c.timeline,
        vec![
            MatchSample::Conquest {
                t: 0.0,
                vp: vec![0.0, 0.0, 2.0]
            },
            MatchSample::Conquest {
                t: 10.0,
                vp: vec![0.0, 0.0, 2.0]
            },
        ]
    );
    assert!(c.finish.is_none());
    assert_eq!(c.story.iter().map(|s| s.captures).sum::<f64>(), 0.0);
}
