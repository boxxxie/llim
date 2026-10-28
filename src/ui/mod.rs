use chrono::{DateTime, Local, Utc};
use compact_str::{CompactString, format_compact};

pub mod fx;
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
