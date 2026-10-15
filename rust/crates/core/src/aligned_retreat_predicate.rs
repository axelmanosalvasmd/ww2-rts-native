//! Read-only CMB-144 observations over the actual constructor and ordinary tick consumers.
use super::*;

pub(super) fn roster(m:&Match)->Value {
    json!(m.world.game.units.values().map(|u|json!({"id":u.id,"owner":u.owner,"kind":m.rules.kinds[u.kind].name,"hp":number(u.hp)})).collect::<Vec<_>>())
}
pub(super) fn selection(m:&Match)->Value {
    let ids=m.world.game.units.keys().map(|id|i64::from(*id)).collect::<Vec<_>>();
    let command=Command::Retreat{ids:ids.clone()};
    let rows=ids.iter().map(|id|json!({"id":id,"eligible":m.eligible_command_unit(*id,0,&command).is_some()})).collect::<Vec<_>>();
    assert_eq!(rows.iter().filter(|row|row["eligible"]==true).map(|row|row["id"].as_i64().unwrap()).collect::<Vec<_>>(),vec![1,2,3]);
    json!(rows)
}
pub(super) fn story(m:&Match)->Value {
    json!(m.story.iter().map(|row|json!({"kills":number(row.kills),"losses":number(row.losses),"built":number(row.built),"captures":number(row.captures),"mpSpent":number(row.mp_spent),"supportCalls":number(row.support_calls),"planesDowned":number(row.planes_downed)})).collect::<Vec<_>>())
}
pub(super) fn six_ticks(m:&Match)->Value {
    let u=&m.world.game.units[&1];
    assert_eq!(u.hp.to_bits(),(100.0_f64-30.0*0.25).to_bits(),"the full original Retreat check observes exactly six ticks, not an arbitrary later hit");
    assert_eq!(m.firing.get(1).unwrap().target_id,0);
    json!({"ticks":6,"hp":number(u.hp),"targetId":0})
}
pub(super) fn tail(m:&Match,report:&TickReport,ids:&[u32],prior_story:&Value,prior_mp:&[f64])->Value {
    let actual=m.world.game.units.keys().copied().collect::<Vec<_>>();
    assert_eq!(actual.as_slice(),ids,"no genuine actor may disappear during retreat");
    assert!(report.retired.is_empty()&&report.world_removed.is_empty());
    assert!(report.phases.contains(&crate::match_tick::TickPhase::Death));
    assert!(report.phases.contains(&crate::match_tick::TickPhase::PointCaptureIncome));
    let current=story(m);
    let balances=m.start.economy.players.iter().enumerate().map(|(owner,p)|{
        for key in ["kills","losses","captures","built","supportCalls","planesDowned"] {
            assert_eq!(prior_story[owner][key],current[owner][key],"retreat must not invent a story event");
        }
        let spent=m.story[owner].mp_spent-prior_story[owner]["mpSpent"]["value"].as_f64().unwrap();
        assert!(spent>=0.0);
        assert_eq!(p.mp.to_bits(),(prior_mp[owner]-spent+p.inc*m.rules.movement.tick).to_bits(),"ordinary income and real recorded expenditure must explain each wallet");
        json!({"owner":owner,"spent":number(spent),"mp":number(p.mp),"income":number(p.inc)})
    }).collect::<Vec<_>>();
    json!({"tick":report.tick,"rosterCount":ids.len(),"retired":0,"story":current,"balances":balances})
}
pub(super) fn completion(m:&Match,id:u32,home:Point,start_distance:f64,progress_ticks:usize)->Value {
    let u=&m.world.game.units[&id]; let c=&m.commands.units[&id];
    assert!(progress_ticks>0);
    assert!(crate::math::hypot(u.x-home.x,u.z-home.z)<start_distance);
    assert!(!u.retreating&&u.path.is_empty()&&u.world_goal.is_none());
    assert_eq!(c.outcome,Some(crate::match_commands::MoveOutcome::Arrived));
    assert!(matches!(c.move_result,Some(crate::match_commands::MoveResult::Completed{..})));
    assert!(c.orders.is_empty()&&c.attack_id==0);
    let end=c.route_end.expect("actual retreat path-provider endpoint");
    assert!(crate::math::hypot(u.x-end.x,u.z-end.z)<=0.25&&u.move_speed.abs()<0.1);
    json!({"id":id,"home":point(home),"routeEnd":point(end),"x":number(u.x),"z":number(u.z),"progressTicks":progress_ticks,"outcome":"completed","outcomeTick":c.outcome_tick,"result":format!("{:?}",c.move_result)})
}
