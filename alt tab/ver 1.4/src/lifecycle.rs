//! Ordering rules independent of the GUI, so shutdown races can be tested.
#[derive(Default)]
pub struct Lifecycle {
    configured: bool,
    grace_elapsed: bool,
    closing: bool,
    closed: bool,
    activation: Option<u64>,
}

#[derive(Debug, PartialEq)]
pub enum AfterClose { Ignore, Exit, Activate(u64) }

impl Lifecycle {
    pub fn configured(&mut self) { self.configured = true; }
    pub fn grace_elapsed(&mut self) { self.grace_elapsed = true; }
    pub fn is_configured(&self) -> bool { self.configured }
    pub fn is_closing(&self) -> bool { self.closing }
    pub fn visible(&self) -> bool { self.configured && self.grace_elapsed && !self.closing }

    pub fn begin_close(&mut self, activation: Option<u64>) -> bool {
        if self.closing { return false; }
        self.closing = true;
        self.activation = activation;
        true
    }

    pub fn surface_closed(&mut self) -> AfterClose {
        if self.closed { return AfterClose::Ignore; }
        self.closed = true;
        self.closing = true;
        self.activation.take().map_or(AfterClose::Exit, AfterClose::Activate)
    }

    pub fn close_timed_out(&self) -> bool { self.closing && !self.closed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_delay_alone_never_exposes_an_unconfigured_surface() {
        let mut lifecycle = Lifecycle::default();
        lifecycle.grace_elapsed();
        assert!(!lifecycle.visible());
        lifecycle.configured();
        assert!(lifecycle.visible());
    }

    #[test]
    fn early_configure_still_waits_for_grace_period() {
        let mut lifecycle = Lifecycle::default();
        lifecycle.configured();
        assert!(!lifecycle.visible());
        lifecycle.grace_elapsed();
        assert!(lifecycle.visible());
    }

    #[test]
    fn quick_release_stays_hidden_and_activates_once_after_teardown() {
        let mut lifecycle = Lifecycle::default();
        assert!(lifecycle.begin_close(Some(42)));
        lifecycle.configured();
        lifecycle.grace_elapsed();
        assert!(!lifecycle.visible());
        // Focus loss and duplicate release while closing must not overwrite selection.
        assert!(!lifecycle.begin_close(None));
        assert!(!lifecycle.begin_close(Some(99)));
        assert_eq!(lifecycle.surface_closed(), AfterClose::Activate(42));
        assert_eq!(lifecycle.surface_closed(), AfterClose::Ignore);
        assert!(!lifecycle.close_timed_out());
    }

    #[test]
    fn escape_and_unexpected_surface_close_never_activate() {
        let mut cancelled = Lifecycle::default();
        cancelled.begin_close(None);
        assert_eq!(cancelled.surface_closed(), AfterClose::Exit);
        assert_eq!(Lifecycle::default().surface_closed(), AfterClose::Exit);
    }

    #[test]
    fn watchdog_only_fires_for_an_unfinished_close() {
        let mut lifecycle = Lifecycle::default();
        assert!(!lifecycle.close_timed_out());
        lifecycle.begin_close(None);
        assert!(lifecycle.close_timed_out());
        lifecycle.surface_closed();
        assert!(!lifecycle.close_timed_out());
    }
}
