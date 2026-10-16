use super::*;
use crate::{field_works::{Pattern, ProjectMaterial, ProjectOrder, FortKind}, game::VBLOCK};

fn three_sides() -> Match {
    let raw = json!({
        "name":"Engineering recipients", "w":64, "h":64, "rows":vec![".".repeat(64);64],
        "spawns":[{"x":2,"y":2},{"x":61,"y":61},{"x":2,"y":61}],
        "points":[{"x":32,"y":32}], "buildings":[]
    });
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&raw).unwrap()).unwrap();
    let mut opts = options();
    opts.players[1].team = 0;
    opts.players.push(PlayerConfig { name: "enemy".into(), team: 2, faction: Faction::Uk });
    Match::new(&map, opts).unwrap()
}

fn works(m: &Match, owner: usize) -> serde_json::Value {
    SnapshotCache::new(m).unwrap().recipient(owner).unwrap().fields["works"].clone()
}

#[test]
fn eng27_pending_walking_and_dug_works_ghosts_follow_alliances_and_completion() {
    let mut m = three_sides();
    let builder = publish(&mut m, "engineer", 0, Point { x: 43., z: 51. });
    for control in m.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_retreat = false;
    }
    m.start.economy.players[0].mp = 10_000.;
    assert_eq!(m.execute_entrench(0, &[i64::from(builder)], ProjectOrder::New {
        pattern: Some(Pattern::Line), material: ProjectMaterial::Fort(FortKind::Trench),
        from: Some(Point { x: 51., z: 51. }), to: Some(Point { x: 67., z: 51. }),
    }, false, false).unwrap(), None);
    let pending = works(&m, 0);
    assert!(pending.as_array().unwrap().len() > 1);
    assert_eq!(works(&m, 1), pending, "allied team receives planned segments");
    assert_eq!(works(&m, 2), json!([]), "enemy receives no plan");
    m.apply_project_unit(builder).unwrap();
    let job = m.field_works.jobs()[&builder].clone();
    assert!(m.current_dig_goal(builder).is_some(), "first assigned segment needs an approach");
    let active = works(&m, 0);
    assert_eq!(active.as_array().unwrap().len(), pending.as_array().unwrap().len());
    assert!(active.as_array().unwrap().iter().any(|row| row[2] == job.at.x && row[3] == job.at.z));
    assert_eq!(works(&m, 1), active);
    assert_eq!(works(&m, 2), json!([]));
    let mut saw_digging = false;
    for _ in 0..4000 {
        m.advance_tick().unwrap();
        if m.field_works.jobs().get(&builder).is_some_and(|j| j.elapsed > 0.) && !saw_digging {
            saw_digging = true;
            assert!(!works(&m, 0).as_array().unwrap().is_empty());
            assert_eq!(works(&m, 1), works(&m, 0));
            assert_eq!(works(&m, 2), json!([]));
        }
        if works(&m, 0) == json!([]) { break; }
    }
    assert!(saw_digging);
    assert_eq!(works(&m, 0), json!([]));
    assert_eq!(works(&m, 1), json!([]));
    assert_eq!(works(&m, 2), json!([]));
    assert!(m.world.game.chars.contains(&b'T'), "ghost ends after real digging");
}

#[test]
fn eng63_mines_reach_allies_and_finders_without_changing_hidden_metadata_or_routes() {
    let mut m = three_sides();
    let at = Point { x: 53., z: 51. };
    let cell = m.world.game.cell_of(at.x, at.z).unwrap();
    for owner in 0..3 {
        publish(&mut m, "rifle", owner, Point { x: 51., z: 51. + owner as f64 * 2. });
    }
    m.world.set_cell(cell, b'Y').unwrap();
    m.refresh_recipient_routing(2).unwrap();
    let before = m.observed_terrain(2, true).unwrap().into_iter().find(|r| r.row.cell == cell).unwrap();
    let route_before = m.recipients[2].knowledge().cell(cell).unwrap();
    assert_ne!(route_before.flags & VBLOCK, 0);
    m.world.mutate_cell(cell, CellChange { mine: Some(true), material: Some("wood".into()), ..Default::default() }).unwrap();
    m.world.mines.insert(cell, 0);
    for owner in [0, 1] {
        let known = m.observed_terrain(owner, true).unwrap().into_iter().find(|r| r.row.cell == cell).unwrap();
        assert_eq!(known.row.ch, b'N');
        assert_eq!(known.mine_owned, Some(true));
        assert!(known.row.data.mine);
    }
    m.refresh_recipient_routing(2).unwrap();
    let hidden = m.observed_terrain(2, true).unwrap().into_iter().find(|r| r.row.cell == cell).unwrap();
    assert_eq!(hidden, before, "unfound overlay changes no recipient metadata or revision");
    let route_after = m.recipients[2].knowledge().cell(cell).unwrap();
    assert_eq!(route_after.character, route_before.character);
    assert_eq!(route_after.flags, route_before.flags);
    assert_eq!(route_after.material, route_before.material);
    assert_eq!(route_after.cell_hp, route_before.cell_hp);
    assert!(!route_after.known_mine);
    assert_ne!(route_after.flags & VBLOCK, 0, "a hidden mine cannot open the tank trap");
    m.world.mine_seen.insert(cell, 1 << 2);
    m.world.log_cell(cell, false).unwrap();
    let found = m.observed_terrain(2, true).unwrap().into_iter().find(|r| r.row.cell == cell).unwrap();
    assert_eq!(found.row.ch, b'N');
    assert_eq!(found.mine_owned, Some(false));
    assert_eq!(found.row.data.mine_material.as_deref(), Some("wood"));
}
