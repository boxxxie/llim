use crate::{event::LlimEvent, ui::widget::shortcuts::Shortcuts};

pub trait InputProcessor {
    fn apply(&mut self, event: &LlimEvent, ui_events: &mut Vec<LlimEvent>);
    fn on_pop(&self) {}
    fn shortcuts(&self) -> Shortcuts;
}
