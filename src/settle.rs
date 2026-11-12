//! Holds a recalculation back while a slider is dragged. A job for every frame of a drag keeps every core busy, and
//! each result that lands costs the UI thread time, so the slider stops following the pointer.
use eframe::egui;

/// The default seconds a held slider's value must stay the same before it is calculated.
pub const REST: f64 = 0.2;
/// The longest delay the Preview panel offers.
pub const REST_MAX: f64 = 1.0;

/// The delay in use: the Preview panel's **Update delay**, or `REST` until it is set.
pub fn rest(ctx: &egui::Context) -> f64 {
    ctx.data(|d| d.get_temp(egui::Id::new("update-delay"))).unwrap_or(REST)
}

pub fn set_rest(ctx: &egui::Context, seconds: f64) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("update-delay"), seconds.clamp(0.0, REST_MAX)));
}

/// When a changed value was last seen changing.
pub struct Settle<T> {
    last: Option<T>,
    at: f64,
}

impl<T> Default for Settle<T> {
    fn default() -> Self {
        Self { last: None, at: 0.0 }
    }
}

impl<T: PartialEq + Clone> Settle<T> {
    /// Call every frame; `pending` says `value` differs from the one last calculated. True when it should be
    /// calculated now: at once with the pointer button up (a click, a key, a drag let go), and with it down once the
    /// value has rested for `rest` seconds. `now` is egui's time in seconds.
    pub fn due(&mut self, pending: bool, value: &T, held: bool, now: f64, rest: f64) -> bool {
        if self.last.as_ref() != Some(value) {
            self.last = Some(value.clone());
            self.at = now;
        }
        pending && (!held || now - self.at >= rest)
    }
}

/// `Settle::due` with the pointer, the time and the delay read from `ui`; asks for the frame the rest ends on when
/// not due.
pub fn due<T: PartialEq + Clone>(ui: &egui::Ui, settle: &mut Settle<T>, pending: bool, value: &T) -> bool {
    let (held, now) = ui.input(|i| (i.pointer.primary_down(), i.time));
    let secs = rest(ui.ctx());
    let due = settle.due(pending, value, held, now, secs);
    if pending && !due {
        ui.ctx().request_repaint_after_secs(secs as f32);
    }
    due
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_value_waits_until_it_has_rested() {
        let mut s = Settle::default();
        assert!(!s.due(true, &1, true, 0.0, REST));
        assert!(!s.due(true, &2, true, 0.1, REST), "it moved, so the rest starts again");
        assert!(!s.due(true, &2, true, 0.1 + REST - 0.01, REST));
        assert!(s.due(true, &2, true, 0.1 + REST, REST));
    }

    #[test]
    fn a_value_that_is_not_held_goes_at_once() {
        let mut s = Settle::default();
        assert!(s.due(true, &1, false, 0.0, REST));
        assert!(!s.due(true, &2, true, 1.0, REST));
        assert!(s.due(true, &2, false, 1.01, REST), "let go before the rest ended");
    }

    #[test]
    fn a_value_the_slider_comes_back_to_rests_again() {
        let mut s = Settle::default();
        assert!(!s.due(true, &2, true, 0.0, REST));
        assert!(!s.due(false, &1, false, 0.1, REST), "back at the calculated value: nothing to calculate");
        assert!(!s.due(true, &2, true, 1.0, REST), "the rest before it left does not count");
    }

    #[test]
    fn the_rest_is_the_callers() {
        let mut s = Settle::default();
        assert!(s.due(true, &1, true, 0.0, 0.0), "no delay: every move is calculated");
        assert!(!s.due(true, &2, true, 0.1, 1.0));
        assert!(!s.due(true, &2, true, 1.0, 1.0));
        assert!(s.due(true, &2, true, 1.1, 1.0));
    }

    #[test]
    fn the_delay_is_kept_per_context_and_clamped() {
        let ctx = egui::Context::default();
        assert_eq!(rest(&ctx), REST);
        set_rest(&ctx, 0.5);
        assert_eq!(rest(&ctx), 0.5);
        set_rest(&ctx, 9.0);
        assert_eq!(rest(&ctx), REST_MAX);
    }
}
