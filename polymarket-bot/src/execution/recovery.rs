use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupState {
    Booting,
    Preflight,
    Reconciling,
    Ready,
    Halted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAssessment {
    pub state: StartupState,
    pub mismatch_count: i64,
    pub unknown_remote_order_ids: Vec<String>,
    pub unresolved_local_order_ids: Vec<String>,
    pub local_without_remote_id: i64,
    pub unknown_remote_position_ids: Vec<String>,
    pub position_size_mismatch_ids: Vec<String>,
}

pub fn assess(
    local_non_terminal_remote_ids: &[String],
    local_without_remote_id: i64,
    remote_open_order_ids: &[String],
    remote_trade_order_ids: &[String],
    local_positions: &[(String, String)],
    remote_positions: &[(String, String)],
) -> RecoveryAssessment {
    let local: HashSet<_> = local_non_terminal_remote_ids.iter().cloned().collect();
    let remote_open: HashSet<_> = remote_open_order_ids.iter().cloned().collect();
    let remote_trades: HashSet<_> = remote_trade_order_ids.iter().cloned().collect();
    let unknown_remote_order_ids: Vec<_> = remote_open.difference(&local).cloned().collect();
    let unresolved_local_order_ids: Vec<_> = local
        .iter()
        .filter(|id| !remote_open.contains(*id) && !remote_trades.contains(*id))
        .cloned()
        .collect();
    let local_positions: HashMap<_, _> = local_positions.iter().cloned().collect();
    let remote_positions: HashMap<_, _> = remote_positions.iter().cloned().collect();
    let unknown_remote_position_ids: Vec<_> = remote_positions
        .keys()
        .filter(|id| !local_positions.contains_key(*id))
        .cloned()
        .collect();
    let position_size_mismatch_ids: Vec<_> = remote_positions
        .iter()
        .filter(|(id, remote_size)| {
            local_positions
                .get(*id)
                .is_some_and(|local_size| !decimal_strings_close(local_size, remote_size))
        })
        .map(|(id, _)| id.clone())
        .collect();
    let missing_remote_position_count = local_positions
        .keys()
        .filter(|id| !remote_positions.contains_key(*id))
        .count() as i64;
    let mismatch_count = unknown_remote_order_ids.len() as i64
        + unresolved_local_order_ids.len() as i64
        + local_without_remote_id
        + unknown_remote_position_ids.len() as i64
        + position_size_mismatch_ids.len() as i64
        + missing_remote_position_count;
    RecoveryAssessment {
        state: if mismatch_count == 0 {
            StartupState::Ready
        } else {
            StartupState::Halted
        },
        mismatch_count,
        unknown_remote_order_ids,
        unresolved_local_order_ids,
        local_without_remote_id,
        unknown_remote_position_ids,
        position_size_mismatch_ids,
    }
}

fn decimal_strings_close(left: &str, right: &str) -> bool {
    match (left.parse::<f64>(), right.parse::<f64>()) {
        (Ok(left), Ok(right)) => (left - right).abs() <= 0.000_001,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halts_on_unknown_remote_or_ambiguous_local_order() {
        let result = assess(&["local".into()], 1, &["remote".into()], &[], &[], &[]);
        assert_eq!(result.state, StartupState::Halted);
        assert_eq!(result.mismatch_count, 3);
    }

    #[test]
    fn accepts_local_order_seen_as_open_or_filled() {
        let result = assess(
            &["open".into(), "filled".into()],
            0,
            &["open".into()],
            &["filled".into()],
            &[],
            &[],
        );
        assert_eq!(result.state, StartupState::Ready);
        assert_eq!(result.mismatch_count, 0);
    }

    #[test]
    fn halts_on_unknown_or_different_remote_position() {
        let unknown = assess(&[], 0, &[], &[], &[], &[("remote".into(), "1.0".into())]);
        assert_eq!(unknown.state, StartupState::Halted);
        assert_eq!(unknown.unknown_remote_position_ids, vec!["remote"]);

        let mismatch = assess(
            &[],
            0,
            &[],
            &[],
            &[("token".into(), "1.0".into())],
            &[("token".into(), "0.5".into())],
        );
        assert_eq!(mismatch.state, StartupState::Halted);
        assert_eq!(mismatch.position_size_mismatch_ids, vec!["token"]);
    }
}
