//! Positive observations for genuine Retreat and the separate ordinary-Unload case.
//! These assertions never mutate Match, commands, actors or consumed inputs.
use super::*;

pub(super) fn retreat_hit(report:&TickReport,tank:u32,rifle:u32)->Value {
    let contacts=report.events.iter().filter_map(|event|match event {
        TickEvent::Firing(crate::match_firing::LaunchEvent::Contact(contact))
            if contact.shooter==tank && contact.target==Some(rifle) && contact.hit => Some(contact),
        _=>None,
    }).collect::<Vec<_>>();
    assert_eq!(contacts.len(),1,"first HP loss requires one actual tank-to-rifle contact in this tick");
    let hit=contacts[0];
    assert_eq!(hit.owner,1);
    assert_eq!(hit.target_owner,Some(0));
    assert!(!hit.killed);
    assert!(!report.retired.iter().any(|retired|retired.unit.id==rifle));
    json!({"tick":report.tick,"flight":hit.flight,"sequence":hit.sequence,"contactId":hit.contact_id,"shooter":hit.shooter,"target":hit.target,"hit":hit.hit,"killed":hit.killed})
}

pub(super) fn unloaded_scuttle(m:&Match,report:&TickReport,tank:u32,site:u32,passenger:u32,prior_story:&[crate::match_story::PlayerStory],prior_ids:&[u32])->Value {
    let tank_loss=report.retired.iter().filter(|loss|loss.unit.id==tank).collect::<Vec<_>>();
    let site_loss=report.retired.iter().filter(|loss|loss.unit.id==site).collect::<Vec<_>>();
    assert_eq!(tank_loss.len(),1);
    assert_eq!(site_loss.len(),1);
    assert_eq!(report.retired.len(),2,"no additional actor dies in the independent unloaded case");
    assert!(report.world_removed.is_empty());
    assert_eq!(m.world.game.units.keys().copied().collect::<Vec<_>>(),prior_ids.iter().copied().filter(|id|*id!=tank&&*id!=site).collect::<Vec<_>>());
    assert_eq!(m.rules.kinds[tank_loss[0].unit.kind].name,"tank");
    assert_eq!(m.rules.kinds[site_loss[0].unit.kind].name,"motorpool");
    assert_eq!(tank_loss[0].unit.owner,0);
    assert_eq!(site_loss[0].unit.owner,0);
    let wrecks=m.world.game.wrecks.iter().filter(|wreck|wreck.id==Some(tank)).collect::<Vec<_>>();
    assert_eq!(wrecks.len(),1,"actual tank loss creates one identity-bound hull");
    assert!(wrecks[0].x.is_finite()&&wrecks[0].z.is_finite());
    let rider=&m.world.game.units[&passenger];
    assert!(rider.hp>0.0&&!rider.riding);
    assert_eq!(m.combat.get(passenger).unwrap().carrier,None);
    assert!(!report.retired.iter().any(|loss|loss.unit.id==passenger));
    assert!(m.abilities.get(tank).is_none()&&m.recovery.get(tank).is_none());
    assert!(m.abilities.get(site).is_none()&&m.recovery.get(site).is_none());
    assert!(m.production.building(site).is_none());
    assert_eq!(m.story.len(),prior_story.len());
    for (owner,(now,prior)) in m.story.iter().zip(prior_story).enumerate() {
        assert_eq!(now.losses,prior.losses+if owner==0 {2.0} else {0.0});
        assert_eq!(now.kills,prior.kills,"friendly scuttle awards no enemy kill");
        assert_eq!(now.built,prior.built);
        assert_eq!(now.captures,prior.captures);
        assert_eq!(now.mp_spent,prior.mp_spent);
        assert_eq!(now.support_calls,prior.support_calls);
        assert_eq!(now.planes_downed,prior.planes_downed);
    }
    assert_eq!(m.fallen.structure,Some(site_loss[0].unit.at()));
    assert_eq!(m.fallen.hq,Some(site_loss[0].unit.at()));
    // Combat/firing metadata may remain for live projectiles. Do not demand its deletion.
    json!({"tick":report.tick,"tankRetirements":tank_loss.len(),"buildingRetirements":site_loss.len(),"wreckCount":wrecks.len(),"passengerAlive":true,"carrierDeathEjectionExercised":false,"ownLossDelta":2,"enemyKillDelta":0,"onlySelectedRetired":true})
}
