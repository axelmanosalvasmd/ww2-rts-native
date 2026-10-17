//! Companion effects over canonical firing, damage and recorded match randomness.
use crate::{
    combat::CombatEvent,
    companion_ray,
    companions::{Input, OPERATIVE, SAFE_INTEGER, WEAPONS},
    environment,
    game::{CELL, Point},
    match_actor::ordinary_ground_mode,
    match_combat::{DamageWorld, ExplosionSource},
    match_firing::{AbilityBlast, LaunchEvent, SmallArmsEvent, SmallArmsHit},
    match_runtime::Match,
    math,
    random::StreamId,
};
use std::collections::BTreeSet;

impl Match {
    /// Resolve an admitted grenade press. The companion phase owns press counts,
    /// input expiry and sprint priority; the canonical firing owner publishes flight state.
    pub fn throw_companion(
        &mut self,
        unit: u32,
        input: &Input,
        winner_decided: bool,
    ) -> Result<u64, String> {
        let companion = self.check_companion(unit)?;
        let op = &self.companions.records[&companion];
        let body = &self.companions.soldiers[&unit];
        let u = &self.world.game.units[&unit];
        if !ordinary_ground_mode(&self.world.game, self.options.mode)
            || winner_decided
            || self.finish.is_some()
            || !op.online
            || self.start.economy.players[op.owner].out
            || u.hp <= 0.0
            || body.nades == 0
            || body.nade_time > 0.0
        {
            return Err("companion grenade is not ready".into());
        }
        if ![input.forward, input.strafe, input.yaw, input.pitch]
            .iter()
            .all(|v| v.is_finite())
            || input.forward.abs() > 1.0
            || input.strafe.abs() > 1.0
            || input.yaw.abs() > 1e4
            || input.pitch.abs() > 1.45
            || input.weapon > 1
        {
            return Err("invalid companion grenade input".into());
        }
        let distance = math::max(3.0, math::min(24.0, 13.0 + input.pitch * 16.0));
        let at = Point {
            x: math::max(
                0.5,
                math::min(
                    self.world.game.w as f64 * CELL - 0.5,
                    u.x + math::cos(input.yaw) * distance,
                ),
            ),
            z: math::max(
                0.5,
                math::min(
                    self.world.game.h as f64 * CELL - 0.5,
                    u.z + math::sin(input.yaw) * distance,
                ),
            ),
        };
        // Source duration uses the requested distance before map-edge clipping.
        let duration = 0.45 + distance / 20.0;
        let rifle = self
            .rules
            .kind("rifle")
            .ok_or("missing rifle grenade profile")?;
        let blast = AbilityBlast::from_unit(rifle, &self.rules)?;
        let flight = self.launch_ability_flight(unit, at, blast, Some(duration), None)?;
        let body = self
            .companions
            .soldiers
            .get_mut(&unit)
            .expect("checked soldier");
        body.nades -= 1;
        body.nade_time = OPERATIVE.nade_cd;
        Ok(flight)
    }

    /// Resolve an admitted fire trigger. The surrounding companion phase owns
    /// input expiry, movement and reload/sprint/weapon priority.
    pub fn fire_companion(
        &mut self,
        unit: u32,
        input: &Input,
        winner_decided: bool,
    ) -> Result<(), String> {
        let companion = self.check_companion(unit)?;
        let body = &self.companions.soldiers[&unit];
        let op = &self.companions.records[&companion];
        let u = &self.world.game.units[&unit];
        if !ordinary_ground_mode(&self.world.game, self.options.mode)
            || winner_decided
            || self.finish.is_some()
            || !op.online
            || self.start.economy.players[op.owner].out
            || u.hp <= 0.0
            || body.reload > 0.0
            || self
                .firing
                .get(unit)
                .expect("checked firing state")
                .cooldown
                > 0.0
            || body.mags[body.weapon] == 0
        {
            return Err("companion fire is not ready".into());
        }
        if [op.shots, op.hits, op.kills]
            .iter()
            .any(|n| *n >= SAFE_INTEGER as u64)
        {
            return Err("companion combat counter exhausted".into());
        }
        let owner = u.owner;
        let from = u.at();
        let moving = u.move_speed;
        let tick = self.world.game.tick;
        let weapon = WEAPONS[body.weapon];
        let weapon_index = body.weapon;
        let unit_order: Vec<_> = self.world.game.units.keys().copied().collect();
        let mut random = self.start.random.clone();
        // Source radius draw precedes source azimuth draw, even when spread is zero.
        let draws = [
            random.next_fraction(StreamId::Combat),
            random.next_fraction(StreamId::Combat),
        ];
        let ray = companion_ray::trace(
            &self.world.game,
            &self.rules,
            &self.companions.soldiers,
            &unit_order,
            unit,
            input,
            moving,
            draws,
        )?;
        let (wind_angle, wind_strength) = self.start.wind;
        let mut damage = DamageWorld {
            world: &mut self.world,
            combat: &mut self.combat,
            grid: &mut self.grid,
            rules: &self.rules,
            logistics: &self.logistics,
            production: &self.production,
            mode: self.options.mode,
            tech_enabled: self.options.tech,
            environment: &mut self.environment,
            environment_seed: &mut self.start.environment_seed,
            next_id: &mut self.next_unit_id,
            unit_order: &unit_order,
            wind_angle,
            wind_strength,
        };
        damage
            .validate()
            .map_err(|e| format!("companion damage context: {e:?}"))?;
        let mut killed = false;
        let mut hurt = None;
        if let Some(hit) = ray.enemy_hit {
            let profile = ExplosionSource {
                infantry: weapon.damage,
                vehicle: weapon.veh,
                terrain: Some(weapon.veh),
                suppression: weapon.supp,
                scar: 0.0,
                anti_garrison: None,
                weapon_kind: None,
                off_map_support: false,
            };
            let mut prepared = damage
                .prepare_hurt(hit.unit, profile, 1.0, owner as i32)
                .map_err(|e| format!("companion hurt preparation: {e:?}"))?;
            // Source FPS fire assigns hit metadata directly without retarget reset or XP.
            prepared.events.push(CombatEvent::HitSource {
                unit: hit.unit,
                shooter: unit,
                tick: tick as f64,
                from,
            });
            killed = prepared.hurt.killed;
            damage
                .apply_events(&prepared.events, &mut self.firing, &mut BTreeSet::new())
                .map_err(|e| format!("companion hurt application: {e:?}"))?;
            hurt = Some(prepared.hurt);
        }
        // All fallible input, damage and rider placement has finished. Publication
        // is infallible and preserves the source Hurt-before-small-arms event order.
        self.start.random = random;
        self.firing
            .get_mut(unit)
            .expect("checked firing state")
            .cooldown = weapon.interval;
        self.companions
            .soldiers
            .get_mut(&unit)
            .expect("checked soldier")
            .mags[weapon_index] -= 1;
        self.world
            .game
            .units
            .get_mut(&unit)
            .expect("checked unit")
            .shot_at = Some(tick as f64);
        let op = self
            .companions
            .records
            .get_mut(&companion)
            .expect("checked companion");
        op.shots += 1;
        if ray.enemy_hit.is_some() {
            op.hits += 1;
            if killed {
                op.kills += 1;
            }
        }
        if let Some(hurt) = hurt {
            self.firing
                .events
                .push(LaunchEvent::Environment(environment::Effect::Hurt(hurt)));
        }
        self.firing
            .events
            .push(LaunchEvent::SmallArms(SmallArmsEvent {
                effect: weapon.fx,
                shooter: unit,
                owner,
                at: ray.end,
                hit: ray.enemy_hit.map(|hit| SmallArmsHit {
                    target: hit.unit,
                    owner: hit.owner,
                    man: hit.man,
                }),
            }));
        Ok(())
    }
}
