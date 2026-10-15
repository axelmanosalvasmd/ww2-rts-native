//! Named engineering checks from pinned test.js, using actual Match components.
use super::*;
use crate::{
    field_works::{Error, FortKind, Reason},
    match_commands::Denial,
    recipient_snapshot::SnapshotCache,
};

fn scene(size: usize, units: &[(u32, f64, f64)]) -> Match {
    let units = units.iter().map(|&(id, x, z)| (id, "rifle", x, z)).collect::<Vec<_>>();
    scene_with_units(size, &units)
}

fn scene_with_units(size: usize, units: &[(u32, &str, f64, f64)]) -> Match {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/mass-entrenchment.json")).unwrap();
    // Reuse only authored map inputs, never captured state or expected output.
    let mut map = fixture["traces"][0]["map"].clone();
    map["w"] = json!(size);
    map["h"] = json!(size);
    map["rows"] = json!(vec![".".repeat(size); size]);
    let units = units
        .iter()
        .map(|&(id, kind, x, z)| json!({"id":id,"owner":0,"type":kind,"x":bits(x),"z":bits(z)}))
        .collect::<Vec<_>>();
    make(&json!({"map":map,"units":units,"mp":[bits(10000.),bits(10000.),bits(10000.)],"tech":false}))
}

fn command(m: &mut Match, input: Value) -> Option<Denial> {
    let command = Command::from_json(&input).unwrap();
    m.execute_command(0, &command, false).unwrap()
}

fn works(m: &Match) -> Value {
    SnapshotCache::new(m).unwrap().recipient(0).unwrap().fields["works"].clone()
}

// A bounded component clock, with the same work-before-movement order as Match's
// unit loop. No income, combat, RNG replacement, actor repositioning or job edits.
fn step(m: &mut Match) {
    m.world.game.tick += 1;
    m.recount_field_projects().unwrap();
    m.begin_path_tick(4096, 20).unwrap();
    let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
    for &id in &ids {
        m.start_queued_orders(id, false).unwrap();
    }
    for id in ids {
        m.world.game.units.get_mut(&id).unwrap().repath -= 0.05;
        m.apply_project_unit(id).unwrap();
        m.apply_field_work_unit(id).unwrap();
        m.move_unit_with_command_routes(id, 1.).unwrap();
    }
}

fn run(m: &mut Match, ticks: usize) {
    for _ in 0..ticks {
        step(m);
    }
}

// Source's sight-loss check deletes its scout. Remove the native metadata for
// that authored deletion too, without changing the remaining actor's pose.
fn remove_scene_unit(m: &mut Match, id: u32) {
    m.cancel_field_work(id).unwrap();
    m.world.game.units.remove(&id);
    m.grid.remove_unit(id);
    m.commands.units.remove(&id);
    m.firing.remove(id);
    m.abilities.remove(id);
    m.recovery.remove(id);
    m.combat.remove(id);
    for player in &mut m.world.game.players {
        player.visible.retain(|visible| *visible != id);
    }
}

#[test]
fn hidden_placement_and_queued_sight_loss_do_not_reveal_or_pay() {
    let order = json!({"t":"entrench","pattern":"line","ids":[1],"x":120,"z":120,"queue":true});
    for ch in [b'.', b'K', b'W'] {
        let mut m = scene(80, &[(1, 5., 5.)]);
        // Match the original check's g.chars.fill input exactly. The check
        // requires refusal before terrain inspection, not a terrain mutation.
        m.world.game.chars.fill(ch);
        let mp = m.start.economy.players[0].mp;
        assert_eq!(command(&mut m, order.clone()), Some(Denial::NotVisible));
        assert_eq!(works(&m), json!([]));
        let result = field_works::placement(
            &m.world,
            FortKind::Trench,
            Point { x: 120., z: 120. },
            0.,
            Some(0),
            |_| false,
        );
        assert_eq!(result, Err(Error::Refused(Reason::NotVisible)));
        assert_eq!(m.start.economy.players[0].mp, mp);
    }
    let mut positive = scene(80, &[(1, 5., 5.), (2, 120., 130.)]);
    let mp = positive.start.economy.players[0].mp;
    assert_eq!(command(&mut positive, order.clone()), None);
    step(&mut positive);
    assert!(positive.world.game.units[&1].dig, "the intact scout permits actual activation");
    assert_eq!(positive.field_works.jobs().len(), 1);
    assert_eq!(mp - positive.start.economy.players[0].mp, 30.);
    assert_eq!(works(&positive).as_array().unwrap().len(), 1);

    let mut m = scene(80, &[(1, 5., 5.), (2, 120., 130.)]);
    assert_eq!(
        field_works::placement(
            &m.world,
            FortKind::Trench,
            Point { x: 120., z: 120. },
            0.,
            Some(0),
            |at| at.x < 120.,
        ),
        Err(Error::Refused(Reason::NotVisible)),
        "every cell needs sight"
    );
    assert_eq!(command(&mut m, order), None);
    assert_eq!(works(&m).as_array().unwrap().len(), 1);
    remove_scene_unit(&mut m, 2);
    let mp = m.start.economy.players[0].mp;
    step(&mut m);
    assert!(!m.world.game.units[&1].dig);
    assert!(m.field_works.jobs().is_empty());
    assert_eq!(m.start.economy.players[0].mp, mp);
    assert_eq!(works(&m), json!([]));
}

#[test]
fn five_segment_line_pays_once_per_segment_and_completes_twenty_cells() {
    let mut m = scene_with_units(40, &[
        (1, "rifle", 36., 50.),
        (2, "rifle", 40., 50.),
        (3, "rifle", 44., 50.),
        (4, "tank", 10., 10.),
    ]);
    assert_eq!(
        command(&mut m, json!({"t":"entrench","pattern":"line","ids":[4],"x":20,"z":40,"x2":60,"z2":40})),
        Some(Denial::NoBuilders)
    );
    assert_eq!(
        command(&mut m, json!({"t":"entrench","pattern":"toString","ids":[1,2,3],"x":20,"z":40,"x2":60,"z2":40})),
        Some(Denial::Blocked)
    );
    let mp = m.start.economy.players[0].mp;
    assert_eq!(
        command(&mut m, json!({"t":"entrench","pattern":"line","ids":[1,2,3,4],"x":20,"z":40,"x2":60,"z2":40})),
        None
    );
    assert_eq!(m.start.economy.players[0].mp, mp, "nothing paid up front");
    step(&mut m);
    assert_eq!(m.field_works.jobs().len(), 3);
    let assigned = m.field_works.jobs().values().map(|job| {
        (job.at.x.to_bits(), job.at.z.to_bits())
    }).collect::<BTreeSet<_>>();
    assert_eq!(assigned.len(), 3, "each worker takes a different segment");
    assert!(assigned.contains(&(40_f64.to_bits(), 40_f64.to_bits())));
    assert!((1..=3).all(|id| m.world.game.units[&id].dig));
    assert!(!m.world.game.units[&4].entrench && !m.world.game.units[&4].dig);
    let projection = SnapshotCache::new(&m).unwrap().recipient(0).unwrap();
    let first = projection.fields["units"].as_array().unwrap().iter().find(|row| row[0] == 1).unwrap();
    assert_ne!(first[12].as_u64().unwrap() & 1024, 0);
    assert_eq!(mp - m.start.economy.players[0].mp, 90.);
    run(&mut m, 1199);
    assert_eq!(m.world.game.chars.iter().filter(|&&ch| ch == b'T').count(), 20);
    assert_eq!(mp - m.start.economy.players[0].mp, 150., "five paid segments without component income");
    assert!(m.world.game.units.values().all(|u| !u.entrench && !u.dig));
    assert_eq!(works(&m), json!([]));
}

#[test]
fn strongpoint_completes_twelve_trench_and_ten_forward_wire_cells() {
    let mut m = scene(40, &[(1, 38., 54.), (2, 42., 54.)]);
    assert_eq!(
        command(&mut m, json!({"t":"entrench","pattern":"strongpoint","ids":[1,2],"x":40,"z":40,"x2":40,"z2":20})),
        None
    );
    run(&mut m, 1800);
    let chars = &m.world.game.chars;
    assert_eq!(chars.iter().filter(|&&ch| ch == b'T').count(), 12);
    assert_eq!(chars.iter().filter(|&&ch| ch == b'X').count(), 10);
    assert!(chars.iter().position(|&ch| ch == b'X').unwrap()
        < chars.iter().position(|&ch| ch == b'T').unwrap());
    assert!(m.world.game.units.values().all(|u| !u.entrench && !u.dig));
}

#[test]
fn mass_line_waits_for_manpower_and_replacement_move_leaves_it_unfinished() {
    let mut poor = scene(40, &[(1, 38., 50.), (2, 42., 50.)]);
    poor.start.economy.players[0].mp = 45.;
    assert_eq!(
        command(&mut poor, json!({"t":"entrench","pattern":"line","ids":[1,2],"x":20,"z":40,"x2":60,"z2":40})),
        None
    );
    run(&mut poor, 600);
    assert_eq!(poor.world.game.chars.iter().filter(|&&ch| ch == b'T').count(), 4);
    assert!(poor.world.game.units.values().all(|u| u.entrench && !u.dig));
    assert_eq!(poor.start.economy.players[0].mp, 15.);
    assert_eq!(poor.story[0].mp_spent, 30.);
    // The original check explicitly refills the purse after proving the wait.
    poor.start.economy.players[0].mp = 10000.;
    run(&mut poor, 1200);
    assert_eq!(poor.world.game.chars.iter().filter(|&&ch| ch == b'T').count(), 20);
    assert_eq!(poor.start.economy.players[0].mp, 9880.);
    assert_eq!(poor.story[0].mp_spent, 150.);
    assert!(poor.world.game.units.values().all(|u| !u.entrench && !u.dig));

    let mut moved = scene(40, &[(1, 40., 50.)]);
    assert_eq!(
        command(&mut moved, json!({"t":"entrench","pattern":"line","ids":[1],"x":20,"z":40,"x2":60,"z2":40})),
        None
    );
    step(&mut moved);
    assert!(moved.world.game.units[&1].dig);
    let mp = moved.start.economy.players[0].mp;
    assert_eq!(command(&mut moved, json!({"t":"move","orders":[[1,40,70]]})), None);
    assert!(!moved.field_works.is_active(1));
    assert_eq!(moved.field_works.membership(1), None);
    assert_eq!(moved.start.economy.players[0].mp, mp, "replacement does not refund payment");
    run(&mut moved, 800);
    let worker = &moved.world.game.units[&1];
    assert!(!worker.entrench && !worker.dig && worker.z > 60.);
    assert!(moved.world.game.chars.iter().filter(|&&ch| ch == b'T').count() < 20);
    assert_eq!(moved.start.economy.players[0].mp, mp);
    assert_eq!(works(&moved), json!([]));
}

#[test]
fn trap_line_executes_and_non_line_materials_are_refused() {
    let mut m = scene(40, &[(1, 40., 50.), (2, 44., 50.)]);
    assert_eq!(
        command(&mut m, json!({"t":"entrench","pattern":"line","fort":"wire","ids":[1,2],"x":25,"z":30,"x2":55,"z2":30})),
        None
    );
    assert_eq!(works(&m).as_array().unwrap().len(), 3);
    run(&mut m, 800);
    assert_eq!(m.world.game.chars.iter().filter(|&&ch| ch == b'X').count(), 15);
    assert_eq!(
        command(&mut m, json!({"t":"entrench","pattern":"line","fort":"traps","ids":[1],"x":25,"z":26,"x2":33,"z2":26})),
        None
    );
    run(&mut m, 400);
    assert_eq!(m.world.game.chars.iter().filter(|&&ch| ch == b'Y').count(), 4);
    assert_eq!(m.world.game.chars.iter().filter(|&&ch| ch == b'X').count(), 15);
    for (fort, pattern) in [("bridge", "line"), ("nest", "line"), ("toString", "line"), ("wire", "ring")] {
        let mp = m.start.economy.players[0].mp;
        let terrain = m.world.game.chars.clone();
        assert_eq!(
            command(&mut m, json!({"t":"entrench","pattern":pattern,"fort":fort,"ids":[1],"x":25,"z":20,"x2":40,"z2":20})),
            Some(Denial::Blocked),
            "{fort}/{pattern}"
        );
        assert_eq!(m.start.economy.players[0].mp, mp);
        assert_eq!(m.world.game.chars, terrain);
    }
}

#[test]
fn malformed_dig_names_do_not_start_work_or_spend_manpower() {
    let m = scene(40, &[(1, 10., 10.)]);
    let before = snapshot(&m);
    for kind in [json!({"toString":null,"valueOf":null}), json!(["trench"]), json!("toString"), json!("__proto__")] {
        assert_eq!(
            Command::from_json(&json!({"t":"dig","ids":[1],"kind":kind,"x":10,"z":10})),
            Err(Denial::Blocked)
        );
        assert_eq!(snapshot(&m), before);
        assert!(m.field_works.jobs().is_empty());
    }
}

#[test]
fn queued_dig_defers_payment_and_skips_unaffordable_work() {
    let mut m = scene(40, &[(1, 5., 5.)]);
    let mp = m.start.economy.players[0].mp;
    assert_eq!(command(&mut m, json!({"t":"move","orders":[[1,27,5]]})), None);
    // Admission stays unpaid; the stored immediate command pays on activation.
    assert_eq!(
        command(&mut m, json!({"t":"dig","ids":[1],"kind":"trench","x":27,"z":5,"dir":0,"queue":true})),
        None
    );
    assert_eq!(command(&mut m, json!({"t":"move","orders":[[1,27,27]],"queue":true})), None);
    assert_eq!(m.start.economy.players[0].mp, mp);
    assert!(!m.world.game.units[&1].dig);
    let mut started = false;
    for _ in 0..160 {
        let before = m.start.economy.players[0].mp;
        step(&mut m);
        if m.world.game.units[&1].dig {
            assert_eq!(before - m.start.economy.players[0].mp, 30.);
            started = true;
            break;
        }
        assert_eq!(m.start.economy.players[0].mp, mp);
    }
    assert!(started, "the dig activates after the leading move");
    let planned = m.field_works.jobs()[&1].cells.clone();
    run(&mut m, 400); // Source: digCells * digTime + 8 seconds.
    assert!(planned.iter().all(|cell| m.world.game.chars[cell.cell] == b'T'));
    let unit = &m.world.game.units[&1];
    assert!(crate::math::hypot(unit.x - 27., unit.z - 27.) < 1.);
    assert_eq!(mp - m.start.economy.players[0].mp, 30.);

    let mut poor = scene(40, &[(1, 5., 5.)]);
    assert_eq!(command(&mut poor, json!({"t":"move","orders":[[1,7,5]]})), None);
    assert_eq!(
        command(&mut poor, json!({"t":"dig","ids":[1],"kind":"trench","x":7,"z":5,"dir":0,"queue":true})),
        None
    );
    assert_eq!(command(&mut poor, json!({"t":"move","orders":[[1,17,5]],"queue":true})), None);
    poor.start.economy.players[0].mp = 0.;
    run(&mut poor, 80);
    let unit = &poor.world.game.units[&1];
    assert!(!unit.dig);
    assert!(poor.field_works.jobs().is_empty());
    assert!(!poor.world.game.chars.contains(&b'T'));
    assert_eq!(poor.start.economy.players[0].mp, 0.);
    assert!(crate::math::hypot(unit.x - 17., unit.z - 5.) < 1.);
    assert!(poor.commands.units[&1].orders.is_empty());
}

#[test]
fn queued_dig_losing_sight_or_ground_skips_without_payment_and_continues_move() {
    for loss in ["sight", "ground"] {
        let mut m = scene(80, &[(1, 5., 5.), (2, 120., 130.)]);
        let target = Point { x: 120., z: 120. };
        assert!(m.recipient_sees(0, target).unwrap());
        assert_eq!(
            command(&mut m, json!({"t":"dig","kind":"trench","ids":[1],"x":120,"z":120,"dir":0,"queue":true})),
            None
        );
        assert_eq!(command(&mut m, json!({"t":"move","orders":[[1,17,5]],"queue":true})), None);
        assert_eq!(m.commands.units[&1].orders.len(), 2);
        if loss == "sight" {
            remove_scene_unit(&mut m, 2);
            assert!(!m.recipient_sees(0, target).unwrap());
        } else {
            let cells = field_works::placement(&m.world, FortKind::Trench, target, 0., Some(0), |_| true).unwrap();
            assert_eq!(cells.len(), 4);
            for cell in cells {
                m.world.set_cell(cell.cell, b'W').unwrap();
            }
            assert!(m.recipient_sees(0, target).unwrap());
        }
        let mp = m.start.economy.players[0].mp;
        let spent = m.story[0].mp_spent;
        run(&mut m, 80);
        let worker = &m.world.game.units[&1];
        assert!(!worker.dig && !worker.entrench, "{loss}");
        assert!(m.field_works.jobs().is_empty(), "{loss}");
        assert!(m.commands.units[&1].orders.is_empty(), "{loss}");
        assert_eq!(m.start.economy.players[0].mp, mp, "{loss}");
        assert_eq!(m.story[0].mp_spent, spent, "{loss}");
        assert!(!m.world.game.chars.contains(&b'T'), "{loss}");
        assert!(crate::math::hypot(worker.x - 17., worker.z - 5.) < 1., "{loss}: the following move executes");
        assert_eq!(works(&m), json!([]));
    }
}
