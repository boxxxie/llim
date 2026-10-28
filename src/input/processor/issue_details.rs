use std::{cell::RefCell, rc::Rc};

use crossterm::event::{KeyCode, KeyModifiers};

use crate::{
    event::LlimEvent,
    input::input_processor::InputProcessor,
    ui::{
        popup::issue_details_popup::IssueDetailsPopup,
        widget::shortcuts::{Shortcut, Shortcuts},
    },
};

pub struct IssueDetailsProcessor {
    popup: Rc<RefCell<IssueDetailsPopup>>,
}

impl IssueDetailsProcessor {
    pub fn new(popup: Rc<RefCell<IssueDetailsPopup>>) -> Self {
        Self { popup }
    }
}

impl InputProcessor for IssueDetailsProcessor {
    fn apply(&mut self, event: &LlimEvent, ui_events: &mut Vec<LlimEvent>) {
        if let LlimEvent::InputKey(e) = event {
            match e.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                    ui_events.push(LlimEvent::IssueDetailsClose);
                }
                KeyCode::Down if e.modifiers.contains(KeyModifiers::SHIFT) => {
                    self.popup.borrow_mut().scroll_down(10);
                }
                KeyCode::Up if e.modifiers.contains(KeyModifiers::SHIFT) => {
                    self.popup.borrow_mut().scroll_up(10);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.popup.borrow_mut().scroll_down(1);
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.popup.borrow_mut().scroll_up(1);
                }
                KeyCode::PageDown | KeyCode::Char('d') => {
                    self.popup.borrow_mut().scroll_down(10);
                }
                KeyCode::PageUp | KeyCode::Char('u') => {
                    self.popup.borrow_mut().scroll_up(10);
                }
                KeyCode::Char('c') | KeyCode::Char('C') => {
                    ui_events.push(LlimEvent::OpenCommentCompose);
                }
                KeyCode::Char('e') => {
                    ui_events.push(LlimEvent::OpenEditIssueForm);
                }
                KeyCode::Char('x') | KeyCode::Char('S') => {
                    ui_events.push(LlimEvent::OpenStatePicker);
                }
                KeyCode::Char('!') | KeyCode::Char('i') => {
                    ui_events.push(LlimEvent::OpenPriorityPicker);
                }
                KeyCode::Char('m') => {
                    ui_events.push(LlimEvent::AssignSelectedToSelf);
                }
                KeyCode::Char('A') => {
                    ui_events.push(LlimEvent::OpenAssigneePicker);
                }
                KeyCode::Char('M') => {
                    ui_events.push(LlimEvent::OpenProjectPicker);
                }
                KeyCode::Char('l') => {
                    ui_events.push(LlimEvent::OpenLabelPicker);
                }
                KeyCode::Char('y') => {
                    let p = self.popup.borrow();
                    ui_events.push(LlimEvent::CopyToClipboard {
                        label: "Issue ID".into(),
                        value: p.issue().identifier.clone(),
                    });
                }
                KeyCode::Char('b') => {
                    let p = self.popup.borrow();
                    let branch = p.issue().git_branch_name.clone().unwrap_or_else(|| {
                        format!(
                            "feat/{}-{}",
                            p.issue().identifier.to_ascii_lowercase(),
                            slugify(&p.issue().title)
                        )
                        .into()
                    });
                    ui_events.push(LlimEvent::CopyToClipboard {
                        label: "Git branch".into(),
                        value: branch,
                    });
                }
                KeyCode::Char('w') | KeyCode::Char('o') => {
                    let url = self.popup.borrow().issue().url.clone();
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
            Shortcut::new("y", "copy ID"),
            Shortcut::new("b", "copy branch"),
            Shortcut::new("w", "open URL"),
            Shortcut::new("Esc", "close"),
        ])
    }
}

pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
        if out.len() >= 36 {
            break;
        }
    }
    out.trim_end_matches('-').to_string()
}
