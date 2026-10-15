//! CMB63 component inputs, independent of the unrecorded ordinary CMB42 packet.
use super::*;
use crate::{
    combat::{Impact, SuppressionInput, WeaponDamage},
    match_combat::{DamageWorld, FlightSource},
};

#[test]
fn cmb63_named_veterancy_and_actual_accuracy_damage_suppression_consumers() {
    let fixture: Value = serde_json::from_str(include_str!("../tests/fixtures/match-combat.json")).unwrap();
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&fixture["cases"][0]["map"]).unwrap()).unwrap();
    for stars in [0_u8, 2] {
        // Authored component input from before construction. Each independent
        // match starts once with the same 0.75 tape. It is never reset mid-scene.
        // 0.75 misses rifle accuracy 0.7, but hits two-star accuracy 0.84.
        let replay = Replay::new(&json!({"layout":[0,1],"tape":vec!["3fe8000000000000";16384]})).unwrap();
        let options = MatchOptions {
            seed:701, mode:Mode::Conquest,
            players:vec![PlayerConfig{name:"A".into(),team:0,faction:Faction::Usa},
                PlayerConfig{name:"B".into(),team:1,faction:Faction::Germany}],
            shuffle:false, army:ArmySize::Standard, weather:WeatherChoice::Map, weather_seed:Some(1),
            living_weather:false, map_key:None, tech:false, supply:false, logistics:false, sandbox:false,
        };
        let mut m = Match::new_aligned(&map,options,replay.clone()).unwrap();
        let rifle = |owner| m.world.game.units.values()
            .find(|u|u.owner==owner && m.rules.kinds[u.kind].name=="rifle").unwrap().id;
        let shooter = rifle(0);
        let target = rifle(1);
        assert_eq!(crate::combat::veterancy("rifle",m.combat.get(shooter).unwrap().xp).unwrap(),0);
        // The named original Veterancy check declares XP directly. Retain that
        // input and snapshot assertion; this is not an earned-XP match replay.
        let xp = if stars == 0 { 0.0 } else { 250.0 };
        for (id,x) in [(shooter,5.0),(target,15.0)] {
            let u = m.world.game.units.get_mut(&id).unwrap();
            u.x=x;u.z=21.0;
            m.combat.get_mut(id).unwrap().xp=xp;
        }
        m.grid.rebuild(&m.world.game.units);
        crate::vision::update_vision(&mut m.world.game,&m.rules,&m.grid);
        assert!(m.world.game.players[0].sees(target));
        let cache = SnapshotCache::new(&m).unwrap();
        let projection = cache.recipient(0).unwrap();
        let row = projection.fields["units"].as_array().unwrap().iter()
            .find(|r|r[0].as_u64()==Some(u64::from(shooter))).unwrap();
        assert_eq!(row[13],json!(stars),"complete named Veterancy snapshot predicate");
        let attack = Command::from_json(&json!({"t":"attack","ids":[shooter],"target":target})).unwrap();
        assert_eq!(m.execute_command(0,&attack,false).unwrap(),None);
        m.apply_ground_attack(shooter).unwrap();
        let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
        m.begin_firing_tick(&order).unwrap();
        m.apply_unit_firing(shooter,false,0.05).unwrap();
        let flights = m.firing.flights.values().filter(|f|f.shooter==shooter).collect::<Vec<_>>();
        assert_eq!(flights.len(),5,"actual per-model rifle volley");
        assert!(flights.iter().all(|f|if stars==0 { f.solution.velocity.z.abs()>0.0 }
            else { f.solution.velocity.z==0.0 }),"actual launch aim must cross the accuracy threshold");
        let receipt = replay.receipt();
        let roles = receipt["draws"].as_array().unwrap().iter()
            .map(|r|r["consumer"][0].as_str().unwrap()).collect::<Vec<_>>();
        assert_eq!(roles.iter().filter(|r|**r=="combat.accuracy").count(),5);
        assert_eq!(roles.iter().filter(|r|**r=="combat.offset.magnitude").count(),if stars==0 {5}else{0});
        assert_eq!(roles.iter().filter(|r|**r=="combat.offset.sign").count(),if stars==0 {5}else{0});
        let before = m.world.game.units[&target].hp;
        let kind = m.rules.kind("rifle").unwrap();
        let from = m.world.game.units[&shooter].at();
        let at = m.world.game.units[&target].at();
        let impact = Impact { owner:0, shooter, weapon:WeaponDamage::from_catalog("rifle").unwrap(),
            damage_scale:None, energy:1.0, armor_piercing:false, ap_naval_only:false,
            ap_multiplier:1.5, launch_origin:from, at, direction:crate::game::Point{x:1.0,z:0.0} };
        let mut damage = DamageWorld {
            world:&mut m.world, combat:&mut m.combat, grid:&mut m.grid, rules:&m.rules,
            logistics:&m.logistics, production:&m.production, mode:Mode::Conquest, tech_enabled:false,
            environment:&mut m.environment, environment_seed:&mut m.start.environment_seed,
            next_id:&mut m.next_unit_id, unit_order:&order, wind_angle:0.0, wind_strength:0.5,
        };
        let taken = damage.direct_hit(FlightSource{shooter,kind,owner:0,airborne:false},target,impact,&mut m.firing).unwrap();
        let expected = 3.0*(1.0-0.08*f64::from(stars));
        assert!((taken-expected).abs()<1e-9);
        damage.suppress(target,SuppressionInput { owner:0,shooter,amount:4.0,scale:None,flame:false,
            launch_origin:from,already_suppressed:false,cover_mul:1.0,cover_behind:0.0 },
            &mut m.firing,&mut BTreeSet::new()).unwrap();
        assert!((before-m.world.game.units[&target].hp-expected).abs()<1e-9);
        assert!((m.world.game.units[&target].supp-4.0*(1.0-0.15*f64::from(stars))).abs()<1e-9);
        assert!((m.combat.get(shooter).unwrap().xp-xp-expected).abs()<1e-9,"actual damage XP");
    }
}
