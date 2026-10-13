//! First-person entry, movement, squad attachment and respawn over canonical Match units.
use crate::{
    companions::{
        CombatRequest, CompanionId, OPERATIVE, RESPAWN_TICKS, SAFE_INTEGER, StepResult, WEAPONS,
    },
    game::{CELL, MOVE, Point},
    match_runtime::Match,
    math, terrain,
};

impl Match {
    pub fn companion_input(
        &mut self,
        key: &str,
        input: &serde_json::Value,
        winner_decided: bool,
    ) -> bool {
        let tick = self.world.game.tick;
        if !(0..=SAFE_INTEGER - RESPAWN_TICKS).contains(&tick) {
            return false;
        }
        let Some(op) = self.companions.get_mut(key) else {
            return false;
        };
        let eligible = !winner_decided
            && self
                .start
                .economy
                .players
                .get(op.owner)
                .is_some_and(|p| !p.out);
        op.accept(input, tick, eligible)
    }

    /// Validate identity and numeric state before any movement, timer or press mutation.
    pub(crate) fn check_companion(&self, id: u32) -> Result<CompanionId, String> {
        let g = &self.world.game;
        let cells =
            g.w.checked_mul(g.h)
                .filter(|n| (1..=4_000_000).contains(n))
                .ok_or("invalid companion terrain dimensions")?;
        if g.flags.len() != cells
            || g.height.as_ref().is_some_and(|h| h.len() != cells)
            || self.rules.movement.tick != 0.05
        {
            return Err("invalid companion terrain or tick duration".into());
        }
        if !(0..=SAFE_INTEGER - RESPAWN_TICKS).contains(&g.tick) {
            return Err("companion tick is outside safe range".into());
        }
        let u = g.units.get(&id).ok_or("missing companion soldier")?;
        let body = self
            .companions
            .soldiers
            .get(&id)
            .ok_or("missing companion body")?;
        let op = self
            .companions
            .records
            .get(&body.companion)
            .ok_or("missing companion record")?;
        let control = self
            .commands
            .units
            .get(&id)
            .ok_or("missing companion control")?;
        let firing = self
            .firing
            .get(id)
            .ok_or("missing companion firing state")?;
        if !control.operative
            || !control.aim.is_finite()
            || op.unit_id != id
            || op.owner != u.owner
            || self.start.economy.players.get(u.owner).is_none()
            || u.kind >= self.rules.kinds.len()
            || body.weapon > 1
            || body.mags.iter().zip(WEAPONS).any(|(n, w)| *n > w.mag)
            || body.nades > OPERATIVE.nades
            || op.input.as_ref().is_some_and(|i| i.weapon > 1)
            || op.last_input < -100
            || op.last_input > SAFE_INTEGER
            || op.respawn_at < 0
            || op.respawn_at > SAFE_INTEGER
            || ![
                u.x,
                u.z,
                u.rot,
                u.hp,
                u.supp,
                u.vx.unwrap_or(f64::NAN),
                u.vz.unwrap_or(f64::NAN),
                body.vy,
                body.jy,
                body.reload,
                body.nade_time,
                firing.cooldown,
            ]
            .iter()
            .all(|n| n.is_finite())
            || u.x < 0.0
            || u.z < 0.0
            || u.x >= g.w as f64 * CELL
            || u.z >= g.h as f64 * CELL
            || body.jy < 0.0
            || body.reload < 0.0
            || body.nade_time < 0.0
            || firing.cooldown < 0.0
            || u.vx.is_some_and(|v| v.abs() > 1e4)
            || u.vz.is_some_and(|v| v.abs() > 1e4)
            || body.vy.abs() > 1e4
            || body.jy > 1e4
        {
            return Err("invalid companion runtime state".into());
        }
        if let Some(input) = &op.input
            && (![input.forward, input.strafe, input.yaw, input.pitch]
                .iter()
                .all(|n| n.is_finite())
                || input.forward.abs() > 1.0
                || input.strafe.abs() > 1.0
                || input.yaw.abs() > 1e4
                || input.pitch.abs() > 1.45)
        {
            return Err("invalid stored companion input".into());
        }
        self.check_squads()?;
        Ok(body.companion)
    }
    fn check_squads(&self) -> Result<(), String> {
        if self.world.game.units.iter().any(|(id, u)| {
            *id != u.id
                || u.kind >= self.rules.kinds.len()
                || u.owner >= self.start.economy.players.len()
                || ![u.x, u.z, u.hp].iter().all(|n| n.is_finite())
                || u.amove
                    .is_some_and(|p| !p.x.is_finite() || !p.z.is_finite())
                || !self.commands.units.contains_key(id)
        }) {
            return Err("invalid companion squad context".into());
        }
        Ok(())
    }

    /// The caller dispatches operative units here instead of ordinary movement.
    /// Fire/grenade requests require the combat owner; this phase does not spend their ammo.
    pub fn step_companion(
        &mut self,
        unit: u32,
        winner_decided: bool,
    ) -> Result<StepResult, String> {
        let companion = self.check_companion(unit)?;
        if self.world.game.units[&unit].hp <= 0.0 {
            return Ok(StepResult::default());
        }
        let tick = self.world.game.tick;
        let op = &self.companions.records[&companion];
        let input = if op.online && tick - op.last_input <= 5 {
            op.input.clone()
        } else {
            None
        };
        let live = input.is_some() && !winner_decided && !self.start.economy.players[op.owner].out;
        let want = if live { op.want } else { [false; 4] };
        if live
            && tick % 5 == 0
            && !self.start.economy.players[op.owner].away
            && (want[3] || self.led_companion_squad(companion).is_some())
        {
            self.recipient_routing(op.owner)?;
            if self.world.game.units.values().any(|s| {
                s.owner == op.owner
                    && s.hp > 0.0
                    && self.rules.kinds[s.kind].infantry
                    && !s.riding
                    && !self.commands.units[&s.id].operative
                    && (self.commands.units[&s.id].follow == 0
                        || self.commands.units[&s.id].follow == companion)
                    && s.path_gen == u64::MAX
            }) {
                return Err("companion squad path identity exhausted".into());
            }
        }
        self.companions
            .records
            .get_mut(&companion)
            .expect("checked record")
            .want = [false; 4];
        let body = self
            .companions
            .soldiers
            .get_mut(&unit)
            .expect("checked body");
        let u = self
            .world
            .game
            .units
            .get_mut(&unit)
            .expect("checked soldier");
        let dt = self.rules.movement.tick;
        let firing = self.firing.get_mut(unit).expect("checked firing state");
        firing.cooldown = math::max(0.0, firing.cooldown - dt);
        body.nade_time = math::max(0.0, body.nade_time - dt);
        u.supp = math::max(0.0, u.supp - 8.0 * dt);
        let grounded = body.jy <= 0.0 && body.vy <= 0.0;
        if live {
            let input = input.as_ref().expect("live input");
            u.rot = input.yaw;
            self.commands
                .units
                .get_mut(&unit)
                .expect("checked control")
                .aim = input.yaw;
            if want[0] && grounded {
                body.vy = OPERATIVE.jump;
                body.crouch = false;
            } else {
                body.crouch = input.crouch && (grounded || body.crouch);
            }
        }
        if body.jy > 0.0 || body.vy > 0.0 {
            body.vy -= OPERATIVE.gravity * dt;
            body.jy = math::max(0.0, body.jy + body.vy * dt);
            if body.jy == 0.0 {
                body.vy = 0.0;
            }
        }
        let active = input.as_ref().filter(|_| live);
        let (f, s) = active.map_or((0.0, 0.0), |i| (i.forward, i.strafe));
        let sprint = active.is_some_and(|i| i.sprint && f > 0.0 && !i.ads && !body.crouch);
        let walk = OPERATIVE.walk;
        let cap = if sprint {
            OPERATIVE.sprint
        } else if body.crouch {
            OPERATIVE.crouch
        } else if active.is_some_and(|i| i.ads) {
            OPERATIVE.ads
        } else {
            walk
        };
        let len = math::max(1.0, math::hypot(f, s));
        let yaw = active.map_or(u.rot, |i| i.yaw);
        let tx = (math::cos(yaw) * f - math::sin(yaw) * s) / len * cap;
        let tz = (math::sin(yaw) * f + math::cos(yaw) * s) / len * cap;
        let step = if grounded {
            OPERATIVE.accel * dt
        } else {
            OPERATIVE.air_accel * dt
        };
        let (mut vx, mut vz) = (u.vx.expect("checked vx"), u.vz.expect("checked vz"));
        let (dx, dz) = (tx - vx, tz - vz);
        let distance = math::hypot(dx, dz);
        if distance <= step {
            vx = tx;
            vz = tz;
        } else {
            vx += dx / distance * step;
            vz += dz / distance * step;
        }
        let (x0, z0) = (u.x, u.z);
        // Release the unit borrow for terrain queries. The x result feeds the z query.
        let at = Point {
            x: x0 + vx * dt,
            z: z0,
        };
        let x = if vx != 0.0 && can_stand(&self.world.game, &self.rules, Point { x: x0, z: z0 }, at)
        {
            at.x
        } else {
            vx = 0.0;
            x0
        };
        let at = Point { x, z: z0 + vz * dt };
        let z = if vz != 0.0 && can_stand(&self.world.game, &self.rules, Point { x, z: z0 }, at) {
            at.z
        } else {
            vz = 0.0;
            z0
        };
        let u = self
            .world
            .game
            .units
            .get_mut(&unit)
            .expect("checked soldier");
        u.x = x;
        u.z = z;
        u.vx = Some(vx);
        u.vz = Some(vz);
        u.move_speed = math::hypot(x - x0, z - z0) / dt;
        if u.move_speed != 0.0 {
            self.grid.update(unit, u);
        }
        if !live {
            return Ok(StepResult::default());
        }
        let input = input.expect("live input");
        let body = self
            .companions
            .soldiers
            .get_mut(&unit)
            .expect("checked body");
        let weapon = WEAPONS[body.weapon];
        let (mag, reload) = (weapon.mag, weapon.reload);
        if input.weapon != body.weapon {
            body.weapon = input.weapon;
            body.reload = 0.0;
            let firing = self.firing.get_mut(unit).expect("checked firing state");
            firing.cooldown = math::max(firing.cooldown, OPERATIVE.swap);
        } else if body.reload > 0.0 {
            body.reload -= dt;
            if body.reload <= 0.0 {
                body.reload = 0.0;
                body.mags[body.weapon] = mag;
            }
        } else if (want[1] || (input.fire && body.mags[body.weapon] == 0))
            && body.mags[body.weapon] < mag
        {
            body.reload = reload;
        }
        if want[3] {
            self.pick_companion_squad(unit)?;
        }
        self.lead_companion_squad(unit, winner_decided)?;
        let body = &self.companions.soldiers[&unit];
        let combat = if want[2] && body.nades > 0 && body.nade_time == 0.0 && !sprint {
            Some(CombatRequest::Grenade { unit, input })
        } else if input.fire
            && !sprint
            && body.reload == 0.0
            && self
                .firing
                .get(unit)
                .expect("checked firing state")
                .cooldown
                <= 0.0
            && body.mags[body.weapon] > 0
        {
            Some(CombatRequest::Fire { unit, input })
        } else {
            None
        };
        Ok(StepResult { combat })
    }

    pub fn pick_companion_squad(&mut self, unit: u32) -> Result<(), String> {
        let companion = self.check_companion(unit)?;
        let op = &self.companions.records[&companion];
        if self.led_companion_squad(companion).is_some() {
            self.commands
                .units
                .get_mut(&op.squad_id)
                .expect("checked squad control")
                .follow = 0;
            self.companions
                .records
                .get_mut(&companion)
                .expect("checked record")
                .squad_id = 0;
            return Ok(());
        }
        let u = &self.world.game.units[&unit];
        let mut best = None;
        let mut distance = 8.0;
        for s in self.world.game.units.values() {
            let d = math::hypot(s.x - u.x, s.z - u.z);
            let control = &self.commands.units[&s.id];
            if d < distance
                && s.owner == u.owner
                && s.hp > 0.0
                && self.rules.kinds[s.kind].infantry
                && !control.operative
                && !s.riding
                && control.follow == 0
            {
                best = Some(s.id);
                distance = d;
            }
        }
        if let Some(id) = best {
            self.commands
                .units
                .get_mut(&id)
                .expect("checked squad control")
                .follow = companion;
            self.companions
                .records
                .get_mut(&companion)
                .expect("checked record")
                .squad_id = id;
        }
        Ok(())
    }
    /// Reuse the ordinary command actuator and restore follow after its ownership filter.
    pub fn lead_companion_squad(&mut self, unit: u32, winner_decided: bool) -> Result<(), String> {
        use crate::match_commands::{Command, MoveOrder};
        let companion = self.check_companion(unit)?;
        let Some((squad, at)) = self.companion_lead_request(unit)? else {
            return Ok(());
        };
        let owner = self.world.game.units[&squad].owner;
        let command = Command::Move {
            orders: vec![MoveOrder {
                unit: i64::from(squad),
                at,
            }],
            control_ids: None,
            attack_move: true,
            queue: false,
            together: false,
            face: None,
        };
        self.execute_command(owner, &command, winner_decided)?;
        self.commands
            .units
            .get_mut(&squad)
            .expect("retained led squad")
            .follow = companion;
        Ok(())
    }
    pub(crate) fn led_companion_squad(&self, companion: CompanionId) -> Option<u32> {
        let op = self.companions.records.get(&companion)?;
        let s = self.world.game.units.get(&op.squad_id)?;
        (s.hp > 0.0 && self.commands.units.get(&s.id)?.follow == companion).then_some(s.id)
    }
    /// A real ordinary attack-move command must consume this request immediately.
    /// Its normal command release is followed by restoring this companion's follow identity.
    fn companion_lead_request(&mut self, unit: u32) -> Result<Option<(u32, Point)>, String> {
        let companion = self.check_companion(unit)?;
        let Some(squad) = self.led_companion_squad(companion) else {
            self.companions
                .records
                .get_mut(&companion)
                .expect("checked record")
                .squad_id = 0;
            return Ok(None);
        };
        let g = &self.world.game;
        let s = &g.units[&squad];
        let u = &g.units[&unit];
        if g.tick % 5 != 0 || s.retreating {
            return Ok(None);
        }
        let at = Point {
            x: math::max(
                1.0,
                math::min(g.w as f64 * CELL - 1.0, u.x - math::cos(u.rot) * 4.0),
            ),
            z: math::max(
                1.0,
                math::min(g.h as f64 * CELL - 1.0, u.z - math::sin(u.rot) * 4.0),
            ),
        };
        if s.amove.map_or_else(
            || math::hypot(s.x - at.x, s.z - at.z) < 5.0,
            |a| math::hypot(a.x - at.x, a.z - at.z) < 3.0,
        ) {
            return Ok(None);
        }
        Ok(Some((squad, at)))
    }
    /// Preflight these hooks before canonical retirement removes any dead units.
    /// The orchestration caller then invokes companion_died on returned retired records.
    pub fn prepare_companion_deaths(&self, units: &[u32]) -> Result<(), String> {
        let tick = self.world.game.tick;
        if !(0..=SAFE_INTEGER - RESPAWN_TICKS).contains(&tick) {
            return Err("companion death tick is outside safe range".into());
        }
        for id in units {
            let Some(body) = self.companions.soldiers.get(id) else {
                continue;
            };
            let op = self
                .companions
                .records
                .get(&body.companion)
                .ok_or("missing dead companion record")?;
            if op.unit_id != *id
                || self
                    .world
                    .game
                    .units
                    .get(id)
                    .is_some_and(|u| !u.hp.is_finite())
            {
                return Err("dead companion identity mismatch".into());
            }
        }
        Ok(())
    }
    /// Call alongside canonical retirement, once per retired operative record.
    /// Normal death removal, rewards and cleanup remain with the combat retirement phase.
    pub fn companion_died(&mut self, unit: u32) -> Result<bool, String> {
        let Some(body) = self.companions.soldiers.get(&unit) else {
            return Ok(false);
        };
        let tick = self.world.game.tick;
        if self
            .world
            .game
            .units
            .get(&unit)
            .is_some_and(|u| !u.hp.is_finite() || u.hp > 0.0)
        {
            return Err("companion retirement requires a dead soldier".into());
        }
        if !(0..=SAFE_INTEGER - RESPAWN_TICKS).contains(&tick) {
            return Err("companion death tick is outside safe range".into());
        }
        let op = self
            .companions
            .records
            .get_mut(&body.companion)
            .ok_or("missing dead companion record")?;
        if op.unit_id != unit {
            return Err("dead companion identity mismatch".into());
        }
        op.unit_id = 0;
        op.input = None;
        op.respawn_at = tick + RESPAWN_TICKS;
        self.companions.soldiers.remove(&unit);
        Ok(true)
    }
}
fn can_stand(g: &crate::game::Game, rules: &crate::rules::Rules, from: Point, to: Point) -> bool {
    to.x >= 0.4
        && to.z >= 0.4
        && to.x < g.w as f64 * CELL - 0.4
        && to.z < g.h as f64 * CELL - 0.4
        && terrain::walkable(g, rules, from, to, MOVE, 0.35)
}

impl Match {
    pub(crate) fn check_companion_lifecycle(&self) -> Result<(), String> {
        let g = &self.world.game;
        if !(0..=SAFE_INTEGER - RESPAWN_TICKS).contains(&g.tick)
            || self.start.economy.players.len() != self.start.players.len()
            || self.start.players.len() != g.players.len()
            || self.start.economy.players.iter().any(|p| !p.mp.is_finite())
            || self
                .start
                .players
                .iter()
                .any(|p| !p.spawn.x.is_finite() || !p.spawn.z.is_finite())
        {
            return Err("invalid companion lifecycle context".into());
        }
        self.check_squads()?;
        for (id, op) in &self.companions.records {
            if *id == 0
                || *id != op.id
                || op.owner >= g.players.len()
                || !(0..=SAFE_INTEGER).contains(&op.respawn_at)
                || op.input.as_ref().is_some_and(|i| i.weapon >= WEAPONS.len())
                || self.companions.get(&op.key).is_none_or(|row| row.id != *id)
            {
                return Err("invalid companion lifecycle record".into());
            }
        }
        for (unit, body) in &self.companions.soldiers {
            let op = self
                .companions
                .records
                .get(&body.companion)
                .ok_or("orphaned companion soldier")?;
            if op.unit_id != *unit || !g.units.contains_key(unit) {
                return Err("orphaned companion soldier".into());
            }
        }
        Ok(())
    }
    fn stage_companion_soldier(
        &self,
        op: &crate::companions::Companion,
        id: u32,
        index: u32,
        at: Option<Point>,
    ) -> Result<crate::unit_publication::StagedUnit, String> {
        if at.is_some_and(|p| {
            p.x < 0.0
                || p.z < 0.0
                || p.x >= self.world.game.w as f64 * CELL
                || p.z >= self.world.game.h as f64 * CELL
        }) {
            return Err("respawn squad is outside the map".into());
        }
        let kind = self
            .rules
            .kinds
            .iter()
            .position(|k| k.name == "rifle")
            .ok_or("missing operative rifle definition")?;
        let mut draft = crate::unit_publication::StagedUnit::ordinary(
            &self.world.game,
            &self.rules,
            op.owner,
            kind,
            id,
            index,
            self.start.players[op.owner].spawn,
            at,
        )
        .map_err(|e| format!("companion spawn: {e:?}"))?;
        draft.unit.hp = OPERATIVE.hp;
        draft.unit.hold_pos = true;
        draft.unit.vx = Some(0.0);
        draft.unit.vz = Some(0.0);
        draft.control.operative = true;
        draft.control.auto_cast = false;
        draft.control.auto_retreat = false;
        draft.control.hold_fire = true;
        if self.firing.get(id).is_some()
            || self.combat.get(id).is_some()
            || self.abilities.get(id).is_some()
            || self.recovery.get(id).is_some()
        {
            return Err("companion unit metadata identity already exists".into());
        }
        Ok(draft)
    }
    /// First admission gets a free soldier. Existing keys keep their original commander.
    pub fn join_companion(
        &mut self,
        key: &str,
        owner: usize,
        winner_decided: bool,
    ) -> Result<Option<u32>, String> {
        use crate::unit_publication::{PublicationParts, prepare_publication};
        let Some(player) = self.start.economy.players.get(owner) else {
            return Ok(None);
        };
        if player.out || winner_decided {
            return Ok(None);
        }
        if let Some(op) = self.companions.get(key) {
            return Ok(Some(op.unit_id));
        }
        self.check_companion_lifecycle()?;
        let id = self
            .companions
            .next_id()
            .ok_or("companion identity capacity exhausted")?;
        let mut op = crate::companions::Companion::new(id, key.to_owned(), owner);
        let index = u32::try_from(self.world.game.units.len())
            .map_err(|_| "companion scatter capacity exhausted")?;
        let unit = self.next_unit_id;
        let draft = self.stage_companion_soldier(&op, unit, index, None)?;
        let publication = prepare_publication(
            PublicationParts {
                units: &mut self.world.game.units,
                grid: &mut self.grid,
                controls: &mut self.commands.units,
                recruitment: &mut self.recruit_metadata,
                next_id: &mut self.next_unit_id,
            },
            &self.rules,
            self.start.players.len(),
            vec![draft],
        )
        .map_err(|e| format!("companion publication: {e:?}"))?;
        publication.publish_with(|u, _| {
            self.firing.initialize(u.id);
            self.combat.initialize(u.id);
            self.abilities.initialize(u.id);
            self.recovery.initialize(u.id);
        });
        op.unit_id = unit;
        self.companions
            .soldiers
            .insert(unit, crate::companions::Soldier::new(id, 0));
        self.companions.insert(op);
        Ok(Some(unit))
    }
    /// Run after the caller increments the tick, before movement or other match phases.
    /// All eligible soldiers and existing squad HP are prepared before any wallet changes.
    pub fn respawn_companions(&mut self, winner_decided: bool) -> Result<Vec<u32>, String> {
        use crate::{
            catalog::Catalog,
            companions::{RESPAWN_COST, Soldier},
            unit_publication::{PublicationParts, prepare_publication},
        };
        self.check_companion_lifecycle()?;
        if winner_decided {
            return Ok(Vec::new());
        }
        let tick = self.world.game.tick;
        let mut wallets: Vec<_> = self.start.economy.players.iter().map(|p| p.mp).collect();
        let mut hp = std::collections::BTreeMap::new();
        let mut drafts = Vec::new();
        let mut bodies = Vec::new();
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        for op in self.companions.records.values() {
            if op.unit_id != 0
                || !op.online
                || tick < op.respawn_at
                || self.start.economy.players[op.owner].out
            {
                continue;
            }
            let mut from = None;
            if let Some(id) = self.led_companion_squad(op.id) {
                let s = &self.world.game.units[&id];
                let per = catalog
                    .unit(&self.rules.kinds[s.kind].name)
                    .map_err(|e| e.to_string())?
                    .get("hpPer")
                    .and_then(|v| v.number())
                    .filter(|n| n.is_finite() && *n > 0.0)
                    .ok_or("invalid respawn squad health definition")?;
                let health = hp.get(&id).copied().unwrap_or(s.hp);
                if s.garrison.is_none() && !s.riding && health > per {
                    from = Some(s.at());
                    hp.insert(id, health - per);
                }
            }
            if from.is_none() {
                if wallets[op.owner] < RESPAWN_COST {
                    continue;
                }
                wallets[op.owner] -= RESPAWN_COST;
            }
            let offset =
                u32::try_from(drafts.len()).map_err(|_| "companion spawn count exhausted")?;
            let unit = self
                .next_unit_id
                .checked_add(offset)
                .ok_or("companion unit identity exhausted")?;
            let index = u32::try_from(self.world.game.units.len())
                .map_err(|_| "companion scatter capacity exhausted")?
                .checked_add(offset)
                .ok_or("companion scatter capacity exhausted")?;
            drafts.push(self.stage_companion_soldier(op, unit, index, from)?);
            bodies.push((op.id, unit, op.input.as_ref().map_or(0, |i| i.weapon)));
        }
        if drafts.is_empty() {
            return Ok(Vec::new());
        }
        let mut publication = prepare_publication(
            PublicationParts {
                units: &mut self.world.game.units,
                grid: &mut self.grid,
                controls: &mut self.commands.units,
                recruitment: &mut self.recruit_metadata,
                next_id: &mut self.next_unit_id,
            },
            &self.rules,
            self.start.players.len(),
            drafts,
        )
        .map_err(|e| format!("companion publication: {e:?}"))?;
        for (id, health) in hp {
            publication
                .stage_existing_hp(id, health)
                .map_err(|e| format!("companion squad payment: {e:?}"))?;
        }
        for (player, mp) in self.start.economy.players.iter_mut().zip(wallets) {
            player.mp = mp;
        }
        let ids = publication.publish_with(|u, _| {
            self.firing.initialize(u.id);
            self.combat.initialize(u.id);
            self.abilities.initialize(u.id);
            self.recovery.initialize(u.id);
        });
        for (companion, unit, weapon) in bodies {
            self.companions
                .soldiers
                .insert(unit, Soldier::new(companion, weapon));
            let op = self
                .companions
                .records
                .get_mut(&companion)
                .expect("prepared companion");
            op.unit_id = unit;
            op.input = None;
        }
        Ok(ids)
    }
}
