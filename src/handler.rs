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
//! use koilang::{CommandHandler, Runtime, Result};
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
//!     ) -> Result<()> {
//!         match name {
//!             "increment" => {
//!                 self.counter += 1;
//!                 println!("Counter: {}", self.counter);
//!                 Ok(())
//!             }
//!             "@start" => {
//!                 println!("Environment started");
//!                 Ok(())
//!             }
//!             "@end" => {
//!                 println!("Environment ended");
//!                 Ok(())
//!             }
//!             "@text" => {
//!                 let content = args.get(0)
//!                     .map(|v| match v {
//!                         Value::String(s) => s.as_str(),
//!                         _ => "",
//!                     })
//!                     .unwrap_or("");
//!                 println!("Text: {}", content);
//!                 Ok(())
//!             }
//!             _ => Err(koilang::KoiError::command_not_found(name)),
//!         }
//!     }
//! }
//! ```

use crate::error::Result;
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
    /// Returns `Ok(())` if the command was handled successfully, or an error
    /// if something went wrong.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// fn handle_command(&mut self, name: &str, args: &[Value], kwargs: &HashMap<String, Value>, runtime: &mut Runtime) -> Result<()> {
    ///     match name {
    ///         "greet" => {
    ///             let name = args.get(0).and_then(|v| v.as_string()).unwrap_or("World");
    ///             println!("Hello, {}!", name);
    ///             Ok(())
    ///         }
    ///         _ => Err(KoiError::command_not_found(name))
    ///     }
    /// }
    /// ```
    fn handle_command(
        &mut self,
        name: &str,
        args: &[Value],
        kwargs: &HashMap<String, Value>,
        runtime: &mut Runtime,
    ) -> Result<()>;
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
) -> Result<()> {
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
        ) -> Result<()> {
            self.last_command = Some(name.to_string());
            if name == "fail" {
                Err(KoiError::runtime("test failure"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn test_handler_dispatch() {
        let mut handler = TestHandler { last_command: None };
        let mut runtime = Runtime::new();
        
        dispatch_to_handler(&mut handler, "test", &[], &HashMap::new(), &mut runtime).unwrap();
        assert_eq!(handler.last_command, Some("test".to_string()));
    }

    #[test]
    fn test_handler_error() {
        let mut handler = TestHandler { last_command: None };
        let mut runtime = Runtime::new();
        
        let result = dispatch_to_handler(&mut handler, "fail", &[], &HashMap::new(), &mut runtime);
        assert!(result.is_err());
    }
}
