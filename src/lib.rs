//! KoiLang Rust Wrapper
//!
//! This crate provides a high-level Rust wrapper around the `koicore` library
//! for executing and generating KoiLang scripts. It provides equivalent functionality
//! to the Python `koilang_py` package with Rust's type safety and performance.
//!
//! # Features
//!
//! - **Runtime**: Execute KoiLang scripts with environment stack management
//! - **CommandHandler**: Trait-based command handling with static dispatch
//! - **Macros**: Procedural macros for automatic CommandHandler implementation
//! - **Middleware**: Chain of responsibility pattern for command interception
//! - **Caching**: Command caching for advanced control flow (jumps, labels)
//! - **Context**: Thread-local context for accessing current runtime/command
//! - **Writer**: Programmatic KoiLang generation
//!
//! # Examples
//!
//! ## Using Macros for Command Registration
//!
//! ```rust,ignore
//! use koilang::{Runtime, command, command_handler, Result};
//!
//! struct MyEnv {
//!     counter: i32,
//! }
//!
//! #[command_handler]
//! impl MyEnv {
//!     #[command]
//!     fn greet(&mut self, name: String) {
//!         println!("Hello, {}!", name);
//!     }
//!
//!     #[command(name = "@start")]
//!     fn on_start(&mut self) {
//!         println!("Environment started");
//!     }
//!
//!     #[command(name = "@end")]
//!     fn on_end(&mut self) {
//!         println!("Environment ended");
//!     }
//! }
//!
//! let mut runtime = Runtime::new();
//! runtime.env_enter(Box::new(MyEnv { counter: 0 }));
//! runtime.execute_str(r#"#greet "Alice""#).unwrap();
//! ```
//!
//! ## Manual CommandHandler Implementation
//!
//! ```rust
//! use koilang::{Runtime, CommandHandler, Result};
//! use koicore::command::Value;
//! use std::collections::HashMap;
//!
//! struct MyEnv;
//!
//! impl CommandHandler for MyEnv {
//!     fn handle_command(
//!         &mut self,
//!         name: &str,
//!         args: &[Value],
//!         _kwargs: &HashMap<String, Value>,
//!         _runtime: &mut Runtime,
//!     ) -> Result<()> {
//!         match name {
//!             "greet" => {
//!                 let name = args.get(0)
//!                     .map(|v| match v {
//!                         Value::String(s) => s.as_str(),
//!                         _ => "World",
//!                     })
//!                     .unwrap_or("World");
//!                 println!("Hello, {}!", name);
//!                 Ok(())
//!             }
//!             _ => Err(koilang::KoiError::command_not_found(name)),
//!         }
//!     }
//! }
//!
//! let mut runtime = Runtime::new();
//! runtime.env_enter(Box::new(MyEnv));
//! runtime.execute_str(r#"#greet "Alice""#).unwrap();
//! ```
//!
//! ## Programmatic Command Execution
//!
//! ```rust,ignore
//! use koilang::Runtime;
//!
//! let mut runtime = Runtime::new();
//! runtime.env_enter(Box::new(MyEnv));
//!
//! // Direct execution
//! runtime.execute_command("greet", &["Alice".into()], &HashMap::new()).unwrap();
//!
//! // Builder pattern
//! runtime.cmd("move")
//!     .arg("left")
//!     .kwarg("speed", 5)
//!     .execute()
//!     .unwrap();
//! ```
//!
//! ## Programmatic Generation
//!
//! ```rust
//! use koilang::Writer;
//! use std::collections::HashMap;
//!
//! let mut buffer = Vec::new();
//! let mut writer = Writer::new(&mut buffer, None).unwrap();
//!
//! writer.command("character", &["Alice".into(), "Hello!".into()], &HashMap::new()).unwrap();
//! writer.text("This is narrative text.").unwrap();
//! drop(writer);
//!
//! println!("{}", String::from_utf8(buffer).unwrap());
//! ```

// Module declarations
mod error;
mod handler;
mod runtime;
mod writer;

// Public exports
pub use error::{KoiError, Result};
pub use handler::CommandHandler;
pub use runtime::{Runtime, MiddlewareFn, CommandBuilder};
pub use writer::{Writer, OptionsProxy};

// Re-export macros
pub use koilang_macros::{command, command_handler};

// Re-export koicore types for convenience
pub use koicore::command::{Command, Parameter, Value};
pub use koicore::parser::{Parser, ParserConfig, TextInputSource};
pub use koicore::writer::{WriterConfig, FormatterOptions};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    struct TestEnv {
        commands: Vec<String>,
    }

    impl CommandHandler for TestEnv {
        fn handle_command(
            &mut self,
            name: &str,
            _args: &[Value],
            _kwargs: &HashMap<String, Value>,
            _runtime: &mut Runtime,
        ) -> Result<()> {
            self.commands.push(name.to_string());
            Ok(())
        }
    }

    #[test]
    fn test_integration() {
        let mut runtime = Runtime::new();
        let env = Box::new(TestEnv { commands: vec![] });
        runtime.env_enter(env);

        // Test execution
        runtime.execute_str(r#"#test_command"#).unwrap();

        // Test execute_command
        runtime.execute_command("another_command", &[], &HashMap::new()).unwrap();
    }

    // Test macro-generated CommandHandler
    struct MacroTestEnv {
        commands: Arc<Mutex<Vec<String>>>,
        last_name: Arc<Mutex<String>>,
    }

    impl MacroTestEnv {
        fn new() -> (Self, Arc<Mutex<Vec<String>>>, Arc<Mutex<String>>) {
            let commands = Arc::new(Mutex::new(Vec::new()));
            let last_name = Arc::new(Mutex::new(String::new()));
            (Self { 
                commands: commands.clone(), 
                last_name: last_name.clone() 
            }, commands, last_name)
        }
    }

    #[command_handler]
    impl MacroTestEnv {
        #[command]
        fn greet(&mut self, name: String) {
            self.commands.lock().unwrap().push(format!("greet: {}", name));
            *self.last_name.lock().unwrap() = name;
        }

        #[command(name = "say_hello")]
        fn hello(&mut self) {
            self.commands.lock().unwrap().push("hello".to_string());
        }

        #[command(name = "@start")]
        fn on_start(&mut self) {
            self.commands.lock().unwrap().push("@start".to_string());
        }

        #[command(name = "@end")]
        fn on_end(&mut self) {
            self.commands.lock().unwrap().push("@end".to_string());
        }

        #[command(name = "@text")]
        fn on_text(&mut self, content: String) {
            self.commands.lock().unwrap().push(format!("text: {}", content));
        }

        #[command(name = "@annotation")]
        fn on_annotation(&mut self, ann: String) {
            self.commands.lock().unwrap().push(format!("annotation: {}", ann));
        }
    }

    #[test]
    fn test_macro_basic_command() {
        let mut runtime = Runtime::new();
        let (env, commands, last_name) = MacroTestEnv::new();
        runtime.env_enter(Box::new(env));

        runtime.execute_command("greet", &["Alice".into()], &HashMap::new()).unwrap();
        
        let cmds = commands.lock().unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "greet: Alice");
        assert_eq!(*last_name.lock().unwrap(), "Alice");
    }

    #[test]
    fn test_macro_custom_name() {
        let mut runtime = Runtime::new();
        let (env, commands, _) = MacroTestEnv::new();
        runtime.env_enter(Box::new(env));

        runtime.execute_command("say_hello", &[], &HashMap::new()).unwrap();
        
        let cmds = commands.lock().unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "hello");
    }

    #[test]
    fn test_macro_lifecycle_hooks() {
        let mut runtime = Runtime::new();
        let (env, commands, _) = MacroTestEnv::new();
        runtime.env_enter(Box::new(env));

        // Execute a script to trigger lifecycle hooks
        runtime.execute_str(r#"#greet "Test""#).unwrap();

        let cmds = commands.lock().unwrap();
        assert!(cmds.contains(&"@start".to_string()));
        assert!(cmds.contains(&"@end".to_string()));
    }

    #[test]
    fn test_macro_text_command() {
        let mut runtime = Runtime::new();
        let (env, commands, _) = MacroTestEnv::new();
        runtime.env_enter(Box::new(env));

        runtime.execute_command("@text", &["Hello World".into()], &HashMap::new()).unwrap();
        
        let cmds = commands.lock().unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "text: Hello World");
    }

    #[test]
    fn test_macro_annotation_command() {
        let mut runtime = Runtime::new();
        let (env, commands, _) = MacroTestEnv::new();
        runtime.env_enter(Box::new(env));

        runtime.execute_command("@annotation", &["note".into()], &HashMap::new()).unwrap();
        
        let cmds = commands.lock().unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "annotation: note");
    }

    // Test Runtime injection in handlers
    struct RuntimeInjectionTestEnv {
        commands: Arc<Mutex<Vec<String>>>,
        runtime_accessed: Arc<Mutex<bool>>,
    }

    impl RuntimeInjectionTestEnv {
        fn new() -> (Self, Arc<Mutex<Vec<String>>>, Arc<Mutex<bool>>) {
            let commands = Arc::new(Mutex::new(Vec::new()));
            let runtime_accessed = Arc::new(Mutex::new(false));
            (Self { 
                commands: commands.clone(), 
                runtime_accessed: runtime_accessed.clone(),
            }, commands, runtime_accessed)
        }
    }

    #[command_handler]
    impl RuntimeInjectionTestEnv {
        // Handler with only Runtime parameter
        #[command(name = "with_runtime")]
        fn with_runtime(&mut self, runtime: &mut Runtime) {
            // Access runtime to verify injection works
            let _ = runtime.is_cache_enabled();
            *self.runtime_accessed.lock().unwrap() = true;
            self.commands.lock().unwrap().push("with_runtime".to_string());
        }

        // Handler with Runtime and arguments
        #[command(name = "runtime_and_args")]
        fn runtime_and_args(&mut self, runtime: &mut Runtime, name: String, count: i32) {
            let _ = runtime.is_cache_enabled();
            *self.runtime_accessed.lock().unwrap() = true;
            self.commands.lock().unwrap().push(format!("runtime_and_args: {} {}", name, count));
        }

        // Handler without Runtime (backward compatibility)
        #[command(name = "without_runtime")]
        fn without_runtime(&mut self, name: String) {
            self.commands.lock().unwrap().push(format!("without_runtime: {}", name));
        }

        // Handler with Runtime at different position (first after self)
        #[command(name = "runtime_first")]
        fn runtime_first(&mut self, runtime: &mut Runtime, arg1: String) {
            let _ = runtime.is_cache_enabled();
            *self.runtime_accessed.lock().unwrap() = true;
            self.commands.lock().unwrap().push(format!("runtime_first: {}", arg1));
        }
    }

    #[test]
    fn test_runtime_injection_only_runtime() {
        let mut runtime = Runtime::new();
        let (env, commands, accessed) = RuntimeInjectionTestEnv::new();
        runtime.env_enter(Box::new(env));

        runtime.execute_command("with_runtime", &[], &HashMap::new()).unwrap();
        
        let cmds = commands.lock().unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "with_runtime");
        assert!(*accessed.lock().unwrap());
    }

    #[test]
    fn test_runtime_injection_with_args() {
        let mut runtime = Runtime::new();
        let (env, commands, accessed) = RuntimeInjectionTestEnv::new();
        runtime.env_enter(Box::new(env));

        runtime.execute_command("runtime_and_args", &["Alice".into(), 42i64.into()], &HashMap::new()).unwrap();
        
        let cmds = commands.lock().unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "runtime_and_args: Alice 42");
        assert!(*accessed.lock().unwrap());
    }

    #[test]
    fn test_runtime_injection_backward_compat() {
        let mut runtime = Runtime::new();
        let (env, commands, _) = RuntimeInjectionTestEnv::new();
        runtime.env_enter(Box::new(env));

        runtime.execute_command("without_runtime", &["Bob".into()], &HashMap::new()).unwrap();
        
        let cmds = commands.lock().unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "without_runtime: Bob");
    }

    #[test]
    fn test_runtime_injection_first_position() {
        let mut runtime = Runtime::new();
        let (env, commands, accessed) = RuntimeInjectionTestEnv::new();
        runtime.env_enter(Box::new(env));

        runtime.execute_command("runtime_first", &["Test".into()], &HashMap::new()).unwrap();
        
        let cmds = commands.lock().unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "runtime_first: Test");
        assert!(*accessed.lock().unwrap());
    }
}
