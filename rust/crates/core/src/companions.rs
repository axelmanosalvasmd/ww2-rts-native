//! First-person infantry input and private runtime state. No transport or C representation.
use serde_json::Value;
use std::collections::BTreeMap;

pub const SAFE_INTEGER: i64 = 9_007_199_254_740_991;
pub const RESPAWN_TICKS: i64 = 160;
pub const RESPAWN_COST: f64 = OPERATIVE.cost;
pub const EDGE_NAMES: [&str; 4] = ["jump", "reload", "nade", "squad"];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OperativeRules {
    pub hp: f64,
    pub eye: f64,
    pub crouch_eye: f64,
    pub respawn: f64,
    pub cost: f64,
    pub nades: u32,
    pub nade_cd: f64,
    pub swap: f64,
    pub walk: f64,
    pub sprint: f64,
    pub crouch: f64,
    pub ads: f64,
    pub accel: f64,
    pub air_accel: f64,
    pub jump: f64,
    pub gravity: f64,
}
pub const OPERATIVE: OperativeRules = OperativeRules {
    hp: 20.0,
    eye: 1.65,
    crouch_eye: 1.1,
    respawn: 8.0,
    cost: 10.0,
    nades: 3,
    nade_cd: 1.0,
    swap: 0.45,
    walk: 4.5,
    sprint: 7.0,
    crouch: 2.3,
    ads: 2.8,
    accel: 30.0,
    air_accel: 6.0,
    jump: 4.6,
    gravity: 15.0,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weapon {
    pub id: &'static str,
    pub name: &'static str,
    pub mag: u32,
    pub interval: f64,
    pub reload: f64,
    pub range: f64,
    pub damage: f64,
    pub veh: f64,
    pub supp: f64,
    pub hip: f64,
    pub ads: f64,
    pub fx: &'static str,
}
pub const WEAPONS: [Weapon; 2] = [
    Weapon {
        id: "rifle",
        name: "Rifle",
        mag: 8,
        interval: 0.32,
        reload: 2.4,
        range: 60.0,
        damage: 8.0,
        veh: 0.5,
        supp: 6.0,
        hip: 0.02,
        ads: 0.003,
        fx: "sniper",
    },
    Weapon {
        id: "smg",
        name: "SMG",
        mag: 32,
        interval: 0.1,
        reload: 2.1,
        range: 32.0,
        damage: 3.5,
        veh: 0.2,
        supp: 3.0,
        hip: 0.05,
        ads: 0.022,
        fx: "commando",
    },
];

/// Opaque source keys are interned in admission order. Zero means no follower.
pub type CompanionId = u32;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Input {
    pub forward: f64,
    pub strafe: f64,
    pub yaw: f64,
    pub pitch: f64,
    pub fire: bool,
    pub ads: bool,
    pub crouch: bool,
    pub sprint: bool,
    pub weapon: usize,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Companion {
    pub id: CompanionId,
    pub key: String,
    pub owner: usize,
    pub unit_id: u32,
    pub squad_id: u32,
    pub respawn_at: i64,
    pub online: bool,
    pub input: Option<Input>,
    pub last_input: i64,
    pub last_seq: i64,
    pub shots: u64,
    pub hits: u64,
    pub kills: u64,
    pub seen: [Option<i64>; 4],
    pub want: [bool; 4],
}
impl Companion {
    pub(crate) fn new(id: CompanionId, key: String, owner: usize) -> Self {
        Self {
            id,
            key,
            owner,
            unit_id: 0,
            squad_id: 0,
            respawn_at: 0,
            online: true,
            input: None,
            last_input: -100,
            last_seq: -1,
            shots: 0,
            hits: 0,
            kills: 0,
            seen: [None; 4],
            want: [false; 4],
        }
    }
    /// The server calls this on a fresh start or reconnect. Soldier and squad stay attached.
    pub fn reconnect(&mut self, online: bool) {
        self.online = online;
        self.last_seq = -1;
        self.input = None;
        self.seen = [None; 4];
        self.want = [false; 4];
    }
    pub fn disconnect(&mut self) {
        self.online = false;
        self.input = None;
    }
    /// Invalid frames leave every field unchanged. Tick and eligibility come from Match.
    pub(crate) fn accept(&mut self, value: &Value, tick: i64, eligible: bool) -> bool {
        if !self.online || !eligible {
            return false;
        }
        let Some(seq) = safe_integer(&value["seq"]) else {
            return false;
        };
        if seq <= self.last_seq {
            return false;
        }
        let numbers = ["forward", "strafe", "yaw", "pitch"].map(|name| value[name].as_f64());
        let [Some(forward), Some(strafe), Some(yaw), Some(pitch)] = numbers else {
            return false;
        };
        if ![forward, strafe, yaw, pitch].iter().all(|n| n.is_finite())
            || forward.abs() > 1.0
            || strafe.abs() > 1.0
            || yaw.abs() > 1e4
            || pitch.abs() > 1.45
        {
            return false;
        }
        let weapon = safe_integer(&value["weapon"])
            .filter(|n| (0..2).contains(n))
            .map(|n| n as usize)
            .unwrap_or_else(|| self.input.as_ref().map_or(0, |i| i.weapon));
        self.last_seq = seq;
        self.input = Some(Input {
            forward,
            strafe,
            yaw,
            pitch,
            weapon,
            fire: value["fire"] == true,
            ads: value["ads"] == true,
            crouch: value["crouch"] == true,
            sprint: value["sprint"] == true,
        });
        for (i, name) in EDGE_NAMES.iter().enumerate() {
            if let Some(n) = safe_integer(&value[*name]).filter(|n| *n >= 0) {
                if self.seen[i].is_some_and(|last| n > last) {
                    self.want[i] = true;
                }
                self.seen[i] = Some(n);
            }
        }
        self.last_input = tick;
        true
    }
}
fn safe_integer(value: &Value) -> Option<i64> {
    let n = value.as_f64()?;
    (n.is_finite() && n.abs() <= SAFE_INTEGER as f64 && n.fract() == 0.0).then_some(n as i64)
}

/// Fields absent from the canonical Unit and UnitControl rows.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Soldier {
    pub companion: CompanionId,
    pub vy: f64,
    pub jy: f64,
    pub crouch: bool,
    pub weapon: usize,
    pub mags: [u32; 2],
    pub reload: f64,
    pub nades: u32,
    pub nade_time: f64,
}
impl Soldier {
    pub(crate) fn new(companion: CompanionId, weapon: usize) -> Self {
        Self {
            companion,
            vy: 0.0,
            jy: 0.0,
            crouch: false,
            weapon,
            mags: WEAPONS.map(|w| w.mag),
            reload: 0.0,
            nades: OPERATIVE.nades,
            nade_time: 0.0,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Companions {
    /// Ascending IDs preserve original Map insertion order, independently of opaque keys.
    pub records: BTreeMap<CompanionId, Companion>,
    pub soldiers: BTreeMap<u32, Soldier>,
    keys: BTreeMap<String, CompanionId>,
}
impl Companions {
    pub fn get(&self, key: &str) -> Option<&Companion> {
        self.records.get(self.keys.get(key)?)
    }
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Companion> {
        self.records.get_mut(self.keys.get(key)?)
    }
    pub(crate) fn next_id(&self) -> Option<CompanionId> {
        self.records
            .last_key_value()
            .map_or(Some(1), |(id, _)| id.checked_add(1))
    }
    pub(crate) fn insert(&mut self, row: Companion) {
        self.keys.insert(row.key.clone(), row.id);
        self.records.insert(row.id, row);
    }
}

/// Algorithms owned by the combat integration are explicit requests, without simulated damage.
#[derive(Clone, Debug, PartialEq)]
pub enum CombatRequest {
    Grenade { unit: u32, input: Input },
    Fire { unit: u32, input: Input },
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StepResult {
    pub combat: Option<CombatRequest>,
}

/// The room adapter checks these source admission facts before asking Match to spawn.
/// Existing identities reconnect before this new-admission branch is reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Admission {
    pub human_commander: bool,
    pub identity_present: bool,
    pub companions_in_room: usize,
    pub world_mode: bool,
}
impl Admission {
    pub fn allowed(self) -> bool {
        self.human_commander
            && self.identity_present
            && self.companions_in_room < 4
            && !self.world_mode
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoleMessage {
    Input,
    Resync,
    Name,
    LatencyPing,
    ArmyOrder,
    HostAction,
    SeatChange,
    MapPing,
}
/// Room and pause gates still apply to input. Transport timing stays outside the rules core.
pub fn role_allows(message: RoleMessage) -> bool {
    matches!(
        message,
        RoleMessage::Input | RoleMessage::Resync | RoleMessage::Name | RoleMessage::LatencyPing
    )
}
