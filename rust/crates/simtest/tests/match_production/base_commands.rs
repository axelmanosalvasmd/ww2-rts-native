//! Actual recorded native commands and canonical births. No imported expected state.
use super::*;
use ww2_core::match_commands::Command;

fn observed(m: &Match, makers: &[u32], initial_next: u32) -> Value {
    json!({
        "tick": m.world.game.tick, "next": m.next_unit_id, "nextJob": m.production.next_job_id(),
        "mode": if m.options.mode == Mode::World { json!("world") } else { Value::Null },
        "wallets": m.start.economy.players.iter().map(|p| json!([bits(p.mp),bits(p.mun),bits(p.fuel)])).collect::<Vec<_>>(),
        "spent": m.story.iter().map(|p| bits(p.mp_spent)).collect::<Vec<_>>(),
        "personal": m.player_rallies.iter().map(|p| pos(*p)).collect::<Vec<_>>(),
        "buildings": makers.iter().map(|id| {
            let u = &m.world.game.units[id];
            let b = m.production.building(*id).unwrap();
            json!([id,u.owner,m.rules.kinds[u.kind].name,bits(u.hp),bits(u.built.unwrap()),bits(b.progress()),
                pos(m.building_rallies.get(id).copied().flatten()),b.queue().collect::<Vec<_>>(),
                b.rows().into_iter().map(|r| json!([r.job.id,r.job.kind,bits(r.job.charge.mp),bits(r.job.charge.fuel),
                    if r.status == JobStatus::Active { "active" } else { "waiting" }])).collect::<Vec<_>>()])
        }).collect::<Vec<_>>(),
        "actors": m.world.game.units.values().map(|u| json!([u.id,
            if u.owner == LOCAL_DEFENDER_OWNER { -1 } else { i32::try_from(u.owner).unwrap() },
            m.rules.kinds[u.kind].name,bits(u.hp),bits(u.x),bits(u.z),u.cells])).collect::<Vec<_>>(),
        "births": m.world.game.units.values().filter(|u| u.id >= initial_next).map(|u| {
            let c = &m.commands.units[&u.id];
            json!([u.id,u.owner,m.rules.kinds[u.kind].name,
                [u.x,u.z,u.rot,c.aim,u.ghost].into_iter().map(bits).collect::<Vec<_>>(),
                pos(u.world_goal),pos(c.route_end),u.path.iter().map(|p| pos(Some(*p))).collect::<Vec<_>>(),
                u.drift,c.auto_cast,c.auto_retreat,m.grid.in_order(&m.world.game.units,(u.x,u.z),12.0,|_,_|true)])
        }).collect::<Vec<_>>(),
    })
}

fn replay(name: &str) -> Match {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/base-command-consumers.json");
    let f: Value = serde_json::from_str(&std::fs::read_to_string(path).expect(
        "Root must authorize original Source recording and fixture freeze before these tests",
    ))
    .unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    assert_eq!(
        f["scope"],
        "Five original native-command component scenes. Training helpers are not full ticks. No Classic, Horde, wire or whole-row equivalence claim."
    );
    assert_eq!(f["cases"].as_array().unwrap().len(), 5);
    let case = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap();
    let mut m = make(&case["input"]);
    let initial_next = m.next_unit_id;
    assert_eq!(json!(initial_next), case["initialNext"]);
    let makers = m
        .world
        .game
        .units
        .values()
        .filter(|u| u.owner != LOCAL_DEFENDER_OWNER && m.production.building(u.id).is_some())
        .map(|u| u.id)
        .collect::<Vec<_>>();
    assert_eq!(json!(makers), case["makers"]);
    assert_eq!(observed(&m, &makers, initial_next), case["initial"]);
    let expected_count = match name {
        "world-five-slots-invalid-makers" => 14,
        "world-waiter-cancel-preserves-head" => 12,
        "world-real-trained-rifle-rally" => 6,
        "conquest-personal-rally-real-birth" => 9,
        "conquest-building-rally-selection" => 7,
        _ => panic!("unregistered command scene"),
    };
    assert_eq!(case["records"].as_array().unwrap().len(), expected_count);
    for (index, record) in case["records"].as_array().unwrap().iter().enumerate() {
        let before = observed(&m, &makers, initial_next);
        match record["op"].as_str().unwrap() {
            "command" => {
                let command = Command::from_json(&record["cmd"]).unwrap();
                let slot = record["slot"].as_u64().unwrap() as usize;
                let result = m.execute_command(slot, &command, false).unwrap();
                assert_eq!(
                    json!(result.map(|d| d.name())),
                    record["result"],
                    "{name}:{index}"
                );
                if !record["result"].is_null() {
                    assert_eq!(observed(&m, &makers, initial_next), before);
                }
            }
            "train" => {
                let maker = record["maker"].as_u64().unwrap() as u32;
                let dt = record["dt"].as_f64().unwrap();
                assert_eq!(m.world.game.units[&maker].owner, 0);
                m.step_training(maker, dt).unwrap();
            }
            _ => panic!("unexpected Source action"),
        }
        assert_eq!(
            observed(&m, &makers, initial_next),
            record["expected"],
            "{name}:{index}"
        );
    }
    m
}

#[test]
fn base20_real_world_queue_five_slots_and_explicit_maker_inputs() {
    let m = replay("world-five-slots-invalid-makers");
    let hq = resolve(&m, 0, "hq");
    assert_eq!(m.production.building(hq).unwrap().jobs().len(), 5);
}

#[test]
fn base22_real_waiter_cancel_refund_progress_identity_and_promotion() {
    let m = replay("world-waiter-cancel-preserves-head");
    let hq = resolve(&m, 0, "hq");
    assert_eq!(m.production.building(hq).unwrap().jobs().len(), 1);
    assert_eq!(m.production.building(hq).unwrap().progress(), 0.0);
}

#[test]
fn base21_real_world_rifle_birth_timing_and_commanded_building_rally() {
    let m = replay("world-real-trained-rifle-rally");
    let hq = resolve(&m, 0, "hq");
    assert!(m.production.building(hq).unwrap().jobs().is_empty());
    assert_eq!(m.building_rallies[&hq], Some(Point { x: 57.0, z: 33.0 }));
}

#[test]
fn base23_personal_rally_refusals_and_actual_recruit_endpoint() {
    let m = replay("conquest-personal-rally-real-birth");
    assert_eq!(m.player_rallies[0], Some(Point { x: 41.0, z: 41.0 }));
}

#[test]
fn base23_building_rally_selection_and_actual_recruit_endpoint() {
    let m = replay("conquest-building-rally-selection");
    assert_eq!(m.player_rallies[0], None);
}

#[test]
fn base22_cancel_story_preflight_failure_preserves_paid_jobs_and_wallet() {
    let old = fixture();
    let input = old["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| !c["map"]["world"].is_null())
        .unwrap();
    let mut m = make(input);
    let hq = resolve(&m, 0, "hq");
    for _ in 0..2 {
        let command = Command::from_json(&json!({"t":"buy","unit":"rifle","from":hq})).unwrap();
        assert_eq!(m.execute_command(0, &command, false).unwrap(), None);
    }
    let job = m.production.building(hq).unwrap().jobs()[1].id;
    // Fault injection removes bookkeeping, never an actor or a paid job. This
    // verifies the transaction's failure boundary, not a valid Source state.
    m.story.clear();
    preserves(
        &mut m,
        0,
        json!({"t":"cancelProduction","id":hq,"job":job}),
        Err(()),
    );
}

#[test]
fn base22_world_recorded_cancel_and_rally_reach_the_actual_tick_boundary() {
    let old = fixture();
    let input = old["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| !c["map"]["world"].is_null())
        .unwrap();
    let mut m = make(input);
    let hq = resolve(&m, 0, "hq");
    for _ in 0..2 {
        let buy = Command::from_json(&json!({"t":"buy","unit":"rifle","from":hq})).unwrap();
        assert_eq!(m.execute_command(0, &buy, false).unwrap(), None);
    }
    let jobs = m.production.building(hq).unwrap().jobs().to_vec();
    let spent = m.story[0].mp_spent;
    for (index, raw) in [
        json!({"t":"rally","id":hq,"x":57,"z":33}),
        json!({"t":"cancelProduction","id":hq,"job":jobs[1].id}),
    ]
    .into_iter()
    .enumerate()
    {
        m.enqueue_command(
            ww2_core::orders::OrderKey {
                tick: 1,
                issuer: 0,
                sequence: index as u64 + 1,
            },
            Command::from_json(&raw).unwrap(),
        )
        .unwrap();
    }
    let report = m.advance_tick().unwrap();
    assert_eq!(report.tick, 1);
    assert_eq!(report.commands.len(), 2);
    assert!(report.commands.iter().all(|c| c.denial.is_none()));
    for phase in [
        ww2_core::match_tick::TickPhase::Commands,
        ww2_core::match_tick::TickPhase::Production,
    ] {
        assert!(report.phases.contains(&phase));
        assert!(!report.unavailable.contains(&phase));
    }
    let remaining = m.production.building(hq).unwrap();
    assert_eq!(remaining.jobs(), &jobs[..1]);
    assert_eq!(remaining.progress(), m.rules.movement.tick);
    assert_eq!(m.story[0].mp_spent, spent - jobs[1].charge.mp);
    assert_eq!(m.building_rallies[&hq], Some(Point { x: 57.0, z: 33.0 }));
}

#[derive(Debug, PartialEq)]
struct JournalState {
    cells: Vec<ww2_core::world::CellRow>,
    new_cells: Vec<ww2_core::world::CellRow>,
    journal: Vec<usize>,
    base: u64,
    end: u64,
    pending: Vec<Option<std::collections::BTreeSet<usize>>>,
}
fn journals(m: &Match) -> JournalState {
    use ww2_core::world::Recipient;
    JournalState {
        cells: m.world.cell_log().to_vec(),
        new_cells: m.world.new_cells().to_vec(),
        journal: m.world.journal().to_vec(),
        base: m.world.journal_base(),
        end: m.world.journal_end().unwrap(),
        pending: (0..m.world.game.players.len())
            .map(Recipient::Player)
            .chain(std::iter::once(Recipient::Spectators))
            .map(|r| m.world.pending(r).cloned())
            .collect(),
    }
}
fn preserves(
    m: &mut Match,
    slot: usize,
    raw: Value,
    expected: Result<Option<ww2_core::match_commands::Denial>, ()>,
) {
    let makers = m
        .world
        .game
        .units
        .values()
        .filter(|u| u.owner != LOCAL_DEFENDER_OWNER && m.production.building(u.id).is_some())
        .map(|u| u.id)
        .collect::<Vec<_>>();
    let initial_next = m.next_unit_id;
    let projection = observed(m, &makers, initial_next);
    let production = m.production.clone();
    // Keep wallet bits exact even for explicit malformed-wallet fault injection.
    let wallet = |m: &Match| {
        m.start
            .economy
            .players
            .iter()
            .map(|p| {
                json!([
                    p.team,
                    p.away,
                    p.out,
                    bits(p.mp),
                    bits(p.mun),
                    bits(p.fuel),
                    bits(p.vp),
                    bits(p.inc),
                    p.fuel_inc.map(bits),
                    p.upkeep.map(bits),
                ])
            })
            .collect::<Vec<_>>()
    };
    let wallets = wallet(m);
    let story = m.story.clone();
    let control = m.commands.units.clone();
    let rallies = m.building_rallies.clone();
    let logs = journals(m);
    let command = Command::from_json(&raw).unwrap();
    assert_eq!(
        m.execute_command(slot, &command, false).map_err(|_| ()),
        expected
    );
    assert_eq!(observed(m, &makers, initial_next), projection);
    assert_eq!(m.production, production);
    assert_eq!(wallet(m), wallets);
    assert_eq!(m.story, story);
    assert_eq!(m.commands.units, control);
    assert_eq!(m.building_rallies, rallies);
    assert_eq!(journals(m), logs);
}
fn actual(world: bool) -> Match {
    let old = fixture();
    let input = old["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| !c["map"]["world"].is_null() == world)
        .unwrap();
    make(input)
}
#[test]
fn base20_actual_world_queue_and_conquest_recruit_preflight_failures_preserve_state() {
    use ww2_core::match_commands::Denial;
    for world in [false, true] {
        let mut m = actual(world);
        let hq = resolve(&m, 0, "hq");
        // A real route is prepared by the Conquest buy before its story token.
        let rally = json!({"t":"rally","id":hq,"x":41,"z":41});
        assert_eq!(
            m.execute_command(0, &Command::from_json(&rally).unwrap(), false)
                .unwrap(),
            None
        );
        let foreign = resolve(&m, 1, "hq");
        preserves(
            &mut m,
            1,
            json!({"t":"buy","unit":"rifle","from":hq}),
            Ok(Some(if world {
                Denial::QueueFull
            } else {
                Denial::Needs
            })),
        );
        preserves(
            &mut m,
            0,
            json!({"t":"buy","unit":"rifle","from":foreign}),
            Ok(Some(if world {
                Denial::QueueFull
            } else {
                Denial::Needs
            })),
        );
        m.start.economy.players[0].mp = 0.0;
        preserves(
            &mut m,
            0,
            json!({"t":"buy","unit":"rifle","from":hq}),
            Ok(Some(Denial::Mp)),
        );
        m.start.economy.players[0].mp = 5000.125;
        m.story.clear();
        preserves(
            &mut m,
            0,
            json!({"t":"buy","unit":"rifle","from":hq}),
            Err(()),
        );
    }
}
#[test]
fn base22_actual_waiter_wallet_failure_and_nonzero_index_stored_refunds() {
    let mut m = actual(true);
    let hq = resolve(&m, 0, "hq");
    for kind in ["rifle", "engineer", "rifle", "engineer"] {
        let buy = Command::from_json(&json!({"t":"buy","unit":kind,"from":hq})).unwrap();
        assert_eq!(m.execute_command(0, &buy, false).unwrap(), None);
    }
    let paid = m.production.building(hq).unwrap().jobs().to_vec();
    assert_ne!(paid[0].charge, paid[1].charge);
    m.step_training(hq, 5.0).unwrap();
    preserves(
        &mut m,
        1,
        json!({"t":"cancelProduction","id":hq,"job":paid[2].id}),
        Ok(Some(ww2_core::match_commands::Denial::Unseen)),
    );
    let original_mp = m.start.economy.players[0].mp;
    // Corrupt wallet data only. No charge, actor, progress or RNG is replaced.
    m.start.economy.players[0].mp = f64::NAN;
    preserves(
        &mut m,
        0,
        json!({"t":"cancelProduction","id":hq,"job":paid[2].id}),
        Err(()),
    );
    m.start.economy.players[0].mp = original_mp;
    for (index, job) in [(2, &paid[2]), (1, &paid[1])] {
        assert_eq!(m.production.building(hq).unwrap().jobs()[index].id, job.id);
        let before = m.start.economy.players[0].clone();
        let spent = m.story[0].mp_spent;
        let logs = journals(&m);
        let cancel =
            Command::from_json(&json!({"t":"cancelProduction","id":hq,"job":job.id})).unwrap();
        assert_eq!(m.execute_command(0, &cancel, false).unwrap(), None);
        assert_eq!(
            m.start.economy.players[0].mp.to_bits(),
            (before.mp + job.charge.mp).to_bits()
        );
        assert_eq!(
            m.start.economy.players[0].fuel.to_bits(),
            (before.fuel + job.charge.fuel).to_bits()
        );
        assert_eq!(
            m.start.economy.players[0].mun.to_bits(),
            before.mun.to_bits()
        );
        assert_eq!(
            m.story[0].mp_spent.to_bits(),
            (spent - job.charge.mp).to_bits()
        );
        let b = m.production.building(hq).unwrap();
        assert_eq!(b.jobs()[0], paid[0]);
        assert_eq!(b.progress(), 5.0);
        assert_eq!(journals(&m), logs);
    }
    assert_eq!(
        m.production.building(hq).unwrap().jobs(),
        &[paid[0].clone(), paid[3].clone()]
    );
}
#[test]
fn base23_actual_multi_maker_rally_preflights_all_selected_rows() {
    let mut m = actual(false);
    let hq = resolve(&m, 0, "hq");
    let barracks = resolve(&m, 0, "barracks");
    let foreign = resolve(&m, 1, "hq");
    // A foreign valid maker is ignored. It never receives the selected rally.
    let foreign_rally = m.building_rallies[&foreign];
    let raw = json!({"t":"rally","ids":[hq,foreign,barracks],"x":20,"z":18});
    assert_eq!(
        m.execute_command(0, &Command::from_json(&raw).unwrap(), false)
            .unwrap(),
        None
    );
    assert_eq!(m.building_rallies[&hq], Some(Point { x: 20.0, z: 18.0 }));
    assert_eq!(m.building_rallies[&barracks], m.building_rallies[&hq]);
    assert_eq!(m.building_rallies[&foreign], foreign_rally);
    // Missing metadata on the later owned maker must not change the first one.
    m.building_rallies.remove(&barracks);
    preserves(
        &mut m,
        0,
        json!({"t":"rally","ids":[hq,barracks],"x":31,"z":27}),
        Err(()),
    );
}

#[test]
fn base21_world_recorded_paid_queue_cancel_and_rally_run_through_actual_birth() {
    use ww2_core::{match_tick::TickPhase, orders::OrderKey};
    let mut m = actual(true);
    let hq = resolve(&m, 0, "hq");
    let initial_next = m.next_unit_id;
    for (sequence, raw) in [
        json!({"t":"rally","id":hq,"x":57,"z":33}),
        json!({"t":"buy","unit":"rifle","from":hq}),
        json!({"t":"buy","unit":"engineer","from":hq}),
        json!({"t":"buy","unit":"rifle","from":hq}),
    ]
    .into_iter()
    .enumerate()
    {
        m.enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: sequence as u64 + 1,
            },
            Command::from_json(&raw).unwrap(),
        )
        .unwrap();
    }
    let report = m.advance_tick().unwrap();
    assert_eq!(report.commands.len(), 4);
    assert!(report.commands.iter().all(|c| c.denial.is_none()));
    assert_eq!(m.next_unit_id, initial_next);
    let jobs = m.production.building(hq).unwrap().jobs().to_vec();
    assert_eq!(jobs.len(), 3);
    assert_eq!(
        jobs.iter().map(|j| j.kind.as_str()).collect::<Vec<_>>(),
        ["rifle", "engineer", "rifle"]
    );
    let spent = m.story[0].mp_spent;
    m.enqueue_command(
        OrderKey {
            tick: 2,
            issuer: 0,
            sequence: 5,
        },
        Command::from_json(&json!({"t":"cancelProduction","id":hq,"job":jobs[2].id})).unwrap(),
    )
    .unwrap();
    let report = m.advance_tick().unwrap();
    assert_eq!(report.commands.len(), 1);
    assert_eq!(report.commands[0].denial, None);
    assert_eq!(m.production.building(hq).unwrap().jobs(), &jobs[..2]);
    assert_eq!(
        m.story[0].mp_spent.to_bits(),
        (spent - jobs[2].charge.mp).to_bits()
    );
    // This is a bounded wait for a genuine queued birth, not a clock rewrite or
    // a synthetic Classic deadline. Every interval executes the full World tick.
    let train = Catalog::compiled()
        .unwrap()
        .unit("rifle")
        .unwrap()
        .get("train")
        .and_then(ww2_core::catalog::Value::number)
        .unwrap();
    let bound = (train / m.rules.movement.tick) as usize + 4;
    let mut birth_tick = None;
    for _ in 0..bound {
        let report = m.advance_tick().unwrap();
        for phase in [TickPhase::Commands, TickPhase::Production] {
            assert!(report.phases.contains(&phase));
            assert!(!report.unavailable.contains(&phase));
        }
        if m.next_unit_id > initial_next {
            birth_tick = Some(report.tick);
            break;
        }
    }
    assert!(
        birth_tick.is_some(),
        "actual paid rifle did not emerge within its rules-derived full-tick bound"
    );
    assert_eq!(m.next_unit_id, initial_next + 1);
    let rifle = &m.world.game.units[&initial_next];
    assert_eq!(rifle.owner, 0);
    assert_eq!(m.rules.kinds[rifle.kind].name, "rifle");
    assert_eq!(
        m.commands.units[&initial_next].route_end,
        Some(Point { x: 57.0, z: 33.0 })
    );
    assert_eq!(rifle.path.last(), Some(&Point { x: 57.0, z: 33.0 }));
    assert_eq!(m.production.building(hq).unwrap().jobs(), &jobs[1..2]);
    assert_eq!(m.production.building(hq).unwrap().progress(), 0.0);
}
