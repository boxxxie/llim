use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};

use crate::{
    gruvbox::Gruvbox,
    theme::theme,
    ui::{fx::window::Window, popup::utility::CenteredShrink},
};

#[derive(Debug, Default, Clone)]
pub struct HelpPopup {
    scroll: u16,
    quack_uri: Option<String>,
}

impl HelpPopup {
    pub fn new(quack_uri: Option<String>) -> Self {
        Self {
            scroll: 0,
            quack_uri,
        }
    }

    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_add(amount);
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_sub(amount);
    }

    pub fn popup_area(terminal_area: Rect) -> Rect {
        terminal_area.percent_centered(76, 80)
    }

    pub fn render_in(&self, area: Rect, buf: &mut Buffer) {
        let popup_rect = Self::popup_area(area);
        let title = Line::from(vec![
            Span::styled(" Help & Keybindings ", theme().border.title),
            Span::styled(" llim — Linear TUI Monitor ", theme().border_title),
        ]);

        let win = Window::new(title)
            .border_style(theme().border.config_border)
            .background(theme().background);
        let inner = win.inner(popup_rect);
        win.render(popup_rect, buf);

        let heading = Style::default()
            .fg(Gruvbox::OrangeBright.into())
            .add_modifier(Modifier::BOLD);

        let quack_line = self
            .quack_uri
            .as_deref()
            .unwrap_or("quack:127.0.0.1:9494");

        let rows: Vec<(&str, &str)> = vec![
            ("1 / 2 / 3", "Switch between 1: Issues, 2: My Attention, and 3: Activity Feed"),
            ("[ / ]", "Cycle backward / forward across the 3 views"),
            ("Enter", "Open dedicated full-page Issue view (Esc / Backspace to return to list)"),
            ("J / K (on Issue page)", "Step to next / previous issue in list while inside the dedicated Issue page"),
            ("Tab", "Toggle focus between the Top Table (50%) and Bottom Details Inspector (50%)"),
            ("← / → / J / K", "Scroll Bottom Inspector up (← / K) or down (→ / J) without leaving the table"),
            ("Shift+← / Shift+→", "Page-scroll Bottom Inspector up / down 10 lines without leaving the table"),
            ("Space / v / V", "Select/deselect issue (Space), select+advance (v), or toggle all visible (V)"),
            ("a", "Hide or show the Bottom 50% Details Inspector pane"),
            ("x / S", "Change Workflow State of selected issue(s) (supports bulk multi-select)"),
            ("! / i", "Change Priority of selected issue(s) (1=Urgent, 2=High, 3=Medium, 4=Low, 0=None)"),
            ("m / A", "Assign selected issue(s) to yourself ('m') or open Assignee picker ('A')"),
            ("M", "Move selected issue(s) to a different Project (or Unassigned)"),
            ("l", "Manage Labels on selected issue(s) (Space to toggle, '+' to create new label)"),
            ("c / C", "Post a Markdown comment on selected issue(s) (Ins for $EDITOR, Esc+Enter to send, Esc+Esc to close & keep draft, Ctrl+x to delete draft)"),
            ("e", "Edit selected issue Title, State, Priority, Assignee, Project, and Description (Ins for $EDITOR, Emacs keys supported)"),
            ("I / Ctrl+n", "Create a new Linear issue (Ins to edit Markdown description in $EDITOR)"),
            ("t / T", "Cycle Team scope forward / backward across configured teams"),
            ("p / P", "Cycle Project scope forward / backward (All <-> Unassigned <-> Projects)"),
            ("Ctrl+p", "Open Project Details Popup for the active project or selected issue"),
            ("s", "Cycle Issue sort order in View 1 (Updated↓, Created↓, Pri+Updated, State+Pri, ID)"),
            ("h", "Toggle visibility of Completed & Canceled issues"),
            ("/", "Substring filter across ID, Title, Project, Assignee, Labels, and Activity"),
            ("j / k / ↑ / ↓", "Move cursor in focused table, or scroll when Bottom Inspector is focused"),
            ("g / G / Home / End", "Jump to first / last row (or top / bottom of Inspector)"),
            ("Shift+↑ / Shift+↓", "Page up / Page down (also PgUp / PgDn / Ctrl+u / Ctrl+d)"),
            ("z / Space", "Expand or collapse folded multi-issue batch rows [+] in View 3: Activity"),
            ("y", "Copy selected issue identifier (e.g. ENG-142) to system clipboard"),
            ("b", "Copy selected issue gitBranchName (or generated branch slug) to clipboard"),
            ("w / o", "Open selected issue, project, or comment URL in default web browser"),
            ("r", "Trigger immediate manual delta sync with Linear GraphQL API"),
            ("n / N", "Dismiss active notification banner / Recall last notification"),
            (", / O / F2", "Open interactive Configuration Popup (~/.config/llim.toml)"),
            ("?", "Open or close this Help Popup"),
            ("q / Esc", "Unfocus Inspector, clear active filter, or quit llim"),
        ];

        let mut lines = Vec::with_capacity(rows.len() + 10);
        lines.push(Line::from(Span::styled("Keyboard Shortcuts", heading)));
        lines.push(Line::from(""));

        for (key, desc) in rows {
            lines.push(Line::from(vec![
                Span::styled(format!("  {:<20}", key), theme().issue_id),
                Span::styled(desc, theme().issue_title),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "DuckDB 2.0 + Quack Live Query Access",
            heading,
        )));
        lines.push(Line::from(vec![
            Span::styled("  Active Quack Server: ", theme().input_label),
            Span::styled(quack_line, theme().project_name),
        ]));
        lines.push(Line::from(Span::styled(
            "  Run `llim --print-quack-attach` in another terminal to print the exact DuckDB 2.0 ATTACH snippet.",
            theme().input_description,
        )));

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0))
            .render(inner, buf);
    }
}
