use std::{cell::RefCell, rc::Rc};

use crossterm::event::{KeyCode, KeyModifiers, MouseEventKind};

use crate::{
    domain::ViewKind,
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
        match event {
            LlimEvent::InputKey(e) => {
                if e.modifiers.contains(KeyModifiers::CONTROL) {
                    match e.code {
                        KeyCode::Char('d') | KeyCode::Char('D') => {
                            self.popup.borrow_mut().scroll_down(10);
                            return;
                        }
                        KeyCode::Char('u') | KeyCode::Char('U') => {
                            self.popup.borrow_mut().scroll_up(10);
                            return;
                        }
                        KeyCode::Char('p') | KeyCode::Char('P') => {
                            ui_events.push(LlimEvent::OpenSelectedProjectDetails);
                            return;
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') => {
                            ui_events.push(LlimEvent::OpenCreateIssueForm);
                            return;
                        }
                        KeyCode::Char('c') => {
                            ui_events.push(LlimEvent::Shutdown);
                            return;
                        }
                        _ => {}
                    }
                }

                match e.code {
                    KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('q') => {
                        ui_events.push(LlimEvent::IssueDetailsClose);
                    }
                    KeyCode::Char('1') => {
                        ui_events.push(LlimEvent::IssueDetailsClose);
                        ui_events.push(LlimEvent::SwitchView(ViewKind::Issues));
                    }
                    KeyCode::Char('2') => {
                        ui_events.push(LlimEvent::IssueDetailsClose);
                        ui_events.push(LlimEvent::SwitchView(ViewKind::Attention));
                    }
                    KeyCode::Char('3') => {
                        ui_events.push(LlimEvent::IssueDetailsClose);
                        ui_events.push(LlimEvent::SwitchView(ViewKind::Activity));
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
                    KeyCode::PageDown | KeyCode::Char('d') | KeyCode::Right => {
                        self.popup.borrow_mut().scroll_down(10);
                    }
                    KeyCode::PageUp | KeyCode::Char('u') | KeyCode::Left => {
                        self.popup.borrow_mut().scroll_up(10);
                    }
                    KeyCode::Home | KeyCode::Char('g') => {
                        self.popup.borrow_mut().scroll_top();
                    }
                    KeyCode::End | KeyCode::Char('G') => {
                        self.popup.borrow_mut().scroll_bottom();
                    }
                    KeyCode::Char('J') | KeyCode::Char(']') => {
                        ui_events.push(LlimEvent::IssueDetailsNext);
                    }
                    KeyCode::Char('K') | KeyCode::Char('[') => {
                        ui_events.push(LlimEvent::IssueDetailsPrev);
                    }
                    KeyCode::Char('c') | KeyCode::Char('C') => {
                        ui_events.push(LlimEvent::OpenCommentCompose);
                    }
                    KeyCode::Char('e') => {
                        ui_events.push(LlimEvent::OpenEditIssueForm);
                    }
                    KeyCode::Char('I') => {
                        ui_events.push(LlimEvent::OpenCreateIssueForm);
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
                    KeyCode::Char('r') => {
                        ui_events.push(LlimEvent::ManualRefresh);
                    }
                    KeyCode::Char(',') | KeyCode::Char('O') | KeyCode::F(2) => {
                        ui_events.push(LlimEvent::ConfigOpen);
                    }
                    KeyCode::Char('?') => {
                        ui_events.push(LlimEvent::HelpOpen);
                    }
                    _ => {}
                }
            }
            LlimEvent::InputMouse(m) => match m.kind {
                MouseEventKind::ScrollDown => self.popup.borrow_mut().scroll_down(3),
                MouseEventKind::ScrollUp => self.popup.borrow_mut().scroll_up(3),
                _ => {}
            },
            _ => {}
        }
    }

    fn shortcuts(&self) -> Shortcuts {
        Shortcuts::new(vec![
            Shortcut::new("Esc/Bksp", "back"),
            Shortcut::new("j/k", "scroll"),
            Shortcut::new("J/K", "next/prev"),
            Shortcut::new("x", "state"),
            Shortcut::new("!", "pri"),
            Shortcut::new("m/A", "assign"),
            Shortcut::new("M", "proj"),
            Shortcut::new("l", "labels"),
            Shortcut::new("c", "comment"),
            Shortcut::new("e", "edit"),
            Shortcut::new("b", "branch"),
            Shortcut::new("w", "open"),
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
