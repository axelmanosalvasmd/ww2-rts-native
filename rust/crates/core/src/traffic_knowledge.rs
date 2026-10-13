//! Borrowed terrain for traffic decisions. Physical movement still reads Game.
use crate::{
    game::{CELL, Game, Point, Wreck},
    navigation::TerrainView,
    route_knowledge::{KnowledgeError, RouteKnowledge},
    rules::Rules,
};

/// The frozen workload uses physical terrain. Match callers supply the owner's
/// current remembered view and settled wreck projection without copying Game.
pub struct TrafficKnowledge<'a> {
    source: Source<'a>,
}
enum Source<'a> {
    Physical,
    Observed {
        navigation: TerrainView<'a>,
        knowledge: &'a RouteKnowledge,
        wrecks: &'a [Wreck],
    },
}
impl<'a> TrafficKnowledge<'a> {
    pub fn physical() -> Self {
        Self {
            source: Source::Physical,
        }
    }
    /// Both projections must come from the same canonical recipient knowledge.
    pub fn observed(
        navigation: TerrainView<'a>,
        knowledge: &'a RouteKnowledge,
        wrecks: &'a [Wreck],
    ) -> Result<Self, KnowledgeError> {
        let query = knowledge.query();
        let expected = query.navigation();
        if navigation.width != expected.width
            || navigation.height != expected.height
            || navigation.knowledge_id != expected.knowledge_id
            || !std::ptr::eq(navigation.flags, expected.flags)
            || match (navigation.heights, expected.heights) {
                (Some(a), Some(b)) => !std::ptr::eq(a, b),
                (None, None) => false,
                _ => true,
            }
        {
            return Err(KnowledgeError::InvalidShape);
        }
        Ok(Self {
            source: Source::Observed {
                navigation,
                knowledge,
                wrecks,
            },
        })
    }
    pub(crate) fn clear(
        &self,
        game: &Game,
        rules: &Rules,
        a: Point,
        b: Point,
        mask: u16,
        half: f64,
    ) -> bool {
        match &self.source {
            Source::Physical => crate::terrain::walkable(game, rules, a, b, mask, half),
            Source::Observed { navigation, .. } => {
                crate::pathfinding::walkable(*navigation, a, b, mask, half)
            }
        }
    }
    pub(crate) fn open(&self, game: &Game, mask: u16, x: f64, z: f64) -> bool {
        match &self.source {
            Source::Physical => game
                .cell_of(x, z)
                .is_some_and(|c| game.flags[c] & mask == 0),
            Source::Observed { navigation, .. } => {
                let cx = (x / CELL).floor();
                let cz = (z / CELL).floor();
                cx >= 0.0
                    && cz >= 0.0
                    && cx < navigation.width as f64
                    && cz < navigation.height as f64
                    && navigation.flags[cz as usize * navigation.width + cx as usize] & mask == 0
            }
        }
    }
    pub(crate) fn cover_rank(
        &self,
        game: &Game,
        rules: &Rules,
        at: Point,
        from: Option<Point>,
    ) -> Result<u8, KnowledgeError> {
        match &self.source {
            Source::Physical => Ok(crate::terrain::cover_rank(game, rules, at, from)),
            Source::Observed {
                knowledge, wrecks, ..
            } => match knowledge.cover_rank(at, from, wrecks) {
                // Off-map flags carry no cover, but nearby remembered walls or wrecks
                // can still supply directional cover in the source.
                Err(KnowledgeError::InvalidCell) => Ok(
                    if let Some(from) = from
                        && knowledge.cover_behind(at, from, wrecks)? > 0.4
                    {
                        2
                    } else {
                        3
                    },
                ),
                result => result,
            },
        }
    }
}

#[cfg(test)]
mod tests;
