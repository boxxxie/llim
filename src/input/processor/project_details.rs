use std::{cell::RefCell, rc::Rc};

use crossterm::event::{KeyCode, KeyModifiers};

use crate::{
    event::LlimEvent,
    input::input_processor::InputProcessor,
    ui::{
        popup::project_details_popup::ProjectDetailsPopup,
        widget::shortcuts::{Shortcut, Shortcuts},
    },
};

pub struct ProjectDetailsProcessor {
    popup: Rc<RefCell<ProjectDetailsPopup>>,
}

impl ProjectDetailsProcessor {
    pub fn new(popup: Rc<RefCell<ProjectDetailsPopup>>) -> Self {
        Self { popup }
    }
}

impl InputProcessor for ProjectDetailsProcessor {
    fn apply(&mut self, event: &LlimEvent, ui_events: &mut Vec<LlimEvent>) {
        if let LlimEvent::InputKey(e) = event {
            match e.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                    ui_events.push(LlimEvent::ProjectDetailsClose);
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
                KeyCode::Char('w') | KeyCode::Char('o') => {
                    let url = self.popup.borrow().project().url.clone();
                    if !url.is_empty() {
                        ui_events.push(LlimEvent::OpenUrl(url));
                    }
                }
                _ => {}
            }
        }
    }

    fn shortcuts(&self) -> Shortcuts {
        Shortcuts::new(vec![
            Shortcut::new("j/k", "scroll"),
            Shortcut::new("w", "open URL"),
            Shortcut::new("Esc", "close"),
        ])
    }
}
