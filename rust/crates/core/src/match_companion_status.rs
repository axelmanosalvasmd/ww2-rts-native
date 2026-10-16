use crate::{catalog::Catalog, companions::OPERATIVE, match_runtime::Match, math};
use serde_json::{Value, json};

impl Match {
    /// Prototype server.js:294-306, including its ten-tick attacker position.
    pub fn companion_status(&self, key: &str) -> Value {
        let g = &self.world.game;
        let op = self.companions.get(key);
        let unit = op.and_then(|op| g.units.get(&op.unit_id));
        let squad = op.and_then(|op| {
            let squad = g.units.get(&op.squad_id)?;
            (squad.hp > 0.0 && self.commands.units.get(&squad.id)?.follow == op.id).then_some(squad)
        });
        let definition = squad.map(|squad| {
            Catalog::compiled()
                .expect("compiled unit catalog")
                .unit(&self.rules.kinds[squad.kind].name)
                .expect("canonical squad kind")
        });
        let hp_per = definition
            .and_then(|d| d.get("hpPer"))
            .and_then(|v| v.number());
        let squad_spawn = squad
            .zip(hp_per)
            .is_some_and(|(squad, hp)| squad.garrison.is_none() && !squad.riding && squad.hp > hp);
        let mut status = json!({
            "id": unit.map_or(0, |u| u.id),
            "hp": math::max(0.0, unit.map_or(0.0, |u| u.hp).ceil()),
            "maxHp": OPERATIVE.hp,
            "respawn": op.map_or(0.0, |op| math::max(0.0, ((op.respawn_at as f64 - g.tick as f64) * 0.05).ceil())),
            "cost": OPERATIVE.cost,
            "squad": squad.zip(definition).map(|(squad, definition)| json!({
                "id": squad.id,
                "name": definition.get("name").and_then(|v| v.string()),
                "men": (squad.hp / hp_per.unwrap_or(f64::NAN)).ceil(),
            })),
            "squadSpawn": squad_spawn,
            "waiting": unit.is_none() && !squad_spawn && op.and_then(|op| self.start.economy.players.get(op.owner)).map_or(0.0, |p| p.mp) < OPERATIVE.cost,
            "seq": op.map_or(-1, |op| op.last_seq),
            "shots": op.map_or(0, |op| op.shots),
            "hits": op.map_or(0, |op| op.hits),
            "kills": op.map_or(0, |op| op.kills),
        });
        if let Some(unit) = unit {
            let body = &self.companions.soldiers[&unit.id];
            let rounded = |n| math::round(n * 100.0) / 100.0;
            let from = self.combat.get(unit.id).and_then(|history| {
                (g.tick as f64 - history.hit_at <= 10.0)
                    .then_some(unit.hit_from)
                    .flatten()
                    .map(|at| json!([math::round(at.x), math::round(at.z)]))
            });
            status.as_object_mut().unwrap().extend(
                json!({
                    "x": rounded(unit.x), "z": rounded(unit.z),
                    "vx": rounded(unit.vx.unwrap_or(0.0)), "vz": rounded(unit.vz.unwrap_or(0.0)),
                    "jy": rounded(body.jy), "crouch": body.crouch, "weapon": body.weapon,
                    "mags": body.mags, "reload": math::round(body.reload * 10.0) / 10.0,
                    "nades": body.nades, "from": from,
                })
                .as_object()
                .unwrap()
                .clone(),
            );
        }
        status
    }
}
