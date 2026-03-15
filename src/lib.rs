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
//! use koilang_rs::{Runtime, command, command_handler, Result};
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
//! runtime.execute(r#"#greet "Alice""#).unwrap();
//! ```
//!
//! ## Manual CommandHandler Implementation
//!
//! ```rust,ignore
//! use koilang_rs::{Runtime, CommandHandler, Result};
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
//!     ) -> Result<()> {
//!         match name {
//!             "greet" => {
//!                 let name = args.get(0).and_then(|v| v.as_str()).unwrap_or("World");
//!                 println!("Hello, {}!", name);
//!                 Ok(())
//!             }
//!             _ => Err(koilang_rs::KoiError::command_not_found(name, 0)),
//!         }
//!     }
//! }
//!
//! let mut runtime = Runtime::new();
//! runtime.env_enter(Box::new(MyEnv));
//! runtime.execute(r#"#greet "Alice""#).unwrap();
//! ```
//!
//! ## Programmatic Command Execution
//!
//! ```rust,ignore
//! use koilang_rs::Runtime;
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
//! ```rust,ignore
//! use koilang_rs::Writer;
//!
//! let mut buffer = Vec::new();
//! let mut writer = Writer::new(&mut buffer, None).unwrap();
//!
//! writer.command("character", &["Alice".into(), "Hello!".into()], &HashMap::new()).unwrap();
//! writer.text("This is narrative text.").unwrap();
//! writer.close().unwrap();
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
pub use koicore::parser::{Parser, ParserConfig};
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
        runtime.execute(r#"#test_command"#).unwrap();

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
        runtime.execute(r#"#greet "Test""#).unwrap();

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
}
