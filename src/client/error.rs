use compact_str::CompactString;
use thiserror::Error;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ClientError {
    #[error("HTTP request failed: {0}")]
    Http(CompactString),

    #[error("GraphQL errors returned: {0}")]
    GraphQlPartialError(CompactString),

    #[error("Rate limited by Linear API (reset at {reset_at:?})")]
    RateLimited { reset_at: Option<i64> },

    #[error("Query complexity exceeded Linear API limit: {0}")]
    ComplexityExceeded(CompactString),

    #[error("Authentication failed: invalid or missing Linear API key")]
    Authentication,

    #[error("Failed to parse Linear API response: {0}")]
    JsonParse(CompactString),

    #[error("Client is in offline mode")]
    OfflineMode,
}

impl From<reqwest::Error> for ClientError {
    fn from(value: reqwest::Error) -> Self {
        Self::Http(value.to_string().into())
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(value: serde_json::Error) -> Self {
        Self::JsonParse(value.to_string().into())
    }
}
