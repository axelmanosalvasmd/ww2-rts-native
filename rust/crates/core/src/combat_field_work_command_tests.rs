//! CMB-147 checks command delegation, not the engineering owners' material rules.
use super::*;
use crate::field_works::{FortKind, Pattern, ProjectOrder, entrench_plan};

#[test]
fn cmb147_field_work_commands_delegate_dig_patterns_and_joins() {
    let actors = &[
        ("engineer", 0, 40., 40.),
        ("engineer", 0, 43., 40.),
        ("engineer", 1, 60., 40.),
    ];
    let mut m = scene(actors);
    m.start.economy.players[0].mp = 1000.;
    for (id, kind, at) in [
        (1, FortKind::Trench, p(40., 40.)),
        (2, FortKind::Sandbags, p(43., 40.)),
    ] {
        let mut raw = json!({"t":"dig","ids":[id,3],"x":at.x,"z":at.z,"dir":0.4});
        if kind != FortKind::Trench {
            raw["kind"] = json!(kind.as_str());
        }
        let mp = m.start.economy.players[0].mp;
        let spent = m.story[0].mp_spent;
        assert_eq!(command(&mut m, raw), None);
        let job = &m.field_works.jobs()[&id];
        assert_eq!(job.kind, kind);
        assert_eq!(job.at, at);
        assert_eq!(job.direction, 0.4);
        assert_eq!(job.project, None);
        assert!(m.world.game.units[&id].dig);
        assert!(m.start.economy.players[0].mp < mp);
        assert_eq!(
            m.story[0].mp_spent - spent,
            mp - m.start.economy.players[0].mp
        );
        assert!(!m.field_works.is_active(3));
        m.apply_field_work_unit(id).unwrap();
        assert!(m.field_works.jobs()[&id].elapsed > 0.);
        assert!(m.field_works.progress(&m.world, id).unwrap() > 0.);
    }
    let work = m.field_works.clone();
    let mp = m.start.economy.players[0].mp;
    let story = m.story.clone();
    assert_eq!(
        command(&mut m, json!({"t":"dig","ids":[3],"x":40,"z":40})),
        Some(Denial::NoBuilders)
    );
    assert_eq!(m.field_works, work);
    assert_eq!(m.start.economy.players[0].mp, mp);
    assert_eq!(m.story, story);
    assert!(m.commands.units.values().all(|c| c.orders.is_empty()));

    assert_eq!(
        command(&mut m, json!({"t":"dig","ids":[1],"x":40,"z":40,"queue":true})),
        None
    );
    let waiting = &m.commands.units[&1].orders;
    assert_eq!(waiting.len(), 1);
    assert!(matches!(
        waiting.front().unwrap(),
        Command::Dig { ids, kind: FortKind::Trench, at, direction: Some(direction), queue: false }
            if ids == &[1] && *at == p(40., 40.) && direction.is_finite()
    ));
    assert!(m.commands.units.iter().all(|(id, c)| *id == 1 || c.orders.is_empty()));
    assert_eq!(m.field_works, work);
    assert_eq!(m.start.economy.players[0].mp, mp);
    assert_eq!(m.story, story);

    // Compare dispatch with the existing planner, without duplicating its geometry.
    for (pattern, fort) in [
        (Pattern::Line, FortKind::Trench),
        (Pattern::Zigzag, FortKind::Trench),
        (Pattern::Double, FortKind::Trench),
        (Pattern::Arc, FortKind::Trench),
        (Pattern::Ring, FortKind::Trench),
        (Pattern::Strongpoint, FortKind::Trench),
        (Pattern::Line, FortKind::Sandbags),
        (Pattern::Zigzag, FortKind::Wire),
        (Pattern::Double, FortKind::Traps),
        (Pattern::Line, FortKind::Mines),
    ] {
        let mut m = scene(actors);
        m.start.economy.players[0].mp = 1000.;
        let expected =
            entrench_plan(pattern, p(40., 40.), p(56., 40.), Some(p(40., 40.)), fort).unwrap();
        assert_eq!(
            command(
                &mut m,
                json!({"t":"entrench","ids":[1],"pattern":pattern.as_str(),
                    "fort":fort.as_str(),"x":40,"z":40,"x2":56,"z2":40})
            ),
            None,
            "{pattern:?} {fort:?}"
        );
        let project = m.field_works.membership(1).unwrap();
        let plan = m.field_works.project(project).unwrap();
        assert_eq!(plan.owner, 0);
        assert_eq!(plan.fort, fort);
        assert_eq!(plan.jobs, expected, "{pattern:?} {fort:?}");
        assert_eq!(plan.crew, 1);
        assert!(m.world.game.units[&1].entrench);
        assert!(m.field_works.jobs().is_empty());
        assert_eq!(m.start.economy.players[0].mp, 1000.);
        assert_eq!(m.story[0].mp_spent, 0.);
        if fort != FortKind::Wire {
            continue;
        }

        let work = m.field_works.clone();
        let hostile_join = Command::from_json(&json!({
            "t":"entrench","ids":[3],"join":project
        }))
        .unwrap();
        assert_eq!(
            m.execute_command(1, &hostile_join, false).unwrap(),
            Some(Denial::Blocked)
        );
        for raw in [
            json!({"t":"entrench","ids":[2],"pattern":"ring","fort":"wire",
                "x":40,"z":40,"x2":56,"z2":40}),
            json!({"t":"entrench","ids":[2],"pattern":"line","fort":"nest",
                "x":40,"z":40,"x2":56,"z2":40}),
        ] {
            assert_eq!(command(&mut m, raw), Some(Denial::Blocked));
            assert_eq!(m.field_works, work);
        }
        assert_eq!(m.field_works, work);
        assert_eq!(m.start.economy.players[0].mp, 1000.);
        assert_eq!(m.field_works.membership(3), None);

        // Join selects the existing project, even with unrelated New-plan fields.
        let join = json!({"t":"entrench","ids":[2],"join":project,"queue":true,
            "pattern":"ring","fort":"nest"});
        for _ in 0..8 {
            assert_eq!(command(&mut m, join.clone()), None);
        }
        let queued = m.commands.units[&2].orders.clone();
        assert_eq!(queued.len(), 8);
        assert!(queued.iter().all(|order| matches!(
            order,
            Command::Entrench {
                ids,
                order: ProjectOrder::Join { project: selected, .. },
                queue: false,
            } if ids == &[2] && *selected == project
        )));
        assert_eq!(m.field_works, work);
        assert!(!m.world.game.units[&2].entrench);
        assert_eq!(command(&mut m, join), Some(Denial::QueueFull));
        assert_eq!(m.commands.units[&2].orders, queued);
        assert_eq!(m.field_works, work);
        assert_eq!(m.start.economy.players[0].mp, 1000.);
        assert_eq!(m.story[0].mp_spent, 0.);

        m.start_queued_orders(2, false).unwrap();
        assert_eq!(m.field_works.membership(2), Some(project));
        assert_eq!(m.field_works.project_sequence(), project);
        assert_eq!(m.field_works.project(project).unwrap().crew, 2);
        assert_eq!(m.commands.units[&2].orders.len(), 7);
        assert!(m.world.game.units[&2].entrench);
        assert!(!m.world.game.units[&2].dig);
        assert_eq!(m.start.economy.players[0].mp, 1000.);
        assert_eq!(m.story[0].mp_spent, 0.);
        let pending = m.field_works.project(project).unwrap().jobs.len();
        m.apply_project_unit(2).unwrap();
        let job = &m.field_works.jobs()[&2];
        assert_eq!(job.project, Some(project));
        assert_eq!(job.kind, FortKind::Wire);
        assert_eq!(
            m.field_works.project(project).unwrap().jobs.len(),
            pending - 1
        );
        assert!(m.world.game.units[&2].dig);
        assert!(m.start.economy.players[0].mp < 1000.);
        assert_eq!(m.story[0].mp_spent, 1000. - m.start.economy.players[0].mp);
        m.apply_field_work_unit(2).unwrap();
        assert!(m.field_works.jobs()[&2].elapsed > 0.);
        assert!(m.field_works.progress(&m.world, 2).unwrap() > 0.);
    }
}
