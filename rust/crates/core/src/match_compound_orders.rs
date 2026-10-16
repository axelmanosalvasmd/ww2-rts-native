//! Compound clicks reuse ordinary command admission, issuer and queue semantics.
use crate::{
    match_commands::{Command, Denial},
    match_runtime::Match,
};

impl Command {
    pub(crate) fn compound_leaves(
        parts: &[serde_json::Value],
        queue: bool,
    ) -> Vec<Result<Self, Denial>> {
        parts
            .iter()
            .take(8)
            .filter_map(|part| {
                let object = part.as_object()?;
                if !matches!(
                    object.get("t").and_then(|v| v.as_str()),
                    Some(
                        "move"
                            | "amove"
                            | "attack"
                            | "garrison"
                            | "fireat"
                            | "escort"
                            | "assist"
                            | "rally"
                            | "entrench"
                            | "board"
                    )
                ) {
                    return None;
                }
                let mut leaf = object.clone();
                leaf.insert("queue".into(), serde_json::Value::Bool(queue));
                Some(Self::from_json(&serde_json::Value::Object(leaf)))
            })
            .collect()
    }
}

impl Match {
    pub(crate) fn execute_compound_orders(
        &mut self,
        owner: usize,
        parts: &[serde_json::Value],
        queue: bool,
        winner_decided: bool,
    ) -> Result<Option<Denial>, String> {
        let mut done = false;
        let mut reason = None;
        for leaf in Command::compound_leaves(parts, queue) {
            let denial = match leaf {
                Ok(command) => self.execute_command(owner, &command, winner_decided)?,
                Err(denial) => Some(denial),
            };
            if let Some(denial) = denial {
                reason.get_or_insert(denial);
            } else {
                done = true;
            }
        }
        Ok(if done {
            None
        } else {
            Some(reason.unwrap_or(Denial::Blocked))
        })
    }
}
