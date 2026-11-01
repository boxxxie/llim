use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use crate::{gruvbox::Gruvbox, theme::theme};

/// Render a Markdown document into styled Ratatui [`Line`]s using the Gruvbox palette.
pub fn render_markdown(markdown: &str) -> Vec<Line<'static>> {
    render_markdown_with_options(markdown, "", theme().issue_title)
}

/// Render a Markdown document with a fixed left indentation prefix on every line.
pub fn render_markdown_indented(markdown: &str, indent: &str) -> Vec<Line<'static>> {
    render_markdown_with_options(markdown, indent, theme().issue_title)
}

/// Render a Markdown document with a custom indentation prefix and base text style.
pub fn render_markdown_with_options(
    markdown: &str,
    indent: &str,
    base_style: Style,
) -> Vec<Line<'static>> {
    if markdown.trim().is_empty() {
        return Vec::new();
    }

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(markdown, options);
    let mut renderer = MarkdownRenderer::new(indent.to_string(), base_style);
    renderer.process(parser);
    renderer.finish()
}

#[derive(Debug, Clone)]
struct ListContext {
    next_index: Option<u64>,
    pending_marker: Option<(String, Style)>,
    continuation_indent: usize,
}

#[derive(Debug, Default)]
struct TableState {
    alignments: Vec<Alignment>,
    in_head: bool,
    header: Vec<Vec<Span<'static>>>,
    rows: Vec<Vec<Vec<Span<'static>>>>,
    current_row: Vec<Vec<Span<'static>>>,
}

struct MarkdownRenderer {
    base_indent: String,
    base_style: Style,
    lines: Vec<Line<'static>>,
    current_spans: Vec<Span<'static>>,
    style_stack: Vec<Style>,
    blockquote_depth: usize,
    list_stack: Vec<ListContext>,
    in_code_block: bool,
    code_block_lang: String,
    code_block_buffer: String,
    link_stack: Vec<(String, String)>,
    image_stack: Vec<(String, String)>,
    table_state: Option<TableState>,
    in_heading: Option<HeadingLevel>,
    needs_block_sep: bool,
}

impl MarkdownRenderer {
    fn new(base_indent: String, base_style: Style) -> Self {
        Self {
            base_indent,
            base_style,
            lines: Vec::new(),
            current_spans: Vec::new(),
            style_stack: vec![base_style],
            blockquote_depth: 0,
            list_stack: Vec::new(),
            in_code_block: false,
            code_block_lang: String::new(),
            code_block_buffer: String::new(),
            link_stack: Vec::new(),
            image_stack: Vec::new(),
            table_state: None,
            in_heading: None,
            needs_block_sep: false,
        }
    }

    fn current_style(&self) -> Style {
        self.style_stack.last().copied().unwrap_or(self.base_style)
    }

    fn push_style(&mut self, style: Style) {
        let merged = self.current_style().patch(style);
        self.style_stack.push(merged);
    }

    fn pop_style(&mut self) {
        if self.style_stack.len() > 1 {
            self.style_stack.pop();
        }
    }

    fn emit_block_separator_if_needed(&mut self) {
        if !self.needs_block_sep {
            return;
        }
        self.needs_block_sep = false;
        if self.lines.is_empty() {
            return;
        }
        let mut prefix_spans = Vec::new();
        if !self.base_indent.is_empty() {
            prefix_spans.push(Span::raw(self.base_indent.clone()));
        }
        if self.blockquote_depth > 0 {
            prefix_spans.push(Span::styled(
                "│ ".repeat(self.blockquote_depth),
                Style::default().fg(Gruvbox::PurpleBright.into()),
            ));
        }
        self.lines.push(Line::from(prefix_spans));
    }

    fn build_line_prefix(&mut self) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        if !self.base_indent.is_empty() {
            spans.push(Span::raw(self.base_indent.clone()));
        }

        if self.blockquote_depth > 0 {
            spans.push(Span::styled(
                "│ ".repeat(self.blockquote_depth),
                Style::default().fg(Gruvbox::PurpleBright.into()),
            ));
        }

        if let Some(level) = self.in_heading {
            let (marker, style) = heading_marker_and_style(level);
            spans.push(Span::styled(marker, style));
            return spans;
        }

        let depth = self.list_stack.len();
        if depth > 0 {
            if depth > 1 {
                spans.push(Span::raw("  ".repeat(depth - 1)));
            }
            if let Some(last) = self.list_stack.last_mut() {
                if let Some((marker, style)) = last.pending_marker.take() {
                    last.continuation_indent = marker.width();
                    spans.push(Span::styled(marker, style));
                } else if last.continuation_indent > 0 {
                    spans.push(Span::raw(" ".repeat(last.continuation_indent)));
                }
            }
        }

        spans
    }

    fn flush_line(&mut self) {
        if self.current_spans.is_empty() {
            return;
        }
        self.emit_block_separator_if_needed();
        let mut line_spans = self.build_line_prefix();
        line_spans.append(&mut self.current_spans);
        self.lines.push(Line::from(line_spans));
    }

    fn flush_empty_line(&mut self) {
        self.emit_block_separator_if_needed();
        let line_spans = self.build_line_prefix();
        self.lines.push(Line::from(line_spans));
    }

    fn process<'a>(&mut self, parser: Parser<'a>) {
        for event in parser {
            match event {
                Event::Start(tag) => self.start_tag(tag),
                Event::End(tag) => self.end_tag(tag),
                Event::Text(text) => self.text(&text),
                Event::Code(code) => self.inline_code(&code),
                Event::SoftBreak | Event::HardBreak => {
                    if self.in_code_block {
                        self.code_block_buffer.push('\n');
                    } else if self.table_state.is_some() {
                        self.current_spans.push(Span::raw(" "));
                    } else {
                        self.flush_line();
                    }
                }
                Event::Rule => {
                    self.flush_line();
                    self.emit_block_separator_if_needed();
                    let mut spans = self.build_line_prefix();
                    spans.push(Span::styled(
                        "────────────────────────────────────────",
                        Style::default().fg(Gruvbox::Dark3.into()),
                    ));
                    self.lines.push(Line::from(spans));
                    self.needs_block_sep = true;
                }
                Event::TaskListMarker(checked) => {
                    if let Some(last) = self.list_stack.last_mut() {
                        if checked {
                            last.pending_marker = Some((
                                "[✓] ".to_string(),
                                Style::default()
                                    .fg(Gruvbox::GreenBright.into())
                                    .add_modifier(Modifier::BOLD),
                            ));
                        } else {
                            last.pending_marker = Some((
                                "[ ] ".to_string(),
                                Style::default().fg(Gruvbox::Gray244.into()),
                            ));
                        }
                    }
                }
                Event::Html(html) | Event::InlineHtml(html) => {
                    let trimmed = html.trim();
                    if trimmed.starts_with("<!--") && trimmed.ends_with("-->") {
                        continue;
                    }
                    if trimmed.eq_ignore_ascii_case("<br>")
                        || trimmed.eq_ignore_ascii_case("<br/>")
                        || trimmed.eq_ignore_ascii_case("<br />")
                    {
                        self.flush_line();
                    } else if !trimmed.is_empty() {
                        for (idx, line) in html.lines().enumerate() {
                            if idx > 0 {
                                self.flush_line();
                            }
                            self.current_spans.push(Span::styled(
                                line.to_string(),
                                Style::default().fg(Gruvbox::Gray244.into()),
                            ));
                        }
                    }
                }
                Event::FootnoteReference(name) => {
                    self.current_spans.push(Span::styled(
                        format!("[^{name}]"),
                        Style::default().fg(Gruvbox::PurpleBright.into()),
                    ));
                }
                Event::InlineMath(math) | Event::DisplayMath(math) => {
                    self.current_spans.push(Span::styled(
                        math.to_string(),
                        Style::default()
                            .fg(Gruvbox::AquaBright.into())
                            .add_modifier(Modifier::ITALIC),
                    ));
                }
            }
        }
    }

    fn start_tag(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                self.flush_line();
                self.in_heading = Some(level);
                let (_, style) = heading_marker_and_style(level);
                self.push_style(style);
            }
            Tag::BlockQuote(_) => {
                self.flush_line();
                self.blockquote_depth += 1;
                self.push_style(
                    Style::default()
                        .fg(Gruvbox::Light3.into())
                        .add_modifier(Modifier::ITALIC),
                );
            }
            Tag::CodeBlock(kind) => {
                self.flush_line();
                self.in_code_block = true;
                self.code_block_buffer.clear();
                self.code_block_lang = match kind {
                    CodeBlockKind::Fenced(lang) => {
                        lang.split_whitespace().next().unwrap_or("").to_string()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
            }
            Tag::List(start_num) => {
                self.flush_line();
                self.list_stack.push(ListContext {
                    next_index: start_num,
                    pending_marker: None,
                    continuation_indent: 2,
                });
            }
            Tag::Item => {
                self.flush_line();
                // Tight lists shouldn't insert blank lines between consecutive items
                self.needs_block_sep = false;
                let depth = self.list_stack.len();
                if let Some(ctx) = self.list_stack.last_mut() {
                    let (marker, style) = match ctx.next_index.as_mut() {
                        Some(idx) => {
                            let m = format!("{idx}. ");
                            *idx += 1;
                            (
                                m,
                                Style::default()
                                    .fg(Gruvbox::YellowBright.into())
                                    .add_modifier(Modifier::BOLD),
                            )
                        }
                        None => {
                            let bullet = match depth {
                                1 => "• ",
                                2 => "◦ ",
                                _ => "▪ ",
                            };
                            (
                                bullet.to_string(),
                                Style::default()
                                    .fg(Gruvbox::OrangeBright.into())
                                    .add_modifier(Modifier::BOLD),
                            )
                        }
                    };
                    ctx.continuation_indent = marker.width();
                    ctx.pending_marker = Some((marker, style));
                }
            }
            Tag::Emphasis => {
                self.push_style(Style::default().add_modifier(Modifier::ITALIC));
            }
            Tag::Strong => {
                let mut strong_style = Style::default().add_modifier(Modifier::BOLD);
                if self.current_style().fg == self.base_style.fg {
                    strong_style = strong_style.fg(Gruvbox::Light0Hard.into());
                }
                self.push_style(strong_style);
            }
            Tag::Strikethrough => {
                self.push_style(
                    Style::default()
                        .fg(Gruvbox::Gray244.into())
                        .add_modifier(Modifier::CROSSED_OUT),
                );
            }
            Tag::Link { dest_url, .. } => {
                self.link_stack.push((dest_url.to_string(), String::new()));
                self.push_style(
                    Style::default()
                        .fg(Gruvbox::BlueBright.into())
                        .add_modifier(Modifier::UNDERLINED),
                );
            }
            Tag::Image { dest_url, .. } => {
                self.image_stack.push((dest_url.to_string(), String::new()));
            }
            Tag::Table(alignments) => {
                self.flush_line();
                self.table_state = Some(TableState {
                    alignments,
                    ..Default::default()
                });
            }
            Tag::TableHead => {
                if let Some(ref mut tbl) = self.table_state {
                    tbl.in_head = true;
                    tbl.current_row.clear();
                }
                self.push_style(
                    Style::default()
                        .fg(Gruvbox::YellowBright.into())
                        .add_modifier(Modifier::BOLD),
                );
            }
            Tag::TableRow => {
                if let Some(ref mut tbl) = self.table_state {
                    tbl.current_row.clear();
                }
            }
            Tag::TableCell => {
                self.current_spans.clear();
            }
            _ => {}
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.flush_line();
                if self.list_stack.is_empty() {
                    self.needs_block_sep = true;
                }
            }
            TagEnd::Heading(_) => {
                self.flush_line();
                self.in_heading = None;
                self.pop_style();
                self.needs_block_sep = true;
            }
            TagEnd::BlockQuote(_) => {
                self.flush_line();
                self.blockquote_depth = self.blockquote_depth.saturating_sub(1);
                self.pop_style();
                if self.blockquote_depth == 0 {
                    self.needs_block_sep = true;
                }
            }
            TagEnd::CodeBlock => {
                self.in_code_block = false;
                self.render_code_block();
                self.needs_block_sep = true;
            }
            TagEnd::List(_) => {
                self.flush_line();
                self.list_stack.pop();
                if self.list_stack.is_empty() {
                    self.needs_block_sep = true;
                }
            }
            TagEnd::Item => {
                self.flush_line();
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                self.pop_style();
            }
            TagEnd::Link => {
                self.pop_style();
                if let Some((url, label)) = self.link_stack.pop() {
                    let trimmed_url = url.trim();
                    let trimmed_label = label.trim();
                    if !trimmed_url.is_empty() && trimmed_label != trimmed_url {
                        self.current_spans.push(Span::styled(
                            format!(" ({trimmed_url})"),
                            Style::default().fg(Gruvbox::Gray244.into()),
                        ));
                    }
                }
            }
            TagEnd::Image => {
                if let Some((url, alt)) = self.image_stack.pop() {
                    let label = if alt.trim().is_empty() {
                        "image"
                    } else {
                        alt.trim()
                    };
                    let badge = if url.trim().is_empty() {
                        format!("[🖼 {label}]")
                    } else {
                        format!("[🖼 {label} ({})]", url.trim())
                    };
                    self.current_spans.push(Span::styled(
                        badge,
                        Style::default().fg(Gruvbox::PurpleBright.into()),
                    ));
                }
            }
            TagEnd::TableHead => {
                self.pop_style();
                if let Some(ref mut tbl) = self.table_state {
                    tbl.in_head = false;
                    tbl.header = std::mem::take(&mut tbl.current_row);
                }
            }
            TagEnd::TableRow => {
                if let Some(ref mut tbl) = self.table_state {
                    let row = std::mem::take(&mut tbl.current_row);
                    tbl.rows.push(row);
                }
            }
            TagEnd::TableCell => {
                let cell_spans = std::mem::take(&mut self.current_spans);
                if let Some(ref mut tbl) = self.table_state {
                    tbl.current_row.push(cell_spans);
                }
            }
            TagEnd::Table => {
                if let Some(tbl) = self.table_state.take() {
                    self.render_table(tbl);
                    self.needs_block_sep = true;
                }
            }
            _ => {}
        }
    }

    fn text(&mut self, text: &str) {
        if self.in_code_block {
            self.code_block_buffer.push_str(text);
            return;
        }
        if let Some((_, ref mut alt)) = self.image_stack.last_mut() {
            alt.push_str(text);
            return;
        }
        if let Some((_, ref mut label)) = self.link_stack.last_mut() {
            label.push_str(text);
        }

        let style = self.current_style();
        let parts: Vec<&str> = text.split('\n').collect();
        for (idx, part) in parts.into_iter().enumerate() {
            if idx > 0 {
                if self.table_state.is_some() {
                    self.current_spans.push(Span::raw(" "));
                } else {
                    self.flush_line();
                }
            }
            if !part.is_empty() {
                self.current_spans
                    .push(Span::styled(part.to_string(), style));
            }
        }
    }

    fn inline_code(&mut self, code: &str) {
        if let Some((_, ref mut alt)) = self.image_stack.last_mut() {
            alt.push_str(code);
            return;
        }
        if let Some((_, ref mut label)) = self.link_stack.last_mut() {
            label.push_str(code);
        }
        let code_style = Style::default()
            .fg(Gruvbox::AquaBright.into())
            .bg(Gruvbox::Dark1.into());
        self.current_spans
            .push(Span::styled(code.to_string(), code_style));
    }

    fn render_code_block(&mut self) {
        self.emit_block_separator_if_needed();

        let border_style = Style::default().fg(Gruvbox::Dark3.into());
        let lang_style = Style::default()
            .fg(Gruvbox::YellowBright.into())
            .add_modifier(Modifier::BOLD);
        let code_style = Style::default()
            .fg(Gruvbox::Light0Soft.into())
            .bg(Gruvbox::Dark0Hard.into());

        // Top border
        let mut top_spans = self.build_line_prefix();
        if self.code_block_lang.is_empty() {
            top_spans.push(Span::styled(
                "╭────────────────────────────────────",
                border_style,
            ));
        } else {
            top_spans.push(Span::styled("╭─ ", border_style));
            top_spans.push(Span::styled(self.code_block_lang.clone(), lang_style));
            let dash_len = 33usize.saturating_sub(self.code_block_lang.width());
            top_spans.push(Span::styled(
                format!(" {}", "─".repeat(dash_len)),
                border_style,
            ));
        }
        self.lines.push(Line::from(top_spans));

        let trimmed = self
            .code_block_buffer
            .strip_suffix('\n')
            .unwrap_or(&self.code_block_buffer)
            .to_string();

        if trimmed.is_empty() {
            let mut line_spans = self.build_line_prefix();
            line_spans.push(Span::styled("│ ", border_style));
            self.lines.push(Line::from(line_spans));
        } else {
            for raw_line in trimmed.split('\n') {
                let mut line_spans = self.build_line_prefix();
                line_spans.push(Span::styled("│ ", border_style));
                line_spans.push(Span::styled(raw_line.to_string(), code_style));
                self.lines.push(Line::from(line_spans));
            }
        }

        // Bottom border
        let mut bottom_spans = self.build_line_prefix();
        bottom_spans.push(Span::styled(
            "╰────────────────────────────────────",
            border_style,
        ));
        self.lines.push(Line::from(bottom_spans));

        self.code_block_buffer.clear();
        self.code_block_lang.clear();
    }

    fn render_table(&mut self, tbl: TableState) {
        let num_cols = tbl
            .header
            .len()
            .max(tbl.rows.iter().map(Vec::len).max().unwrap_or(0));
        if num_cols == 0 {
            return;
        }

        self.emit_block_separator_if_needed();

        let mut col_widths = vec![1usize; num_cols];
        for (c, cell) in tbl.header.iter().enumerate() {
            let w: usize = cell.iter().map(|s| s.content.width()).sum();
            col_widths[c] = col_widths[c].max(w);
        }
        for row in &tbl.rows {
            for (c, cell) in row.iter().enumerate() {
                let w: usize = cell.iter().map(|s| s.content.width()).sum();
                col_widths[c] = col_widths[c].max(w);
            }
        }

        let border_style = Style::default().fg(Gruvbox::Dark3.into());

        // Top border: ┌───┬───┐
        let top_border = build_table_rule(&col_widths, "┌", "┬", "┐");
        let mut top_spans = self.build_line_prefix();
        top_spans.push(Span::styled(top_border, border_style));
        self.lines.push(Line::from(top_spans));

        // Header row
        if !tbl.header.is_empty() {
            let mut row_spans = self.build_line_prefix();
            append_table_row(
                &mut row_spans,
                &tbl.header,
                &col_widths,
                &tbl.alignments,
                border_style,
            );
            self.lines.push(Line::from(row_spans));

            // Header separator: ├───┼───┤
            let mid_border = build_table_rule(&col_widths, "├", "┼", "┤");
            let mut mid_spans = self.build_line_prefix();
            mid_spans.push(Span::styled(mid_border, border_style));
            self.lines.push(Line::from(mid_spans));
        }

        // Body rows
        for row in &tbl.rows {
            let mut row_spans = self.build_line_prefix();
            append_table_row(
                &mut row_spans,
                row,
                &col_widths,
                &tbl.alignments,
                border_style,
            );
            self.lines.push(Line::from(row_spans));
        }

        // Bottom border: └───┴───┘
        let bot_border = build_table_rule(&col_widths, "└", "┴", "┘");
        let mut bot_spans = self.build_line_prefix();
        bot_spans.push(Span::styled(bot_border, border_style));
        self.lines.push(Line::from(bot_spans));
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.flush_line();
        if self.lines.is_empty() {
            self.flush_empty_line();
        }
        self.lines
    }
}

fn heading_marker_and_style(level: HeadingLevel) -> (&'static str, Style) {
    match level {
        HeadingLevel::H1 => (
            "# ",
            Style::default()
                .fg(Gruvbox::OrangeBright.into())
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        ),
        HeadingLevel::H2 => (
            "## ",
            Style::default()
                .fg(Gruvbox::YellowBright.into())
                .add_modifier(Modifier::BOLD),
        ),
        HeadingLevel::H3 => (
            "### ",
            Style::default()
                .fg(Gruvbox::AquaBright.into())
                .add_modifier(Modifier::BOLD),
        ),
        HeadingLevel::H4 => (
            "#### ",
            Style::default()
                .fg(Gruvbox::BlueBright.into())
                .add_modifier(Modifier::BOLD),
        ),
        HeadingLevel::H5 => (
            "##### ",
            Style::default()
                .fg(Gruvbox::PurpleBright.into())
                .add_modifier(Modifier::BOLD),
        ),
        HeadingLevel::H6 => (
            "###### ",
            Style::default()
                .fg(Gruvbox::Light3.into())
                .add_modifier(Modifier::BOLD),
        ),
    }
}

fn build_table_rule(col_widths: &[usize], left: &str, mid: &str, right: &str) -> String {
    let mut s = String::from(left);
    for (idx, &w) in col_widths.iter().enumerate() {
        if idx > 0 {
            s.push_str(mid);
        }
        s.push_str(&"─".repeat(w + 2));
    }
    s.push_str(right);
    s
}

fn append_table_row(
    out: &mut Vec<Span<'static>>,
    cells: &[Vec<Span<'static>>],
    col_widths: &[usize],
    alignments: &[Alignment],
    border_style: Style,
) {
    out.push(Span::styled("│", border_style));
    for (col_idx, &width) in col_widths.iter().enumerate() {
        let cell = cells.get(col_idx).map(Vec::as_slice).unwrap_or(&[]);
        let cell_width: usize = cell.iter().map(|s| s.content.width()).sum();
        let pad = width.saturating_sub(cell_width);
        let align = alignments
            .get(col_idx)
            .copied()
            .unwrap_or(Alignment::None);

        let (left_pad, right_pad) = match align {
            Alignment::Right => (pad, 0),
            Alignment::Center => (pad / 2, pad - pad / 2),
            Alignment::Left | Alignment::None => (0, pad),
        };

        out.push(Span::raw(format!(" {}", " ".repeat(left_pad))));
        out.extend(cell.iter().cloned());
        out.push(Span::raw(format!("{} ", " ".repeat(right_pad))));
        out.push(Span::styled("│", border_style));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines_to_plain_strings(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn renders_headings_and_inline_styles() {
        let md = "# Main Title\n\nSome **bold**, *italic*, `inline_code`, and ~~struck~~ text.";
        let lines = render_markdown(md);
        let plain = lines_to_plain_strings(&lines);
        assert_eq!(
            plain,
            vec![
                "# Main Title",
                "",
                "Some bold, italic, inline_code, and struck text."
            ]
        );
    }

    #[test]
    fn renders_task_lists_and_nested_lists() {
        let md = "- [x] Completed item\n- [ ] Pending item\n- Regular bullet\n  1. Nested first\n  2. Nested second";
        let lines = render_markdown_indented(md, "  ");
        let plain = lines_to_plain_strings(&lines);
        assert_eq!(
            plain,
            vec![
                "  [✓] Completed item",
                "  [ ] Pending item",
                "  • Regular bullet",
                "    1. Nested first",
                "    2. Nested second",
            ]
        );
    }

    #[test]
    fn renders_fenced_code_blocks_with_language() {
        let md = "```rust\nlet x = 42;\nprintln!(\"{x}\");\n```";
        let lines = render_markdown(md);
        let plain = lines_to_plain_strings(&lines);
        assert_eq!(plain.len(), 4);
        assert!(plain[0].starts_with("╭─ rust "));
        assert_eq!(plain[1], "│ let x = 42;");
        assert_eq!(plain[2], "│ println!(\"{x}\");");
        assert!(plain[3].starts_with("╰──"));
    }

    #[test]
    fn renders_links_and_blockquotes() {
        let md = "> Quoted [Linear Docs](https://linear.app/docs) and <https://linear.app>";
        let lines = render_markdown(md);
        let plain = lines_to_plain_strings(&lines);
        assert_eq!(
            plain,
            vec!["│ Quoted Linear Docs (https://linear.app/docs) and https://linear.app"]
        );
    }

    #[test]
    fn renders_gfm_tables() {
        let md = "| Key | State |\n| --- | --- |\n| CAA-1 | In Progress |";
        let lines = render_markdown(md);
        let plain = lines_to_plain_strings(&lines);
        assert_eq!(plain.len(), 5);
        assert_eq!(plain[1], "│ Key   │ State       │");
        assert_eq!(plain[3], "│ CAA-1 │ In Progress │");
    }
}
