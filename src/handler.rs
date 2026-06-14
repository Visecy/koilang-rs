//! Command handler trait for KoiLang environments.
//!
//! This module defines the [`CommandHandler`] trait that all environments must implement
//! to handle KoiLang commands. The trait uses static dispatch for performance.
//!
//! # Examples
//!
//! ## Manual Implementation
//!
//! ```rust
//! use koilang::{CommandHandler, Runtime, DispatchResult, KoiError};
//! use koicore::command::Value;
//! use std::collections::HashMap;
//!
//! struct MyEnvironment {
//!     counter: i32,
//! }
//!
//! impl CommandHandler for MyEnvironment {
//!     fn handle_command(
//!         &mut self,
//!         name: &str,
//!         args: &[Value],
//!         _kwargs: &HashMap<String, Value>,
//!         _runtime: &mut Runtime,
//!     ) -> DispatchResult {
//!         match name {
//!             "increment" => {
//!                 self.counter += 1;
//!                 println!("Counter: {}", self.counter);
//!                 DispatchResult::Continue
//!             }
//!             "@start" => {
//!                 println!("Environment started");
//!                 DispatchResult::Continue
//!             }
//!             "@end" => {
//!                 println!("Environment ended");
//!                 DispatchResult::Continue
//!             }
//!             "@text" => {
//!                 let content = args.get(0)
//!                     .map(|v| match v {
//!                         Value::String(s) => s.as_str(),
//!                         _ => "",
//!                     })
//!                     .unwrap_or("");
//!                 println!("Text: {}", content);
//!                 DispatchResult::Continue
//!             }
//!             _ => DispatchResult::Error(koilang::KoiError::command_not_found(name)),
//!         }
//!     }
//! }
//! ```

use crate::error::DispatchResult;
use crate::runtime::Runtime;
use koicore::command::Value;
use std::collections::HashMap;

/// Trait for types that can handle KoiLang commands.
///
/// This trait is the core abstraction for environments that can process
/// KoiLang commands. All commands (regular, lifecycle hooks, text content,
/// annotations) are handled uniformly through the [`handle_command`] method.
///
/// # Command Names
///
/// The `name` parameter can be:
/// - Regular commands: `"command_name"` (from `#command_name` in KoiLang)
/// - Lifecycle hooks: `"@start"`, `"@end"`
/// - Text content: `"@text"`
/// - Annotations: `"@annotation"`
///
/// # Type Safety
///
/// This trait uses static dispatch via vtable, avoiding runtime method lookup.
/// For automatic implementation, use the `#[command_handlers]` macro (future feature).
///
/// # Thread Safety
///
/// Implementations must be `Send` as they may be moved between threads.
pub trait CommandHandler: Send + 'static {
    /// Handle a command by name with given parameters.
    ///
    /// # Arguments
    ///
    /// * `name` - The command name (e.g., "hello", "@start", "@text")
    /// * `args` - Positional arguments as [`Value`] slices
    /// * `kwargs` - Named arguments as a map from String to [`Value`]
    /// * `runtime` - Mutable reference to the current [`Runtime`]
    ///
    /// # Returns
    ///
    /// Returns `Continue(())` if the command was handled normally,
    /// or a `Break` with the appropriate control flow signal (jump, probe, or error).
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// fn handle_command(&mut self, name: &str, args: &[Value], kwargs: &HashMap<String, Value>, runtime: &mut Runtime) -> DispatchResult {
    ///     match name {
    ///         "greet" => {
    ///             let name = args.get(0).and_then(|v| v.as_string()).unwrap_or("World");
    ///             println!("Hello, {}!", name);
    ///             DispatchResult::Continue
    ///         }
    ///         _ => DispatchResult::Error(KoiError::command_not_found(name))
    ///     }
    /// }
    /// ```
    fn handle_command(
        &mut self,
        name: &str,
        args: &[Value],
        kwargs: &HashMap<String, Value>,
        runtime: &mut Runtime,
    ) -> DispatchResult;
}

/// Helper function to convert arguments to a command handler call.
///
/// This is used internally by the runtime to dispatch commands.
#[allow(dead_code)]
pub(crate) fn dispatch_to_handler(
    handler: &mut dyn CommandHandler,
    name: &str,
    args: &[Value],
    kwargs: &HashMap<String, Value>,
    runtime: &mut Runtime,
) -> DispatchResult {
    handler.handle_command(name, args, kwargs, runtime)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::KoiError;

    struct TestHandler {
        last_command: Option<String>,
    }

    impl CommandHandler for TestHandler {
        fn handle_command(
            &mut self,
            name: &str,
            _args: &[Value],
            _kwargs: &HashMap<String, Value>,
            _runtime: &mut Runtime,
        ) -> DispatchResult {
            self.last_command = Some(name.to_string());
            if name == "fail" {
                DispatchResult::Error(KoiError::runtime("test failure"))
            } else {
                DispatchResult::Continue
            }
        }
    }

    #[test]
    fn test_handler_dispatch() {
        let mut handler = TestHandler { last_command: None };
        let mut runtime = Runtime::new();

        let result = dispatch_to_handler(&mut handler, "test", &[], &HashMap::new(), &mut runtime);
        assert!(matches!(result, DispatchResult::Continue));
        assert_eq!(handler.last_command, Some("test".to_string()));
    }

    #[test]
    fn test_handler_error() {
        let mut handler = TestHandler { last_command: None };
        let mut runtime = Runtime::new();

        let result = dispatch_to_handler(&mut handler, "fail", &[], &HashMap::new(), &mut runtime);
        assert!(matches!(result, DispatchResult::Error(_)));
    }
}
