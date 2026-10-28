use std::{cell::RefCell, rc::Rc};

use crossterm::event::{KeyCode, KeyModifiers};

use crate::{
    event::LlimEvent,
    input::input_processor::InputProcessor,
    ui::{
        popup::write_popup::{
            CommentComposePopup, IssueFormPopup, LabelPickerPopup, QuickPickerPopup,
        },
        widget::shortcuts::{Shortcut, Shortcuts},
    },
};

pub struct QuickPickerProcessor {
    popup: Rc<RefCell<QuickPickerPopup>>,
}

impl QuickPickerProcessor {
    pub fn new(popup: Rc<RefCell<QuickPickerPopup>>) -> Self {
        Self { popup }
    }
}

impl InputProcessor for QuickPickerProcessor {
    fn apply(&mut self, event: &LlimEvent, ui_events: &mut Vec<LlimEvent>) {
        if let LlimEvent::InputKey(e) = event {
            let mut p = self.popup.borrow_mut();
            if e.modifiers.contains(KeyModifiers::CONTROL) {
                match e.code {
                    KeyCode::Char('n') | KeyCode::Char('N') => {
                        p.move_down(1);
                        return;
                    }
                    KeyCode::Char('p') | KeyCode::Char('P') => {
                        p.move_up(1);
                        return;
                    }
                    KeyCode::Char('u') | KeyCode::Char('U') => {
                        p.clear_filter();
                        return;
                    }
                    _ => {}
                }
            }

            match e.code {
                KeyCode::Esc => {
                    ui_events.push(LlimEvent::WriteModalClose);
                }
                KeyCode::Enter => {
                    if let Some(m) = p.selected_mutation() {
                        ui_events.push(LlimEvent::WriteModalClose);
                        ui_events.push(LlimEvent::SubmitMutation(Box::new(m)));
                    }
                }
                KeyCode::Down if e.modifiers.contains(KeyModifiers::SHIFT) => {
                    p.move_down(10);
                }
                KeyCode::Up if e.modifiers.contains(KeyModifiers::SHIFT) => {
                    p.move_up(10);
                }
                KeyCode::PageDown => {
                    p.move_down(10);
                }
                KeyCode::PageUp => {
                    p.move_up(10);
                }
                KeyCode::Down => {
                    p.move_down(1);
                }
                KeyCode::Up => {
                    p.move_up(1);
                }
                KeyCode::Backspace => {
                    p.pop_filter_char();
                }
                KeyCode::Char(c) if !e.modifiers.contains(KeyModifiers::ALT) => {
                    if let Some(m) = p.hotkey_mutation(c) {
                        ui_events.push(LlimEvent::WriteModalClose);
                        ui_events.push(LlimEvent::SubmitMutation(Box::new(m)));
                        return;
                    }
                    if p.filter_query.is_empty() {
                        match c {
                            'j' => {
                                p.move_down(1);
                                return;
                            }
                            'k' => {
                                p.move_up(1);
                                return;
                            }
                            'q' => {
                                ui_events.push(LlimEvent::WriteModalClose);
                                return;
                            }
                            _ => {}
                        }
                    }
                    p.push_filter_char(c);
                }
                _ => {}
            }
        }
    }

    fn shortcuts(&self) -> Shortcuts {
        Shortcuts::new(vec![
            Shortcut::new("↑/↓/j/k", "select"),
            Shortcut::new("type", "filter"),
            Shortcut::new("Enter", "apply"),
            Shortcut::new("Esc", "cancel"),
        ])
    }
}

pub struct LabelPickerProcessor {
    popup: Rc<RefCell<LabelPickerPopup>>,
}

impl LabelPickerProcessor {
    pub fn new(popup: Rc<RefCell<LabelPickerPopup>>) -> Self {
        Self { popup }
    }
}

impl InputProcessor for LabelPickerProcessor {
    fn apply(&mut self, event: &LlimEvent, ui_events: &mut Vec<LlimEvent>) {
        if let LlimEvent::InputKey(e) = event {
            let mut p = self.popup.borrow_mut();
            if p.creating_new {
                match e.code {
                    KeyCode::Esc => {
                        p.creating_new = false;
                        p.new_label_name.clear();
                    }
                    KeyCode::Enter => {
                        if let Some(m) = p.build_mutation() {
                            ui_events.push(LlimEvent::WriteModalClose);
                            ui_events.push(LlimEvent::SubmitMutation(Box::new(m)));
                        }
                    }
                    KeyCode::Backspace => {
                        p.new_label_name.pop();
                    }
                    KeyCode::Char(c)
                        if !e.modifiers.contains(KeyModifiers::CONTROL)
                            && !e.modifiers.contains(KeyModifiers::ALT) =>
                    {
                        p.new_label_name.push(c);
                    }
                    _ => {}
                }
                return;
            }

            match e.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    ui_events.push(LlimEvent::WriteModalClose);
                }
                KeyCode::Enter => {
                    if let Some(m) = p.build_mutation() {
                        ui_events.push(LlimEvent::WriteModalClose);
                        ui_events.push(LlimEvent::SubmitMutation(Box::new(m)));
                    }
                }
                KeyCode::Char(' ') => {
                    p.toggle_current();
                }
                KeyCode::Char('+') | KeyCode::Char('n') => {
                    p.creating_new = true;
                }
                KeyCode::Down if e.modifiers.contains(KeyModifiers::SHIFT) => {
                    p.move_down(10);
                }
                KeyCode::Up if e.modifiers.contains(KeyModifiers::SHIFT) => {
                    p.move_up(10);
                }
                KeyCode::PageDown => {
                    p.move_down(10);
                }
                KeyCode::PageUp => {
                    p.move_up(10);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    p.move_down(1);
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    p.move_up(1);
                }
                _ => {}
            }
        }
    }

    fn shortcuts(&self) -> Shortcuts {
        Shortcuts::new(vec![
            Shortcut::new("Space", "toggle label"),
            Shortcut::new("+", "new label"),
            Shortcut::new("Enter", "save"),
            Shortcut::new("Esc", "cancel"),
        ])
    }
}

pub struct CommentComposeProcessor {
    popup: Rc<RefCell<CommentComposePopup>>,
}

impl CommentComposeProcessor {
    pub fn new(popup: Rc<RefCell<CommentComposePopup>>) -> Self {
        Self { popup }
    }
}

impl InputProcessor for CommentComposeProcessor {
    fn apply(&mut self, event: &LlimEvent, ui_events: &mut Vec<LlimEvent>) {
        if let LlimEvent::InputKey(e) = event {
            let mut p = self.popup.borrow_mut();

            // 1. Handle two-step Esc chord (`Esc + Enter` = post to Linear, `Esc + Esc` = don't post to Linear, save as local draft & close)
            if p.esc_pending {
                p.esc_pending = false;
                match e.code {
                    KeyCode::Enter => {
                        if let Some(m) = p.build_mutation() {
                            for target in &p.targets {
                                ui_events.push(LlimEvent::ClearCommentDraft {
                                    issue_id: target.id.clone(),
                                    notify: false,
                                });
                            }
                            ui_events.push(LlimEvent::WriteModalClose);
                            ui_events.push(LlimEvent::SubmitMutation(Box::new(m)));
                        } else {
                            ui_events.push(LlimEvent::ActivityNotice(
                                "Comment body cannot be empty".into(),
                            ));
                        }
                        return;
                    }
                    KeyCode::Esc => {
                        // Esc + Esc = don't post to Linear; keep non-blank comment copy as draft (or clear if blank) and close
                        p.sync_body_from_editor();
                        for target in &p.targets {
                            ui_events.push(LlimEvent::SaveCommentDraft {
                                issue_id: target.id.clone(),
                                body: p.body.clone(),
                            });
                        }
                        ui_events.push(LlimEvent::WriteModalClose);
                        return;
                    }
                    _ => {
                        // Any other key cancels the Esc chord and is processed normally below
                    }
                }
            }

            // 2. Open external $EDITOR via Insert button (freeing Ctrl+e for Emacs end-of-line)
            if e.code == KeyCode::Insert {
                p.sync_body_from_editor();
                ui_events.push(LlimEvent::LaunchExternalEditor {
                    initial_text: p.body.clone(),
                });
                return;
            }

            // 3. Quick action with Control modifier (`Ctrl+x` to delete draft quickly)
            if e.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(e.code, KeyCode::Char('x') | KeyCode::Char('X'))
            {
                p.clear_draft_text();
                for target in &p.targets {
                    ui_events.push(LlimEvent::ClearCommentDraft {
                        issue_id: target.id.clone(),
                        notify: true,
                    });
                }
                return;
            }

            // 4. First Esc enters `esc_pending` chord state (while auto-persisting any non-empty text as draft)
            if e.code == KeyCode::Esc {
                p.sync_body_from_editor();
                for target in &p.targets {
                    ui_events.push(LlimEvent::SaveCommentDraft {
                        issue_id: target.id.clone(),
                        body: p.body.clone(),
                    });
                }
                p.esc_pending = true;
                return;
            }

            // 5. PageUp / PageDown for scrolling long comments
            match e.code {
                KeyCode::PageUp => {
                    p.scroll = p.scroll.saturating_sub(5);
                    return;
                }
                KeyCode::PageDown => {
                    p.scroll = p.scroll.saturating_add(5);
                    return;
                }
                _ => {}
            }

            // 6. Delegate all Emacs / terminal editing keys (`Ctrl+a`, `Ctrl+e`, `Ctrl+j`, `Shift+Enter`, etc.)
            if p.editor.handle_key(e, true) {
                p.sync_body_from_editor();
                p.is_draft = !p.body.trim().is_empty();
                for target in &p.targets {
                    ui_events.push(LlimEvent::SaveCommentDraft {
                        issue_id: target.id.clone(),
                        body: p.body.clone(),
                    });
                }
            }
        }
    }

    fn shortcuts(&self) -> Shortcuts {
        let p = self.popup.borrow();
        if p.esc_pending {
            return Shortcuts::new(vec![
                Shortcut::new("Enter", "post comment"),
                Shortcut::new("Esc", "close (keeps draft)"),
            ]);
        }
        Shortcuts::new(vec![
            Shortcut::new("Ins", "open $EDITOR"),
            Shortcut::new("Esc+Enter", "post"),
            Shortcut::new("Esc+Esc", "close (keeps draft)"),
            Shortcut::new("Ctrl+x", "delete draft"),
        ])
    }
}

pub struct IssueFormProcessor {
    popup: Rc<RefCell<IssueFormPopup>>,
}

impl IssueFormProcessor {
    pub fn new(popup: Rc<RefCell<IssueFormPopup>>) -> Self {
        Self { popup }
    }
}

impl InputProcessor for IssueFormProcessor {
    fn apply(&mut self, event: &LlimEvent, ui_events: &mut Vec<LlimEvent>) {
        if let LlimEvent::InputKey(e) = event {
            let mut p = self.popup.borrow_mut();

            if p.esc_pending {
                p.esc_pending = false;
                match e.code {
                    KeyCode::Enter => {
                        if let Some(m) = p.build_mutation() {
                            ui_events.push(LlimEvent::WriteModalClose);
                            ui_events.push(LlimEvent::SubmitMutation(Box::new(m)));
                        } else {
                            ui_events.push(LlimEvent::ActivityNotice(
                                "Issue title cannot be empty".into(),
                            ));
                        }
                        return;
                    }
                    KeyCode::Esc => {
                        ui_events.push(LlimEvent::WriteModalClose);
                        return;
                    }
                    _ => {}
                }
            }

            if e.code == KeyCode::Insert {
                p.sync_from_editors();
                ui_events.push(LlimEvent::LaunchExternalEditor {
                    initial_text: p.description.clone(),
                });
                return;
            }

            match e.code {
                KeyCode::Esc => {
                    p.esc_pending = true;
                    return;
                }
                KeyCode::Tab => {
                    p.next_field();
                    return;
                }
                KeyCode::BackTab => {
                    p.prev_field();
                    return;
                }
                KeyCode::Down if p.selected_field != 6 => {
                    p.next_field();
                    return;
                }
                KeyCode::Up if p.selected_field != 6 => {
                    p.prev_field();
                    return;
                }
                _ => {}
            }

            match p.selected_field {
                0 | 2 | 3 | 4 | 5 => match e.code {
                    KeyCode::Left => {
                        p.cycle_current_field(true);
                    }
                    KeyCode::Right | KeyCode::Char(' ') => {
                        p.cycle_current_field(false);
                    }
                    KeyCode::Enter => {
                        p.next_field();
                    }
                    _ => {}
                },
                1 => {
                    if e.code == KeyCode::Enter {
                        p.next_field();
                    } else if p.title_editor.handle_key(e, false) {
                        p.sync_from_editors();
                    }
                }
                6 => {
                    if p.description_editor.handle_key(e, true) {
                        p.sync_from_editors();
                    }
                }
                7 => {
                    if matches!(e.code, KeyCode::Enter | KeyCode::Char(' ')) {
                        if let Some(m) = p.build_mutation() {
                            ui_events.push(LlimEvent::WriteModalClose);
                            ui_events.push(LlimEvent::SubmitMutation(Box::new(m)));
                        } else {
                            ui_events.push(LlimEvent::ActivityNotice(
                                "Issue title cannot be empty".into(),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn shortcuts(&self) -> Shortcuts {
        let p = self.popup.borrow();
        if p.esc_pending {
            return Shortcuts::new(vec![
                Shortcut::new("Enter", "save & submit"),
                Shortcut::new("Esc", "cancel"),
            ]);
        }
        Shortcuts::new(vec![
            Shortcut::new("Tab/↑/↓", "field"),
            Shortcut::new("←/→", "cycle"),
            Shortcut::new("Ins", "$EDITOR"),
            Shortcut::new("Esc+Enter", "submit"),
            Shortcut::new("Esc+Esc", "cancel"),
        ])
    }
}
