use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Paragraph, StatefulWidget, Widget},
};

use crate::{
    domain::{HydrationState, RateLimitStatus, ViewKind},
    id::ProjectScope,
    stores::LinearStore,
    theme::theme,
    ui::{
        stateful_widgets::{ModalState, StatefulWidgets},
        widget::{
            activity_feed::ActivityFeedWidget, attention_table::AttentionTableWidget,
            details_inspector::DetailsInspectorWidget, input_field::FilterInputBar,
            issues_table::IssuesTableWidget, notification::NotificationBanner,
        },
    },
};

pub fn render_main_ui(
    frame: &mut Frame,
    widgets: &mut StatefulWidgets,
    store: &LinearStore,
    rate_limit: &RateLimitStatus,
    offline: bool,
) {
    let area = frame.area();
    let buf = frame.buffer_mut();

    Block::default().style(theme().background).render(area, buf);

    let show_filter_bar =
        widgets.normal_processor.filtering || !widgets.filter_scope.search_query.is_empty();

    let mut constraints = Vec::with_capacity(4);
    constraints.push(Constraint::Length(1)); // Top header bar
    if show_filter_bar {
        constraints.push(Constraint::Length(1)); // Filter bar
    }
    constraints.push(Constraint::Min(8)); // Split 50/50 body area
    constraints.push(Constraint::Length(1)); // Shortcuts bar

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut idx = 0;
    let header_area = chunks[idx];
    idx += 1;

    let filter_area = if show_filter_bar {
        let r = chunks[idx];
        idx += 1;
        Some(r)
    } else {
        None
    };

    let body_area = chunks[idx];
    idx += 1;
    let footer_area = chunks[idx];

    render_top_chrome(header_area, buf, widgets, store, rate_limit, offline);

    if let Some(f_area) = filter_area {
        FilterInputBar::from_editor(
            &widgets.normal_processor.filter_editor,
            widgets.normal_processor.filtering,
        )
        .render(f_area, buf);
    }

    if let Some(issue_page) = widgets.open_issue_page() {
        issue_page.borrow().render_in(body_area, buf);
    } else {
        let (top_area, inspector_area) = if widgets.show_inspector && body_area.height >= 10 {
            let split = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(body_area);
            (split[0], Some(split[1]))
        } else {
            (body_area, None)
        };

        let top_focused =
            !(widgets.focus_inspector && widgets.show_inspector) && widgets.modal_stack.is_empty();

        match widgets.active_view {
            ViewKind::Issues => {
                IssuesTableWidget::new(top_focused, &widgets.selected_issue_ids).render(
                    top_area,
                    buf,
                    &mut widgets.issues_view,
                );
            }
            ViewKind::Attention => {
                AttentionTableWidget::new(top_focused, &widgets.selected_issue_ids).render(
                    top_area,
                    buf,
                    &mut widgets.attention_view,
                );
            }
            ViewKind::Activity => {
                ActivityFeedWidget::new(top_focused)
                    .render(top_area, buf, &mut widgets.activity_feed);
            }
        }

        if let Some(insp_area) = inspector_area {
            let insp_focused =
                widgets.focus_inspector && widgets.show_inspector && widgets.modal_stack.is_empty();
            DetailsInspectorWidget::new(insp_focused).render(
                insp_area,
                buf,
                &mut widgets.inspector,
            );
        }
    }

    let shortcuts = widgets.active_shortcuts();
    Paragraph::new(shortcuts.as_line()).render(footer_area, buf);

    // Render modal stack on top (ProjectDetailsPopup, ConfigPopup, HelpPopup, write pickers)
    for modal in &widgets.modal_stack {
        match &modal.state {
            ModalState::IssueDetails(_) => {}
            ModalState::ProjectDetails(p) => p.borrow().render_in(area, buf),
            ModalState::ActivityDetails(p) => p.borrow().render_in(area, buf),
            ModalState::Config(p) => p.borrow().render_in(area, buf),
            ModalState::Help(p) => p.borrow().render_in(area, buf),
            ModalState::QuickPicker(p) => p.borrow().render_in(area, buf),
            ModalState::LabelPicker(p) => p.borrow().render_in(area, buf),
            ModalState::CommentCompose(p) => p.borrow().render_in(area, buf),
            ModalState::IssueForm(p) => p.borrow().render_in(area, buf),
        }
    }

    // Render transient notification banner on top
    if let Some(ref notice) = widgets.active_notice {
        NotificationBanner::new(notice).render(area, buf);
    }
}

fn render_top_chrome(
    area: Rect,
    buf: &mut ratatui::buffer::Buffer,
    widgets: &StatefulWidgets,
    store: &LinearStore,
    rate_limit: &RateLimitStatus,
    offline: bool,
) {
    let tab1_style = if widgets.active_view == ViewKind::Issues {
        theme().active_tab
    } else {
        theme().inactive_tab
    };
    let tab2_style = if widgets.active_view == ViewKind::Attention {
        theme().active_tab
    } else {
        theme().inactive_tab
    };
    let tab3_style = if widgets.active_view == ViewKind::Activity {
        theme().active_tab
    } else {
        theme().inactive_tab
    };

    let team_label = widgets
        .filter_scope
        .team_key
        .as_deref()
        .unwrap_or("All");

    let proj_pill_text = match &widgets.filter_scope.project_scope {
        ProjectScope::All => "Project: All".to_string(),
        ProjectScope::Unassigned => "Project: Unassigned".to_string(),
        ProjectScope::Project(pid) => {
            if let Some(proj) = store.projects.get(pid) {
                let pct = (proj.progress * 100.0).round() as u32;
                let filled = ((pct.min(100) as usize) * 8) / 100;
                let bar = format!(
                    "{}{}",
                    "█".repeat(filled),
                    "░".repeat(8usize.saturating_sub(filled))
                );
                format!("Project: {} [{}] {}%", proj.name, bar, pct)
            } else {
                "Project: Selected".to_string()
            }
        }
    };

    let quota_pct = rate_limit.quota_percent();
    let health_style = if offline {
        theme().health_warn
    } else if rate_limit.is_low_budget() {
        theme().health_err
    } else {
        theme().health_ok
    };

    let sync_label = if offline {
        "OFFLINE".to_string()
    } else {
        match store.hydration_state {
            HydrationState::ColdBootstrap => "Hydrating...".to_string(),
            HydrationState::CatchUpDelta => "Catching up...".to_string(),
            HydrationState::LiveSteadyState => format!("Live • Quota {quota_pct}%"),
        }
    };

    let quack_badge = if let Some(uri) = store.quack_uri() {
        format!(" • 🦆 {uri}")
    } else {
        String::new()
    };

    let mut spans = vec![
        Span::styled(
            format!(" 1: Issues ({}) ", widgets.issues_view.rows.len()),
            tab1_style,
        ),
        Span::raw(" "),
        Span::styled(
            format!(" 2: My Attention ({}) ", widgets.attention_view.rows.len()),
            tab2_style,
        ),
        Span::raw(" "),
        Span::styled(
            format!(" 3: Activity ({}) ", widgets.activity_feed.raw_events.len()),
            tab3_style,
        ),
        Span::raw("  "),
        Span::styled(format!("[Team: {team_label}]"), theme().team_badge),
        Span::raw(" "),
        Span::styled(format!("[{proj_pill_text}]"), theme().scope_pill),
    ];
    if let Some(issue_page) = widgets.open_issue_page() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!(" [Issue: {}] ", issue_page.borrow().issue().identifier),
            theme().active_tab,
        ));
    } else if !widgets.selected_issue_ids.is_empty() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!(" [✓ Selected: {}] ", widgets.selected_issue_ids.len()),
            theme().active_tab,
        ));
    }
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("({sync_label}{quack_badge})"),
        health_style,
    ));

    let line = Line::from(spans);

    Paragraph::new(line).render(area, buf);
}
