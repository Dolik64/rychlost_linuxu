//! Per-window activation history. Never identify windows by title or app_id.
use std::{cmp::Reverse, collections::HashMap};

#[derive(Default)]
pub struct History {
    clock: u64,
    windows: HashMap<u64, (bool, u64)>,
}
impl History {
    pub fn open(&mut self, id: u64) { self.windows.entry(id).or_insert((false, 0)); }
    pub fn close(&mut self, id: u64) { self.windows.remove(&id); }
    pub fn state(&mut self, id: u64, activated: bool) {
        let Some((was_active, last_used)) = self.windows.get_mut(&id) else { return; };
        if activated && !*was_active {
            self.clock += 1;
            *last_used = self.clock;
        }
        *was_active = activated;
    }
    pub fn sort_key(&self, id: u64) -> (Reverse<u64>, u64) {
        (Reverse(self.windows.get(&id).map(|w| w.1).unwrap_or(0)), id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn order(h: &History) -> Vec<u64> {
        let mut ids: Vec<_> = h.windows.keys().copied().collect();
        ids.sort_by_key(|id| h.sort_key(*id)); ids
    }
    fn focus(h: &mut History, id: u64) {
        let ids: Vec<_> = h.windows.keys().copied().collect();
        for i in ids { h.state(i, i == id); }
    }
    #[test]
    fn settings_becomes_previous_after_firefox() {
        let mut h = History::default(); for id in 1..=3 { h.open(id); }
        focus(&mut h, 1); focus(&mut h, 3); focus(&mut h, 2);
        assert_eq!(order(&h), vec![2, 3, 1]);
        let next = order(&h)[1];
        focus(&mut h, next); // rapid Alt+Tab
        assert_eq!(order(&h), vec![3, 2, 1]);
        focus(&mut h, 2); assert_eq!(order(&h), vec![2, 3, 1]);
    }
    #[test]
    fn metadata_and_duplicate_activated_do_not_reorder() {
        let mut h = History::default(); h.open(1); h.open(2);
        h.state(1, true); h.state(2, true); // events can arrive before old deactivation
        h.state(1, true); assert_eq!(order(&h), vec![2, 1]);
    }
    #[test]
    fn overlay_focus_loss_and_cancel_preserve_history() {
        let mut h = History::default(); h.open(1); h.open(2);
        focus(&mut h, 1); focus(&mut h, 2);
        h.state(2, false); assert_eq!(order(&h), vec![2, 1]);
        h.state(2, true); assert_eq!(order(&h), vec![2, 1]);
    }
    #[test]
    fn frozen_snapshot_and_closed_window_ids() {
        let mut h = History::default(); for id in 1..=3 { h.open(id); }
        focus(&mut h, 1); focus(&mut h, 2);
        let frozen = order(&h);
        h.close(2); h.open(4); focus(&mut h, 4);
        assert_eq!(frozen, vec![2, 1, 3]);
        assert_eq!(order(&h), vec![4, 1, 3]);
        h.state(2, true); assert_eq!(order(&h), vec![4, 1, 3]);
    }
    #[test]
    fn two_windows_of_same_application_have_independent_history() {
        let mut h = History::default(); h.open(1); h.open(2);
        focus(&mut h, 2); focus(&mut h, 1);
        assert_eq!(order(&h), vec![1, 2]);
        focus(&mut h, 2); assert_eq!(order(&h), vec![2, 1]);
    }
    #[test]
    fn startup_unknown_history_is_deterministic() {
        let mut h = History::default(); for id in [3, 1, 2] { h.open(id); }
        assert_eq!(order(&h), vec![1, 2, 3]);
        focus(&mut h, 3); assert_eq!(order(&h), vec![3, 1, 2]);
    }
}
