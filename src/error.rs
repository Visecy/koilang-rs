//! Error types for KoiLang runtime operations.

use std::io;

use koicore::command::Command;
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

    /// Parse error from koicore.
    #[error("Parse error: {0}")]
    Parse(#[from] koicore::parser::ParseError),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
}

/// Result of command dispatch.
///
/// Represents the possible outcomes from dispatching a command:
/// normal continuation, a jump, a cache probe request, or an error.
pub enum DispatchResult<E = KoiError> {
    /// Continue normal execution, advance to next command.
    Continue,
    /// Jump to a specific position in the command cache.
    Jump(usize),
    /// Need more commands in cache before proceeding.
    ProbeNeeded {
        strategy: Box<dyn FnMut(&Command, usize) -> bool>,
        offset: i32,
    },
    /// An error occurred.
    Error(E),
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
}
