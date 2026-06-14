use thiserror::Error;

#[derive(Error, Debug)]
pub enum BotError {
    #[error("API error: {0}")]
    ApiError(String),

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Config error: {0}")]
    ConfigError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Insufficient data: {0}")]
    InsufficientData(String),

    #[error("Position not found: {0}")]
    PositionNotFound(String),
}

impl From<reqwest::Error> for BotError {
    fn from(err: reqwest::Error) -> Self {
        BotError::ApiError(err.to_string())
    }
}
