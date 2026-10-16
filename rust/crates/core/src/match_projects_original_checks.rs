//! Named engineering checks from pinned test.js, using actual Match components.
use super::*;
use crate::{match_commands::Denial, recipient_snapshot::SnapshotCache};

fn scene(size: usize, units: &[(u32, f64, f64)]) -> Match {
    let units = units
        .iter()
        .map(|&(id, x, z)| (id, "rifle", x, z))
        .collect::<Vec<_>>();
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
    make(
        &json!({"map":map,"units":units,"mp":[bits(10000.),bits(10000.),bits(10000.)],"tech":false}),
    )
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
        m.prepare_unit_commands(id, false).unwrap();
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

#[test]
fn five_segment_line_pays_once_per_segment_and_completes_twenty_cells() {
    let mut m = scene_with_units(
        40,
        &[
            (1, "rifle", 36., 50.),
            (2, "rifle", 40., 50.),
            (3, "rifle", 44., 50.),
            (4, "tank", 10., 10.),
        ],
    );
    assert_eq!(
        command(
            &mut m,
            json!({"t":"entrench","pattern":"line","ids":[4],"x":20,"z":40,"x2":60,"z2":40})
        ),
        Some(Denial::NoBuilders)
    );
    assert_eq!(
        command(
            &mut m,
            json!({"t":"entrench","pattern":"toString","ids":[1,2,3],"x":20,"z":40,"x2":60,"z2":40})
        ),
        Some(Denial::Blocked)
    );
    let mp = m.start.economy.players[0].mp;
    assert_eq!(
        command(
            &mut m,
            json!({"t":"entrench","pattern":"line","ids":[1,2,3,4],"x":20,"z":40,"x2":60,"z2":40})
        ),
        None
    );
    assert_eq!(m.start.economy.players[0].mp, mp, "nothing paid up front");
    step(&mut m);
    assert_eq!(m.field_works.jobs().len(), 3);
    let assigned = m
        .field_works
        .jobs()
        .values()
        .map(|job| (job.at.x.to_bits(), job.at.z.to_bits()))
        .collect::<BTreeSet<_>>();
    assert_eq!(assigned.len(), 3, "each worker takes a different segment");
    assert!(assigned.contains(&(40_f64.to_bits(), 40_f64.to_bits())));
    assert!((1..=3).all(|id| m.world.game.units[&id].dig));
    assert!(!m.world.game.units[&4].entrench && !m.world.game.units[&4].dig);
    let projection = SnapshotCache::new(&m).unwrap().recipient(0).unwrap();
    let first = projection.fields["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[0] == 1)
        .unwrap();
    assert_ne!(first[12].as_u64().unwrap() & 1024, 0);
    assert_eq!(mp - m.start.economy.players[0].mp, 90.);
    run(&mut m, 1199);
    assert_eq!(
        m.world.game.chars.iter().filter(|&&ch| ch == b'T').count(),
        20
    );
    assert_eq!(
        mp - m.start.economy.players[0].mp,
        150.,
        "five paid segments without component income"
    );
    assert!(m.world.game.units.values().all(|u| !u.entrench && !u.dig));
    assert_eq!(works(&m), json!([]));
}

#[test]
fn strongpoint_completes_twelve_trench_and_ten_forward_wire_cells() {
    let mut m = scene(40, &[(1, 38., 54.), (2, 42., 54.)]);
    assert_eq!(
        command(
            &mut m,
            json!({"t":"entrench","pattern":"strongpoint","ids":[1,2],"x":40,"z":40,"x2":40,"z2":20})
        ),
        None
    );
    run(&mut m, 1800);
    let chars = &m.world.game.chars;
    assert_eq!(chars.iter().filter(|&&ch| ch == b'T').count(), 12);
    assert_eq!(chars.iter().filter(|&&ch| ch == b'X').count(), 10);
    assert!(
        chars.iter().position(|&ch| ch == b'X').unwrap()
            < chars.iter().position(|&ch| ch == b'T').unwrap()
    );
    assert!(m.world.game.units.values().all(|u| !u.entrench && !u.dig));
}
