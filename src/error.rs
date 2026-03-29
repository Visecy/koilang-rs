//! Error types for KoiLang runtime operations.

use std::io;
use thiserror::Error;

/// Main error type for KoiLang runtime operations.
#[derive(Error, Debug)]
pub enum KoiError {
    /// Runtime error with context.
    #[error("Runtime error: {message}")]
    Runtime {
        /// Error message.
        message: String,
    },

    /// Command not found error.
    #[error("Command '{name}' not found")]
    CommandNotFound {
        /// Command name that was not found.
        name: String,
    },

    /// Jump request for control flow.
    /// This is not a real error but a control flow mechanism.
    #[error("Jump to position {position}")]
    JumpRequest {
        /// Target position to jump to.
        position: usize,
    },

    /// Parse error from koicore.
    #[error("Parse error: {0}")]
    Parse(#[from] koicore::parser::ParseError),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
}

impl KoiError {
    /// Create a new runtime error.
    pub fn runtime(message: impl Into<String>) -> Self {
        Self::Runtime {
            message: message.into(),
        }
    }

    /// Create a new command not found error.
    pub fn command_not_found(name: impl Into<String>) -> Self {
        Self::CommandNotFound {
            name: name.into(),
        }
    }

    /// Create a new jump request.
    pub fn jump_request(position: usize) -> Self {
        Self::JumpRequest { position }
    }

    /// Check if this error is a jump request.
    pub fn is_jump_request(&self) -> bool {
        matches!(self, Self::JumpRequest { .. })
    }

    /// Get the jump position if this is a jump request.
    pub fn jump_position(&self) -> Option<usize> {
        match self {
            Self::JumpRequest { position } => Some(*position),
            _ => None,
        }
    }

}

/// Result type alias for KoiLang operations.
pub type Result<T> = std::result::Result<T, KoiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_error() {
        let err = KoiError::runtime("test error");
        assert!(matches!(err, KoiError::Runtime { .. }));
    }

    #[test]
    fn test_command_not_found() {
        let err = KoiError::command_not_found("test_cmd");
        assert!(
            matches!(err, KoiError::CommandNotFound { name } if name == "test_cmd")
        );
    }

    #[test]
    fn test_jump_request() {
        let err = KoiError::jump_request(42);
        assert!(err.is_jump_request());
        assert_eq!(err.jump_position(), Some(42));
    }
}
