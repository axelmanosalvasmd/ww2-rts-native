fn plane_frame(m: &Match, id: u32, probe: Point) -> Value {
    use crate::shared::aviation::state_name;
    let u = &m.world.game.units[&id];
    let a = &m.aviation.planes[&id];
    json!({"environment":environment(m),"id":id,"x":u.x,"z":u.z,"rot":u.rot,
        "air":{"state":state_name(u.air.unwrap()),"fuel":a.fuel,"ammo":a.ammo,"timer":a.timer,"ang":a.orbit,
            "mission":a.mission.map(|mission| json!({"kind":mission.kind.name(),"x":mission.at.x,"z":mission.at.z,"id":mission.target.unwrap_or(0)}))},
        "sees":ww2_core::vision::team_sees(&m.world.game,&m.rules,&m.grid,0,probe)})
}

#[test]
fn flying_planes_travel_and_see_through_all_five_weathers() {
    let f = fixture();
    let spec = &f["input"]["planes"];
    let traces = f["planes"].as_array().unwrap();
    assert_eq!(traces.len(), 5);
    let mut baseline: Option<(u64, Vec<bool>)> = None;
    let mut total = 0;
    for trace in traces {
        assert_eq!(trace["map"], authored_map(&f["input"], ".", 80, 80));
        let actor = &trace["setup"]["actors"][0];
        assert_eq!(actor["type"], "fighter");
        assert_eq!(actor["air"]["state"], "out");
        assert_eq!(actor["air"]["mission"], spec["mission"]);
        assert_eq!(actor["x"], spec["actor"]["x"]);
        assert_eq!(actor["z"], spec["actor"]["z"]);
        let mut m = scene(&f["input"], trace, None);
        let id = trace["setup"]["actors"][0]["id"].as_u64().unwrap() as u32;
        let count = spec["ticks"].as_u64().unwrap();
        let mut seen = Vec::new();
        assert_eq!(trace["frames"].as_array().unwrap().len(), 41);
        for n in 0..=count {
            if n > 0 {
                tick(&mut m, "flying plane");
                total += 1;
            }
            let actual = plane_frame(&m, id, point(&spec["sightProbe"]));
            compare(&actual, &trace["frames"][n as usize], "plane actual tick");
            seen.push(actual["sees"].as_bool().unwrap());
        }
        let distance = m.world.game.units[&id].x - number(&spec["actor"]["x"]);
        assert!(distance > 0.0, "actual aircraft moved");
        assert_eq!(distance.to_bits(), number(&trace["travel"]).to_bits());
        if let Some((bits, sight)) = &baseline {
            assert_eq!(*bits, distance.to_bits());
            assert_eq!(*sight, seen);
        } else {
            baseline = Some((distance.to_bits(), seen));
        }
        // Air rows remain refused by the current public snapshot provider. This
        // test uses actual aircraft and point sight, and makes no air-row claim.
        assert!(
            matches!(ww2_core::recipient_snapshot::SnapshotCache::new(&m),
            Err(ww2_core::recipient_snapshot::SnapshotError::Unsupported {
                unit, capability: ww2_core::recipient_snapshot::SnapshotCapability::Air
            }) if unit == id)
        );
    }
    assert_eq!(total, 200);
}
