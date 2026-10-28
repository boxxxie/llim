use std::{cell::RefCell, rc::Rc};

use crossterm::event::KeyCode;

use crate::{
    event::LlimEvent,
    input::input_processor::InputProcessor,
    ui::{
        popup::activity_details_popup::ActivityDetailsPopup,
        widget::shortcuts::{Shortcut, Shortcuts},
    },
};

pub struct ActivityDetailsProcessor {
    popup: Rc<RefCell<ActivityDetailsPopup>>,
}

impl ActivityDetailsProcessor {
    pub fn new(popup: Rc<RefCell<ActivityDetailsPopup>>) -> Self {
        Self { popup }
    }
}

impl InputProcessor for ActivityDetailsProcessor {
    fn apply(&mut self, event: &LlimEvent, ui_events: &mut Vec<LlimEvent>) {
        if let LlimEvent::InputKey(e) = event {
            match e.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    ui_events.push(LlimEvent::ActivityDetailsClose);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.popup.borrow_mut().scroll_down(1);
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.popup.borrow_mut().scroll_up(1);
                }
                KeyCode::Enter | KeyCode::Char('i') => {
                    let issue_id = self.popup.borrow().activity().issue_id.clone();
                    ui_events.push(LlimEvent::ActivityDetailsClose);
                    ui_events.push(LlimEvent::IssueDetailsOpen(issue_id));
                }
                KeyCode::Char('y') => {
                    let ident = self.popup.borrow().activity().issue_identifier.clone();
                    ui_events.push(LlimEvent::CopyToClipboard {
                        label: "Issue ID".into(),
                        value: ident,
                    });
                }
                KeyCode::Char('w') | KeyCode::Char('o') => {
                    let url = self.popup.borrow().activity().url.clone();
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
            Shortcut::new("Enter", "open issue"),
            Shortcut::new("y", "copy ID"),
            Shortcut::new("w", "open URL"),
            Shortcut::new("Esc", "close"),
        ])
    }
}
