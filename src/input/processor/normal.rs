use compact_str::CompactString;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEventKind};

use crate::{
    domain::ViewKind,
    event::LlimEvent,
    input::input_processor::InputProcessor,
    ui::widget::{
        input_field::TextEditor,
        shortcuts::{Shortcut, Shortcuts},
    },
};

#[derive(Debug, Default)]
pub struct NormalModeProcessor {
    pub active_view: ViewKind,
    pub filtering: bool,
    pub filter_editor: TextEditor,
    pub filter_text: CompactString,
    pub inspector_focused: bool,
    has_notification: bool,
}

impl NormalModeProcessor {
    pub fn new(active_view: ViewKind) -> Self {
        Self {
            active_view,
            filtering: false,
            filter_editor: TextEditor::new(),
            filter_text: CompactString::default(),
            inspector_focused: false,
            has_notification: false,
        }
    }

    pub fn set_has_notification(&mut self, has_notification: bool) {
        self.has_notification = has_notification;
    }

    fn process_filter_key(&mut self, e: &KeyEvent, ui_events: &mut Vec<LlimEvent>) {
        match e.code {
            KeyCode::Esc => {
                self.filtering = false;
                self.filter_editor.clear();
                self.filter_text.clear();
                ui_events.push(LlimEvent::FilterInputClear);
            }
            KeyCode::Enter => {
                self.filtering = false;
                self.filter_text = CompactString::from(self.filter_editor.as_str());
                ui_events.push(LlimEvent::FilterInputApply(self.filter_text.clone()));
            }
            _ => {
                if self.filter_editor.handle_key(e, false) {
                    self.filter_text = CompactString::from(self.filter_editor.as_str());
                    ui_events.push(LlimEvent::FilterInputChar(self.filter_text.clone()));
                }
            }
        }
    }

    fn process_normal_key(&mut self, e: &KeyEvent, ui_events: &mut Vec<LlimEvent>) {
        if e.modifiers.contains(KeyModifiers::CONTROL) {
            match e.code {
                KeyCode::Char('p') | KeyCode::Char('P') => {
                    ui_events.push(LlimEvent::OpenSelectedProjectDetails);
                    return;
                }
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    ui_events.push(LlimEvent::CursorPageDown);
                    return;
                }
                KeyCode::Char('u') | KeyCode::Char('U') => {
                    ui_events.push(LlimEvent::CursorPageUp);
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
            KeyCode::Char('1') => {
                self.active_view = ViewKind::Issues;
                ui_events.push(LlimEvent::SwitchView(ViewKind::Issues));
            }
            KeyCode::Char('2') => {
                self.active_view = ViewKind::Attention;
                ui_events.push(LlimEvent::SwitchView(ViewKind::Attention));
            }
            KeyCode::Char('3') => {
                self.active_view = ViewKind::Activity;
                ui_events.push(LlimEvent::SwitchView(ViewKind::Activity));
            }
            KeyCode::Char('[') => {
                self.active_view = self.active_view.prev();
                ui_events.push(LlimEvent::SwitchView(self.active_view));
            }
            KeyCode::Char(']') => {
                self.active_view = self.active_view.next();
                ui_events.push(LlimEvent::SwitchView(self.active_view));
            }
            KeyCode::Tab | KeyCode::BackTab => {
                ui_events.push(LlimEvent::FocusToggle);
            }
            KeyCode::Enter => {
                ui_events.push(LlimEvent::OpenSelectedDetails);
            }
            KeyCode::Char('a') => {
                ui_events.push(LlimEvent::ToggleActivityFeed);
            }
            KeyCode::Char('t') => {
                ui_events.push(LlimEvent::CycleTeamScope { reverse: false });
            }
            KeyCode::Char('T') => {
                ui_events.push(LlimEvent::CycleTeamScope { reverse: true });
            }
            KeyCode::Char('p') => {
                ui_events.push(LlimEvent::CycleProjectScope { reverse: false });
            }
            KeyCode::Char('P') => {
                ui_events.push(LlimEvent::CycleProjectScope { reverse: true });
            }
            KeyCode::Char('s') => {
                ui_events.push(LlimEvent::CycleIssueSort);
            }
            KeyCode::Char('h') => {
                ui_events.push(LlimEvent::ToggleShowCompleted);
            }
            KeyCode::Char('/') => {
                self.filtering = true;
                ui_events.push(LlimEvent::FilterInputStart);
            }
            KeyCode::Down if e.modifiers.contains(KeyModifiers::SHIFT) => {
                ui_events.push(LlimEvent::CursorPageDown);
            }
            KeyCode::Up if e.modifiers.contains(KeyModifiers::SHIFT) => {
                ui_events.push(LlimEvent::CursorPageUp);
            }
            KeyCode::Left if e.modifiers.contains(KeyModifiers::SHIFT) => {
                ui_events.push(LlimEvent::InspectorScrollUp(10));
            }
            KeyCode::Right if e.modifiers.contains(KeyModifiers::SHIFT) => {
                ui_events.push(LlimEvent::InspectorScrollDown(10));
            }
            KeyCode::Left => {
                ui_events.push(LlimEvent::InspectorScrollUp(3));
            }
            KeyCode::Right => {
                ui_events.push(LlimEvent::InspectorScrollDown(3));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                ui_events.push(LlimEvent::CursorNext);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                ui_events.push(LlimEvent::CursorPrevious);
            }
            KeyCode::Char('J') => {
                ui_events.push(LlimEvent::InspectorScrollDown(3));
            }
            KeyCode::Char('K') => {
                ui_events.push(LlimEvent::InspectorScrollUp(3));
            }
            KeyCode::PageDown => {
                ui_events.push(LlimEvent::CursorPageDown);
            }
            KeyCode::PageUp => {
                ui_events.push(LlimEvent::CursorPageUp);
            }
            KeyCode::Home | KeyCode::Char('g') => {
                ui_events.push(LlimEvent::CursorFirst);
            }
            KeyCode::End | KeyCode::Char('G') => {
                ui_events.push(LlimEvent::CursorLast);
            }
            KeyCode::Char('z') => {
                ui_events.push(LlimEvent::ToggleActivityBatchExpand);
            }
            KeyCode::Char(' ') => {
                if self.active_view == ViewKind::Activity {
                    ui_events.push(LlimEvent::ToggleActivityBatchExpand);
                } else {
                    ui_events.push(LlimEvent::ToggleSelectIssue { move_down: false });
                }
            }
            KeyCode::Char('v') => {
                ui_events.push(LlimEvent::ToggleSelectIssue { move_down: true });
            }
            KeyCode::Char('V') => {
                ui_events.push(LlimEvent::ToggleSelectAllVisible);
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
            KeyCode::Char('c') | KeyCode::Char('C') => {
                ui_events.push(LlimEvent::OpenCommentCompose);
            }
            KeyCode::Char('e') => {
                ui_events.push(LlimEvent::OpenEditIssueForm);
            }
            KeyCode::Char('I') => {
                ui_events.push(LlimEvent::OpenCreateIssueForm);
            }
            KeyCode::Char('y') => {
                ui_events.push(LlimEvent::CopySelectedIssueKey);
            }
            KeyCode::Char('b') => {
                ui_events.push(LlimEvent::CopySelectedGitBranch);
            }
            KeyCode::Char('w') | KeyCode::Char('o') => {
                ui_events.push(LlimEvent::OpenSelectedUrl);
            }
            KeyCode::Char('r') => {
                ui_events.push(LlimEvent::ManualRefresh);
            }
            KeyCode::Char('n') => {
                ui_events.push(LlimEvent::NotificationDismiss);
            }
            KeyCode::Char('N') => {
                ui_events.push(LlimEvent::NotificationRecallLast);
            }
            KeyCode::Char(',') | KeyCode::Char('O') | KeyCode::F(2) => {
                ui_events.push(LlimEvent::ConfigOpen);
            }
            KeyCode::Char('?') => {
                ui_events.push(LlimEvent::HelpOpen);
            }
            KeyCode::Esc => {
                if self.inspector_focused {
                    ui_events.push(LlimEvent::FocusToggle);
                } else if !self.filter_text.is_empty() {
                    self.filter_editor.clear();
                    self.filter_text.clear();
                    ui_events.push(LlimEvent::FilterInputClear);
                } else {
                    ui_events.push(LlimEvent::EscapeOrQuit);
                }
            }
            KeyCode::Char('q') => {
                ui_events.push(LlimEvent::Shutdown);
            }
            _ => {}
        }
    }
}

impl InputProcessor for NormalModeProcessor {
    fn apply(&mut self, event: &LlimEvent, ui_events: &mut Vec<LlimEvent>) {
        match event {
            LlimEvent::SwitchView(v) => {
                self.active_view = *v;
            }
            LlimEvent::InputKey(e) => {
                if self.filtering {
                    self.process_filter_key(e, ui_events);
                } else {
                    self.process_normal_key(e, ui_events);
                }
            }
            LlimEvent::InputMouse(m) => match m.kind {
                MouseEventKind::ScrollDown => ui_events.push(LlimEvent::CursorNext),
                MouseEventKind::ScrollUp => ui_events.push(LlimEvent::CursorPrevious),
                _ => {}
            },
            _ => {}
        }
    }

    fn shortcuts(&self) -> Shortcuts {
        if self.filtering {
            return Shortcuts::new(vec![
                Shortcut::new("Enter", "apply filter"),
                Shortcut::new("Esc", "clear filter"),
            ]);
        }
        if self.inspector_focused {
            return Shortcuts::new(vec![
                Shortcut::new("j/k", "scroll"),
                Shortcut::new("Tab/Esc", "table"),
                Shortcut::new("x", "state"),
                Shortcut::new("!", "priority"),
                Shortcut::new("m/A", "assign"),
                Shortcut::new("c", "comment"),
                Shortcut::new("e", "edit"),
                Shortcut::new("I", "new issue"),
                Shortcut::new(",", "config"),
                Shortcut::new("?", "help"),
            ]);
        }
        Shortcuts::new(vec![
            Shortcut::new("1/2/3", "views"),
            Shortcut::new("Enter", "open issue"),
            Shortcut::new("←/→", "scroll insp"),
            Shortcut::new("Spc/v/V", "select"),
            Shortcut::new("x", "state"),
            Shortcut::new("!", "pri"),
            Shortcut::new("m/A", "assign"),
            Shortcut::new("M", "proj"),
            Shortcut::new("l", "labels"),
            Shortcut::new("c", "comment"),
            Shortcut::new("e/I", "edit/new"),
            Shortcut::new("?", "help"),
        ])
    }
}
