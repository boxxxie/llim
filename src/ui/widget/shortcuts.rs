use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::gruvbox::Gruvbox;

#[derive(Debug, Clone)]
pub struct Shortcut {
    pub key: &'static str,
    pub action: &'static str,
}

impl Shortcut {
    pub const fn new(key: &'static str, action: &'static str) -> Self {
        Self { key, action }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Shortcuts {
    pub items: Vec<Shortcut>,
}

impl Shortcuts {
    pub fn new(items: Vec<Shortcut>) -> Self {
        Self { items }
    }

    pub fn as_line(&self) -> Line<'static> {
        let key_style = Style::default()
            .fg(Gruvbox::OrangeBright.into())
            .add_modifier(Modifier::BOLD);
        let desc_style = Style::default().fg(Gruvbox::Light3.into());
        let sep_style = Style::default().fg(Gruvbox::Dark4.into());

        let mut spans: Vec<Span<'static>> = Vec::with_capacity(self.items.len() * 4);
        for (idx, item) in self.items.iter().enumerate() {
            if idx > 0 {
                spans.push(Span::styled(" │ ", sep_style));
            }
            spans.extend(spans_from_mnemonic(item.key, item.action, key_style, desc_style));
        }
        Line::from(spans)
    }
}

pub fn spans_from_mnemonic(
    key: &'static str,
    action: &'static str,
    key_style: Style,
    desc_style: Style,
) -> Vec<Span<'static>> {
    // Only attempt single-character mnemonic highlighting if `key` is a single ASCII alphanumeric char
    let mut chars = key.chars();
    if let (Some(kch), None) = (chars.next(), chars.next()) {
        if kch.is_ascii_alphanumeric() {
            if let Some((byte_idx, matched_ch)) = action
                .char_indices()
                .find(|(_, c)| c.eq_ignore_ascii_case(&kch))
            {
                let char_len = matched_ch.len_utf8();
                let before = &action[..byte_idx];
                let mid = &action[byte_idx..byte_idx + char_len];
                let after = &action[byte_idx + char_len..];
                let mut out = Vec::with_capacity(3);
                if !before.is_empty() {
                    out.push(Span::styled(before.to_string(), desc_style));
                }
                out.push(Span::styled(
                    mid.to_string(),
                    key_style.add_modifier(Modifier::UNDERLINED),
                ));
                if !after.is_empty() {
                    out.push(Span::styled(after.to_string(), desc_style));
                }
                return out;
            }
        }
    }

    vec![
        Span::styled(format!("[{key}] "), key_style),
        Span::styled(action.to_string(), desc_style),
    ]
}
