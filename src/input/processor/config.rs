use std::{cell::RefCell, rc::Rc};

use crossterm::event::KeyCode;

use crate::{
    event::LlimEvent,
    input::input_processor::InputProcessor,
    ui::{
        popup::config_popup::ConfigPopup,
        widget::shortcuts::{Shortcut, Shortcuts},
    },
};

pub struct ConfigProcessor {
    popup: Rc<RefCell<ConfigPopup>>,
}

impl ConfigProcessor {
    pub fn new(popup: Rc<RefCell<ConfigPopup>>) -> Self {
        Self { popup }
    }
}

impl InputProcessor for ConfigProcessor {
    fn apply(&mut self, event: &LlimEvent, ui_events: &mut Vec<LlimEvent>) {
        if let LlimEvent::InputKey(e) = event {
            let mut popup = self.popup.borrow_mut();
            match e.code {
                KeyCode::Esc => {
                    ui_events.push(LlimEvent::ConfigClose);
                }
                KeyCode::Enter => {
                    let (cfg, persist_api_key) = popup.to_config();
                    ui_events.push(LlimEvent::ConfigApply {
                        config: cfg,
                        persist_api_key,
                    });
                }
                KeyCode::Down | KeyCode::Tab => {
                    popup.select_next_input();
                }
                KeyCode::Up | KeyCode::BackTab => {
                    popup.select_previous_input();
                }
                _ => {
                    popup.handle_key(e);
                }
            }
        }
    }

    fn shortcuts(&self) -> Shortcuts {
        Shortcuts::new(vec![
            Shortcut::new("Tab/↑/↓", "select field"),
            Shortcut::new("Space", "toggle bool"),
            Shortcut::new("Enter", "apply & save"),
            Shortcut::new("Esc", "cancel"),
        ])
    }
}
