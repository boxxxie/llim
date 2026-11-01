use chrono::{DateTime, Local, Utc};
use compact_str::{CompactString, format_compact};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::{
    domain::{Issue, StateCategory},
    gruvbox::Gruvbox,
    theme::theme,
};

pub mod fx;
pub mod markdown;
pub mod popup;
pub mod stateful_widgets;
pub mod widget;

pub fn format_relative_time(dt: DateTime<Utc>) -> CompactString {
    let now = Utc::now();
    let delta = now.signed_duration_since(dt);
    let secs = delta.num_seconds();
    if secs < 60 {
        format_compact!("{}s ago", secs.max(0))
    } else if secs < 3600 {
        format_compact!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format_compact!("{}h ago", secs / 3600)
    } else if secs < 86_400 * 30 {
        format_compact!("{}d ago", secs / 86_400)
    } else {
        let local: DateTime<Local> = dt.into();
        local.format("%Y-%m-%d").to_string().into()
    }
}

pub fn render_epic_and_relations_section(issue: &Issue, indent: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let indent_owned = indent.to_string();

    if issue.is_sub_issue() {
        let parent_id = issue
            .parent_identifier
            .as_deref()
            .unwrap_or("Parent Epic");
        let parent_title = issue.parent_title.as_deref().unwrap_or("");
        let mut spans = vec![
            Span::raw(indent_owned.clone()),
            Span::styled("▍ ", Style::default().fg(Gruvbox::AquaBright.into())),
            Span::styled("↳ Part of Epic: ", theme().sub_issue_badge),
            Span::styled(format!("◆ {parent_id}"), theme().epic_badge),
        ];
        if !parent_title.is_empty() {
            spans.push(Span::styled(" — ", theme().date));
            spans.push(Span::styled(parent_title.to_string(), theme().epic_title));
        }
        lines.push(Line::from(spans));
        lines.push(Line::from(""));
    }

    if let Some(bd) = issue.epic_breakdown() {
        if !issue.sub_issues.is_empty() {
            let total = bd.sub_issues_total;
            let done = bd.sub_issues_done;
            let in_prog = bd.sub_issues_in_progress;
            let todo = bd.sub_issues_todo;
            let canceled = bd.sub_issues_canceled;
            let active_total = total.saturating_sub(canceled).max(1);
            let pct = ((done as f64 / active_total as f64) * 100.0).round() as usize;

            let bar_width = 20usize;
            let done_cells = ((done * bar_width) / active_total).min(bar_width);
            let prog_cells = ((in_prog * bar_width) / active_total).min(bar_width - done_cells);
            let rem_cells = bar_width.saturating_sub(done_cells + prog_cells);

            lines.push(Line::from(vec![
                Span::raw(indent_owned.clone()),
                Span::styled("▍ ", Style::default().fg(Gruvbox::PurpleBright.into())),
                Span::styled(
                    format!("◆ Epic Rollup & Sub-Issues ({done}/{active_total} completed)"),
                    theme().epic_badge,
                ),
            ]));

            let mut bar_spans = vec![
                Span::raw(format!("{indent_owned}  [")),
                Span::styled(
                    "█".repeat(done_cells),
                    Style::default().fg(Gruvbox::GreenBright.into()),
                ),
                Span::styled(
                    "▓".repeat(prog_cells),
                    Style::default().fg(Gruvbox::YellowBright.into()),
                ),
                Span::styled(
                    "░".repeat(rem_cells),
                    Style::default().fg(Gruvbox::Dark3.into()),
                ),
                Span::raw("] "),
                Span::styled(
                    format!("{pct}%"),
                    Style::default()
                        .fg(Gruvbox::Light1.into())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  ({done} Done • {in_prog} In Progress • {todo} Todo"),
                    theme().date,
                ),
            ];
            if canceled > 0 {
                bar_spans.push(Span::styled(
                    format!(" • {canceled} Canceled)"),
                    theme().date,
                ));
            } else {
                bar_spans.push(Span::styled(")", theme().date));
            }
            lines.push(Line::from(bar_spans));

            let sub_count = issue.sub_issues.len();
            for (idx, sub) in issue.sub_issues.iter().enumerate() {
                let connector = if idx + 1 == sub_count {
                    "└─ "
                } else {
                    "├─ "
                };
                let (glyph, glyph_style) = match sub.state_category {
                    StateCategory::Completed => (
                        "✓",
                        Style::default()
                            .fg(Gruvbox::GreenBright.into())
                            .add_modifier(Modifier::BOLD),
                    ),
                    StateCategory::Started => (
                        "●",
                        Style::default()
                            .fg(Gruvbox::YellowBright.into())
                            .add_modifier(Modifier::BOLD),
                    ),
                    StateCategory::Canceled => ("⊘", Style::default().fg(Gruvbox::Dark4.into())),
                    StateCategory::Triage => (
                        "◇",
                        Style::default().fg(Gruvbox::OrangeBright.into()),
                    ),
                    StateCategory::Unstarted | StateCategory::Backlog => {
                        ("○", Style::default().fg(Gruvbox::BlueBright.into()))
                    }
                };
                let state_style =
                    theme().state_category_style(sub.state_category, Some(&sub.state_color));
                let assignee = sub.assignee_name.as_deref().unwrap_or("Unassigned");
                lines.push(Line::from(vec![
                    Span::styled(format!("{indent_owned}  {connector}"), theme().date),
                    Span::styled(format!("{glyph} "), glyph_style),
                    Span::styled(format!("{:<8} ", sub.identifier), theme().issue_id),
                    Span::styled(format!("{:<12} ", sub.state_name), state_style),
                    Span::styled(
                        format!("{:<4} ", sub.priority.short_label()),
                        theme().priority_style(sub.priority),
                    ),
                    Span::styled(format!("{:<12} ", assignee), theme().assignee),
                    Span::styled(sub.title.to_string(), theme().issue_title),
                ]));
            }
            lines.push(Line::from(""));
        } else {
            lines.push(Line::from(vec![
                Span::raw(indent_owned.clone()),
                Span::styled("▍ ", Style::default().fg(Gruvbox::PurpleBright.into())),
                Span::styled("◆ Umbrella Epic Breakdown", theme().epic_badge),
            ]));

            if bd.checklist_total > 0 {
                let bar_width = 12usize;
                let done_cells = ((bd.checklist_done * bar_width) / bd.checklist_total.max(1))
                    .min(bar_width);
                let rem_cells = bar_width.saturating_sub(done_cells);
                lines.push(Line::from(vec![
                    Span::raw(format!("{indent_owned}  [")),
                    Span::styled(
                        "█".repeat(done_cells),
                        Style::default().fg(Gruvbox::GreenBright.into()),
                    ),
                    Span::styled(
                        "░".repeat(rem_cells),
                        Style::default().fg(Gruvbox::Dark3.into()),
                    ),
                    Span::raw("] "),
                    Span::styled(
                        format!(
                            "{}/{} prerequisites completed",
                            bd.checklist_done, bd.checklist_total
                        ),
                        Style::default()
                            .fg(Gruvbox::YellowBright.into())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" • Unsplit (0 sub-issues linked)", theme().date),
                ]));
                let count = bd.checklist_items.len();
                for (idx, (checked, text)) in bd.checklist_items.iter().enumerate() {
                    let connector = if idx + 1 == count { "└─ " } else { "├─ " };
                    let (box_str, box_style) = if *checked {
                        (
                            "[✓] ",
                            Style::default()
                                .fg(Gruvbox::GreenBright.into())
                                .add_modifier(Modifier::BOLD),
                        )
                    } else {
                        ("[ ] ", Style::default().fg(Gruvbox::OrangeBright.into()))
                    };
                    lines.push(Line::from(vec![
                        Span::styled(format!("{indent_owned}  {connector}"), theme().date),
                        Span::styled(box_str, box_style),
                        Span::styled(text.clone(), theme().issue_title),
                    ]));
                }
            } else if !bd.milestones.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw(format!("{indent_owned}  ")),
                    Span::styled(
                        format!(
                            "◆ Umbrella Epic (0 sub-issues linked) • {} planned milestones to split before Todo",
                            bd.milestones.len()
                        ),
                        Style::default()
                            .fg(Gruvbox::YellowBright.into())
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
                let count = bd.milestones.len();
                for (idx, (num, text)) in bd.milestones.iter().enumerate() {
                    let connector = if idx + 1 == count { "└─ " } else { "├─ " };
                    lines.push(Line::from(vec![
                        Span::styled(format!("{indent_owned}  {connector}"), theme().date),
                        Span::styled(format!("[{num}] "), theme().epic_badge),
                        Span::styled(text.clone(), theme().issue_title),
                    ]));
                }
            } else {
                lines.push(Line::from(vec![
                    Span::raw(format!("{indent_owned}  ")),
                    Span::styled(
                        "◆ Umbrella Epic (0 sub-issues linked — decompose into child tickets before moving to Todo)",
                        theme().date,
                    ),
                ]));
            }
            lines.push(Line::from(""));
        }
    }

    if !issue.related_issues.is_empty() {
        lines.push(Line::from(vec![
            Span::raw(indent_owned.clone()),
            Span::styled("▍ ", Style::default().fg(Gruvbox::Orange.into())),
            Span::styled(
                format!("Related Issues ({})", issue.related_issues.len()),
                theme().border_title,
            ),
        ]));
        for rel in &issue.related_issues {
            let state_style =
                theme().state_category_style(rel.state_category, Some(&rel.state_color));
            lines.push(Line::from(vec![
                Span::styled(format!("{indent_owned}  ↔ "), theme().scope_pill),
                Span::styled(format!("{:<8} ", rel.identifier), theme().issue_id),
                Span::styled(format!("{:<12} ", rel.state_name), state_style),
                Span::styled(format!("({:<7}) ", rel.relation_type), theme().date),
                Span::styled(rel.title.to_string(), theme().issue_title),
            ]));
        }
        lines.push(Line::from(""));
    }

    lines
}
