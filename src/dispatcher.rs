use std::sync::mpsc;

use crate::event::LlimEvent;

pub trait Dispatcher {
    fn dispatch(&self, event: LlimEvent);
}

impl Dispatcher for mpsc::Sender<LlimEvent> {
    fn dispatch(&self, event: LlimEvent) {
        let _ = self.send(event);
    }
}
