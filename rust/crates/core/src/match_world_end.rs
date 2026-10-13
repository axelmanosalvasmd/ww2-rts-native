//! Source World finish after capture, home relocation and army elimination preparation.
//! The region actuator owns those prerequisites and refuses unrepresented fort/removal effects.
use crate::{
    catalog::{Catalog,Value}, economy::Mode, economy_state::EconomyState,
    game::{CELL,ModeKind,Point}, match_end::{EndReason,MatchFinish,MatchSample},
    match_runtime::Match, match_world::RegionState,
};
impl Match {
    /// Owned preparation retains no canonical borrow across region/wallet/home/story commits.
    /// This is internal to the region actuator, not a shortcut past its side-effect preflight.
    pub(crate) fn prepare_world_finish(
        &self, wallets: &EconomyState, regions: &[RegionState],
    ) -> Result<Option<(MatchFinish,MatchSample)>,String> {
        if self.options.mode != Mode::World || self.world.game.mode != Some(ModeKind::World) {
            return Err("World finish requires explicit canonical World mode".into());
        }
        if self.finish.is_some() { return Ok(None); }
        self.validate_match_finish_state()?;
        self.validate_sample_units()?;
        let geography=self.world.geography().ok_or("missing World geography")?;
        let total=geography.size.region_count();
        if wallets.players.len()!=self.start.economy.players.len() || regions.len()!=total {
            return Err("invalid staged World finish shape".into());
        }
        for (slot,p) in wallets.players.iter().enumerate() {
            if p.team!=self.start.economy.players[slot].team || [p.mp,p.mun,p.fuel,p.inc].into_iter().chain(p.fuel_inc).chain(p.upkeep).any(|v|!v.is_finite()) {
                return Err("invalid staged World finish commander".into());
            }
            if !regions.iter().any(|r|r.team==p.team) && (!p.out || self.world.game.units.values().any(|u|u.owner==slot)) {
                return Err("World finish requires completed army elimination".into());
            }
        }
        for r in regions {
            if !(-1..6).contains(&r.team) || !r.progress.is_finite() || !(0.0..=1.0).contains(&r.progress)
                || r.capper.is_some_and(|slot|slot>=wallets.players.len())
                || r.cap_tick.is_some_and(|tick|!(-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&tick)) {
                return Err("invalid staged World finish region".into());
            }
        }
        // Set insertion order is first player appearance, not sorted team IDs.
        let mut alive=Vec::new();let mut all=Vec::new();
        for p in &wallets.players {
            if !all.contains(&p.team){all.push(p.team);}
            if !p.out && !alive.contains(&p.team){alive.push(p.team);}
        }
        let tuning=Catalog::compiled().map_err(|e|e.to_string())?.table("WORLD_TUNING").map_err(|e|e.to_string())?;
        let share=tuning.get("winShare").and_then(Value::number).filter(|v|v.is_finite()).ok_or("invalid World win share")?;
        let goal=(total as f64*share).ceil();
        if !goal.is_finite() || goal<0.0 {return Err("invalid World goal".into());}
        let rivals=all.len()>1;
        let mut decision=if alive.is_empty(){Some((-1,EndReason::Draw))}else{None};
        if decision.is_none() {
            for team in &alive {
                let owned=regions.iter().filter(|r|r.team==*team).count();
                if owned==total || (rivals && owned as f64>=goal){decision=Some((*team,EndReason::World));break;}
            }
            if decision.is_none() && rivals && alive.len()==1 {decision=Some((alive[0],EndReason::Nations));}
        }
        let Some((winner,reason))=decision else{return Ok(None);};
        // The first region captured this tick supplies the location, even for another team.
        // Draw uses the source map-center fallback regardless of capture timestamps.
        let at=if winner==-1{None}else{regions.iter().position(|r|r.cap_tick==Some(self.world.game.tick)).map(|i|Point{
            x:(geography.regions[i].x as f64+0.5)*CELL,z:(geography.regions[i].y as f64+0.5)*CELL,
        })};
        Ok(Some(self.prepare_match_finish(winner,reason,at)?))
    }
}
