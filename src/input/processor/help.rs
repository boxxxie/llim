use std::{cell::RefCell, rc::Rc};

use crossterm::event::{KeyCode, KeyModifiers};

use crate::{
    event::LlimEvent,
    input::input_processor::InputProcessor,
    ui::{
        popup::help_popup::HelpPopup,
        widget::shortcuts::{Shortcut, Shortcuts},
    },
};

pub struct HelpProcessor {
    popup: Rc<RefCell<HelpPopup>>,
}

impl HelpProcessor {
    pub fn new(popup: Rc<RefCell<HelpPopup>>) -> Self {
        Self { popup }
    }
}

impl InputProcessor for HelpProcessor {
    fn apply(&mut self, event: &LlimEvent, ui_events: &mut Vec<LlimEvent>) {
        if let LlimEvent::InputKey(e) = event {
            match e.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') | KeyCode::Enter => {
                    ui_events.push(LlimEvent::HelpClose);
                }
                KeyCode::Down if e.modifiers.contains(KeyModifiers::SHIFT) => {
                    self.popup.borrow_mut().scroll_down(10);
                }
                KeyCode::Up if e.modifiers.contains(KeyModifiers::SHIFT) => {
                    self.popup.borrow_mut().scroll_up(10);
                }
                KeyCode::PageDown => {
                    self.popup.borrow_mut().scroll_down(10);
                }
                KeyCode::PageUp => {
                    self.popup.borrow_mut().scroll_up(10);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.popup.borrow_mut().scroll_down(1);
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.popup.borrow_mut().scroll_up(1);
                }
                _ => {}
            }
        }
    }

    fn shortcuts(&self) -> Shortcuts {
        Shortcuts::new(vec![
            Shortcut::new("j/k", "scroll"),
            Shortcut::new("Esc/?", "close"),
        ])
    }
}
