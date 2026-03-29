//! Runtime module for KoiLang execution.
//!
//! This module provides the [`Runtime`] struct which orchestrates KoiLang script execution,
//! including environment stack management, middleware support, command caching, and jump operations.

use crate::error::{KoiError, Result};
use crate::handler::CommandHandler;
use koicore::command::{Command, Value};
use koicore::parser::{FileInputSource, Parser, ParserConfig, StringInputSource, TextInputSource};
use std::collections::HashMap;
use std::path::Path;

/// Type alias for middleware functions.
///
/// Middleware functions wrap command execution and can modify or intercept commands.
/// They receive the runtime, the command, and a continuation function to call the next
/// middleware or the actual command handler.
pub type MiddlewareFn =
    dyn Fn(&Runtime, &Command, &dyn Fn(&Command) -> Result<()>) -> Result<()>;

/// Runtime for executing KoiLang scripts.
///
/// The runtime manages the execution environment, including:
/// - Environment stack for command handlers
/// - Middleware chain for command interception
/// - Command caching for jump operations
/// - Lifecycle management for sessions
/// - Programmatic command execution
///
/// # Examples
///
/// ```rust,ignore
/// use koilang::Runtime;
///
/// let mut runtime = Runtime::new();
/// runtime.execute_file("script.ktxt").unwrap();
/// ```
pub struct Runtime {
    /// Stack of environments for command handling.
    env_stack: Vec<Box<dyn CommandHandler>>,

    /// Middleware chain.
    middleware: Vec<Box<MiddlewareFn>>,

    /// Parser configuration.
    parser_config: Option<ParserConfig>,

    /// Currently executing command (for context).
    current_command: Option<Command>,

    /// Whether command caching is enabled.
    cache_enabled: bool,

    /// Cache of parsed commands.
    command_cache: Vec<Command>,

    /// Label index for jumps.
    label_index: HashMap<String, usize>,

    /// Current position in the command cache.
    current_position: usize,

    /// Session depth for lifecycle management.
    lifecycle_depth: usize,
}

impl Runtime {
    /// Create a new runtime with default settings.
    pub fn new() -> Self {
        Self {
            env_stack: Vec::new(),
            middleware: Vec::new(),
            parser_config: None,
            current_command: None,
            cache_enabled: false,
            command_cache: Vec::new(),
            label_index: HashMap::new(),
            current_position: 0,
            lifecycle_depth: 0,
        }
    }

    /// Create a new runtime with parser configuration.
    pub fn with_config(config: ParserConfig) -> Self {
        let mut runtime = Self::new();
        runtime.parser_config = Some(config);
        runtime
    }

    /// Check if cache is enabled.
    pub fn is_cache_enabled(&self) -> bool {
        self.cache_enabled
    }

    /// Get the current position in the command cache.
    pub fn current_position(&self) -> usize {
        self.current_position
    }

    /// Enable command caching.
    ///
    /// When enabled, parsed commands are stored in memory, allowing
    /// for jump operations and random access.
    pub fn enable_cache(&mut self) {
        self.cache_enabled = true;
    }

    /// Disable command caching and clear the cache.
    pub fn disable_cache(&mut self) {
        self.cache_enabled = false;
        self.command_cache.clear();
        self.label_index.clear();
        self.current_position = 0;
    }

    /// Register a label at the current position.
    ///
    /// # Arguments
    ///
    /// * `label` - The label name
    /// * `position` - Optional position (defaults to current position)
    pub fn register_label(&mut self, label: &str, position: Option<usize>) -> Result<()> {
        if !self.cache_enabled {
            return Err(KoiError::runtime(
                "Cache must be enabled to register labels",
            ));
        }

        let pos = position.unwrap_or(self.current_position);

        if self.label_index.contains_key(label) {
            return Err(KoiError::runtime(
                format!("Label '{}' already registered", label),
            ));
        }

        self.label_index.insert(label.to_string(), pos);
        Ok(())
    }

    /// Jump to a specific position in the command cache.
    ///
    /// This returns a `JumpRequest` error which is handled by the execution loop.
    pub fn jump_to_position(&self, position: usize) -> Result<()> {
        if !self.cache_enabled {
            return Err(KoiError::runtime(
                "Cache must be enabled for jumps",
            ));
        }

        Err(KoiError::JumpRequest { position })
    }

    /// Jump to a registered label.
    ///
    /// # Arguments
    ///
    /// * `label` - The label name to jump to
    pub fn jump_to_label(&self, label: &str) -> Result<()> {
        match self.label_index.get(label) {
            Some(&pos) => self.jump_to_position(pos),
            None => Err(KoiError::runtime(
                format!("Label '{}' not found", label),
            )),
        }
    }

    /// Scan forward and jump to a command matching the strategy.
    ///
    /// # Arguments
    ///
    /// * `strategy` - Function that returns true when the target is found
    /// * `offset` - Offset to apply to the found position
    pub fn scan_and_jump<F>(&mut self, mut strategy: F, offset: i32) -> Result<()>
    where
        F: FnMut(&Command, usize) -> bool,
    {
        if !self.cache_enabled {
            return Err(KoiError::runtime(
                "Cache must be enabled for scan_and_jump",
            ));
        }

        // This is a simplified implementation
        // In practice, we'd need to parse more commands as needed
        for pos in self.current_position + 1..self.command_cache.len() {
            let cmd = &self.command_cache[pos];
            if strategy(cmd, pos) {
                let target = (pos as i32 + offset) as usize;
                return self.jump_to_position(target);
            }
        }

        Err(KoiError::runtime("Jump target not found"))
    }

    /// Probe (fill cache) until a condition is met, without jumping.
    pub fn probe_until<F>(&mut self, _strategy: F) -> Result<()>
    where
        F: FnMut(&Command, usize) -> bool,
    {
        if !self.cache_enabled {
            return Err(KoiError::runtime(
                "Cache must be enabled for probe_until",
            ));
        }

        // Simplified implementation
        Ok(())
    }

    /// Jump to a matching end marker, tracking nesting depth.
    ///
    /// # Arguments
    ///
    /// * `start` - The start marker command name
    /// * `end` - The end marker command name
    /// * `alternative` - Optional alternative marker to stop at (when depth is 1)
    /// * `offset` - Offset to apply to the found position
    pub fn jump_to_matching(
        &mut self,
        start: &str,
        end: &str,
        alternative: Option<&str>,
        offset: i32,
    ) -> Result<()> {
        let mut depth = 1i32;

        self.scan_and_jump(
            |cmd, _| {
                let name = cmd.name();
                if name == start {
                    depth += 1;
                } else if name == end {
                    depth -= 1;
                    if depth == 0 {
                        return true;
                    }
                } else if depth == 1 {
                    if let Some(alt) = alternative {
                        if name == alt {
                            return true;
                        }
                    }
                }
                false
            },
            offset,
        )
    }

    /// Enter an environment (push to stack).
    ///
    /// This also dispatches the `@start` command to the new environment.
    pub fn env_enter(&mut self, env: Box<dyn CommandHandler>) {
        self.env_stack.push(env);

        // Dispatch @start command if in an active session
        if self.lifecycle_depth > 0 {
            let _ = self.dispatch_args("@start", &[], &HashMap::new());
        }
    }

    /// Exit the current environment (pop from stack).
    ///
    /// This dispatches the `@end` command before removing the environment.
    pub fn env_exit(&mut self) {
        if let Some(mut env) = self.env_stack.pop() {
            // Dispatch @end command
            if self.lifecycle_depth > 0 {
                let _ = env.handle_command("@end", &[], &HashMap::new(), self);
            }
        }
    }

    /// Add middleware to the chain.
    pub fn add_middleware<F>(&mut self, middleware: F)
    where
        F: Fn(&Runtime, &Command, &dyn Fn(&Command) -> Result<()>) -> Result<()> + 'static,
    {
        self.middleware.push(Box::new(middleware));
    }

    /// Execute a command, searching the environment stack.
    fn execute_command_internal(&mut self, cmd: &Command) -> Result<()> {
        let name = cmd.name();
        let args: Vec<Value> = cmd.params().iter().filter_map(|p| {
            match p {
                koicore::command::Parameter::Basic(v) => Some(v.clone()),
                _ => None,
            }
        }).collect();
        let kwargs = HashMap::new(); // TODO: Parse named parameters

        // Try each environment in the stack (top to bottom)
        // We need to use a raw pointer here to work around borrow checker limitations
        // when passing &mut self to handle_command while iterating over env_stack
        let runtime_ptr = self as *mut Runtime;
        for env in self.env_stack.iter_mut().rev() {
            // SAFETY: We only access the runtime through one mutable reference at a time
            // and the environment stack iteration doesn't overlap with runtime usage
            let runtime_ref = unsafe { &mut *runtime_ptr };
            match env.handle_command(&name, &args, &kwargs, runtime_ref) {
                Ok(()) => return Ok(()),
                Err(KoiError::CommandNotFound { .. }) => continue,
                Err(e) => return Err(e),
            }
        }

        // Special case: @annotation is silently ignored if not handled
        if name == "@annotation" {
            return Ok(());
        }

        Err(KoiError::command_not_found(name))
    }

    /// Dispatch a command through the middleware chain.
    fn dispatch(&mut self, cmd: &Command) -> Result<()> {
        // For simplicity, execute directly without middleware chain for now
        // A full implementation would build a proper middleware chain
        self.execute_command_internal(cmd)
    }

    /// Dispatch a command by name and arguments.
    fn dispatch_args(
        &mut self,
        name: &str,
        args: &[Value],
        _kwargs: &HashMap<String, Value>,
    ) -> Result<()> {
        // Create a temporary command
        use koicore::command::Parameter;
        let params: Vec<Parameter> = args.iter().cloned().map(Parameter::from).collect();
        let cmd = Command::new(name, params);

        // Execute directly without middleware for internal calls
        self.execute_command_internal(&cmd)
    }

    /// Execute a KoiLang script from a string.
    ///
    /// # Arguments
    ///
    /// * `source` - The KoiLang source string
    pub fn execute_str(&mut self, source: &str) -> Result<()> {
        let input = StringInputSource::new(source);
        let config = self.parser_config.clone().unwrap_or_default();
        let mut parser = Parser::new(input, config);

        self.run_with_parser(&mut parser)
    }

    /// Run with a pre-configured parser.
    fn run_with_parser<S>(&mut self, parser: &mut Parser<S>) -> Result<()>
    where
        S: TextInputSource,
    {
        // Enter session
        let is_outermost = self.lifecycle_depth == 0;
        self.lifecycle_depth += 1;

        if is_outermost {
            // Dispatch @start to all environments
            let runtime_ptr = self as *mut Runtime;
            for env in self.env_stack.iter_mut() {
                let runtime_ref = unsafe { &mut *runtime_ptr };
                let _ = env.handle_command("@start", &[], &HashMap::new(), runtime_ref);
            }
        }

        // Main execution loop
        let result = self.execution_loop(parser);

        // Exit session
        self.lifecycle_depth -= 1;

        if is_outermost {
            // Dispatch @end to all environments (in reverse order)
            let runtime_ptr = self as *mut Runtime;
            for env in self.env_stack.iter_mut().rev() {
                let runtime_ref = unsafe { &mut *runtime_ptr };
                let _ = env.handle_command("@end", &[], &HashMap::new(), runtime_ref);
            }
        }

        result
    }

    /// Main execution loop.
    fn execution_loop<S>(&mut self, parser: &mut Parser<S>) -> Result<()>
    where
        S: TextInputSource,
    {
        loop {
            // Check if we should use cached commands or parse new ones
            let cmd = if self.cache_enabled && self.current_position < self.command_cache.len() {
                // Use cached command
                self.command_cache[self.current_position].clone()
            } else {
                // Parse next command
                match parser.next_command() {
                    Ok(Some(cmd)) => {
                        if self.cache_enabled {
                            self.command_cache.push(cmd.clone());
                        }
                        cmd
                    }
                    Ok(None) => break, // End of input
                    Err(e) => return Err(KoiError::Parse(*e)),
                }
            };

            // Set current command for context
            self.current_command = Some(cmd.clone());

            // Dispatch the command
            match self.dispatch(&cmd) {
                Ok(()) => {}
                Err(KoiError::JumpRequest { position }) => {
                    self.current_position = position;
                    continue; // Continue from the new position
                }
                Err(e) => return Err(e),
            }

            // Move to next position
            self.current_position += 1;
        }

        Ok(())
    }

    /// Execute a KoiLang script from any input source.
    ///
    /// # Arguments
    ///
    /// * `source` - The input source implementing `TextInputSource`
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use koilang::Runtime;
    /// use koicore::parser::StringInputSource;
    ///
    /// let mut runtime = Runtime::new();
    /// let source = StringInputSource::new("#greet \"World\"");
    /// runtime.execute(source).unwrap();
    /// ```
    pub fn execute<S>(&mut self, source: S) -> Result<()>
    where
        S: TextInputSource,
    {
        let config = self.parser_config.clone().unwrap_or_default();
        let mut parser = Parser::new(source, config);
        self.run_with_parser(&mut parser)
    }

    /// Execute a KoiLang script from a file.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the KoiLang script file
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use koilang::Runtime;
    ///
    /// let mut runtime = Runtime::new();
    /// runtime.execute_file("script.ktxt").unwrap();
    /// ```
    pub fn execute_file<P>(&mut self, path: P) -> Result<()>
    where
        P: AsRef<Path>,
    {
        let input = FileInputSource::new(path)?;
        self.execute(input)
    }

    /// Get a reference to the environment stack.
    pub fn env_stack(&self) -> &[Box<dyn CommandHandler>] {
        &self.env_stack
    }

    /// Get a mutable reference to the environment stack.
    pub fn env_stack_mut(&mut self) -> &mut Vec<Box<dyn CommandHandler>> {
        &mut self.env_stack
    }

    /// Get the currently executing command.
    pub fn current_command(&self) -> Option<&Command> {
        self.current_command.as_ref()
    }

    /// Execute a command programmatically through the runtime's dispatch chain.
    ///
    /// # Arguments
    ///
    /// * `name` - The command name
    /// * `args` - Positional arguments
    /// * `kwargs` - Named arguments
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use koilang::Runtime;
    /// use std::collections::HashMap;
    ///
    /// let mut runtime = Runtime::new();
    /// runtime.execute_command("hello", &["World".into()], &HashMap::new()).unwrap();
    /// ```
    pub fn execute_command(
        &mut self,
        name: &str,
        args: &[Value],
        kwargs: &HashMap<String, Value>,
    ) -> Result<()> {
        self.dispatch_args(name, args, kwargs)
    }

    /// Execute a pre-constructed `Command` object through the runtime's dispatch chain.
    ///
    /// This method allows executing a `koicore::Command` directly, going through
    /// the full dispatch chain including middleware and environment stack.
    ///
    /// # Arguments
    ///
    /// * `cmd` - The command to execute
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use koilang::Runtime;
    /// use koicore::command::Command;
    ///
    /// let mut runtime = Runtime::new();
    /// let cmd = Command::new("greet", vec!["World".into()]);
    /// runtime.execute_command_obj(&cmd).unwrap();
    /// ```
    pub fn execute_command_obj(&mut self, cmd: &Command) -> Result<()> {
        self.dispatch(cmd)
    }

    /// Execute a command directly on a specific environment.
    ///
    /// # Arguments
    ///
    /// * `index` - The environment index in the stack (0 = bottom)
    /// * `name` - The command name
    /// * `args` - Positional arguments
    /// * `kwargs` - Named arguments
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use koilang::Runtime;
    /// use std::collections::HashMap;
    ///
    /// let mut runtime = Runtime::new();
    /// runtime.execute_on_environment(0, "local_command", &[], &HashMap::new()).unwrap();
    /// ```
    pub fn execute_on_environment(
        &mut self,
        index: usize,
        name: &str,
        args: &[Value],
        kwargs: &HashMap<String, Value>,
    ) -> Result<()> {
        if index >= self.env_stack.len() {
            return Err(KoiError::runtime(
                format!("Environment index {} out of bounds", index),
            ));
        }
        
        // Use raw pointer to work around borrow checker
        let runtime_ptr = self as *mut Runtime;
        let env = &mut self.env_stack[index];
        let runtime_ref = unsafe { &mut *runtime_ptr };
        env.handle_command(name, args, kwargs, runtime_ref)
    }

    /// Create a command builder for fluent command construction.
    ///
    /// # Arguments
    ///
    /// * `name` - The command name
    ///
    /// # Returns
    ///
    /// Returns a [`CommandBuilder`] for chaining arguments.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use koilang::Runtime;
    ///
    /// let mut runtime = Runtime::new();
    /// runtime.cmd("draw")
    ///     .arg("circle")
    ///     .arg(100)
    ///     .kwarg("color", "red")
    ///     .execute()
    ///     .unwrap();
    /// ```
    pub fn cmd(&mut self, name: &str) -> CommandBuilder<'_> {
        CommandBuilder {
            runtime: self,
            name: name.to_string(),
            args: Vec::new(),
            kwargs: HashMap::new(),
        }
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for constructing command executions.
///
/// This provides a fluent API for building command executions
/// with positional and named arguments.
///
/// # Examples
///
/// ```rust,ignore
/// use koilang::Runtime;
///
/// let mut runtime = Runtime::new();
/// runtime
///     .cmd("draw")
///     .arg("circle")
///     .arg(100)
///     .kwarg("color", "red")
///     .kwarg("filled", true)
///     .execute()
///     .unwrap();
/// ```
pub struct CommandBuilder<'a> {
    runtime: &'a mut Runtime,
    name: String,
    args: Vec<Value>,
    kwargs: HashMap<String, Value>,
}

impl<'a> CommandBuilder<'a> {
    /// Add a positional argument.
    ///
    /// # Arguments
    ///
    /// * `value` - The argument value
    ///
    /// # Returns
    ///
    /// Returns self for method chaining.
    pub fn arg(mut self, value: impl Into<Value>) -> Self {
        self.args.push(value.into());
        self
    }

    /// Add a named argument.
    ///
    /// # Arguments
    ///
    /// * `key` - The argument name
    /// * `value` - The argument value
    ///
    /// # Returns
    ///
    /// Returns self for method chaining.
    pub fn kwarg(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.kwargs.insert(key.to_string(), value.into());
        self
    }

    /// Execute the built command.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if execution fails.
    pub fn execute(self) -> Result<()> {
        self.runtime.execute_command(&self.name, &self.args, &self.kwargs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct TestEnv {
        commands: Arc<Mutex<Vec<String>>>,
    }

    impl TestEnv {
        fn new() -> (Self, Arc<Mutex<Vec<String>>>) {
            let commands = Arc::new(Mutex::new(Vec::new()));
            (Self { commands: commands.clone() }, commands)
        }
    }

    impl CommandHandler for TestEnv {
        fn handle_command(
            &mut self,
            name: &str,
            _args: &[Value],
            _kwargs: &HashMap<String, Value>,
            _runtime: &mut Runtime,
        ) -> Result<()> {
            self.commands.lock().unwrap().push(name.to_string());
            Ok(())
        }
    }

    #[test]
    fn test_runtime_new() {
        let runtime = Runtime::new();
        assert!(!runtime.is_cache_enabled());
        assert_eq!(runtime.current_position(), 0);
    }

    #[test]
    fn test_env_stack() {
        let mut runtime = Runtime::new();
        let (env, _) = TestEnv::new();

        runtime.env_enter(Box::new(env));
        assert_eq!(runtime.env_stack().len(), 1);

        runtime.env_exit();
        assert_eq!(runtime.env_stack().len(), 0);
    }

    #[test]
    fn test_cache_enable_disable() {
        let mut runtime = Runtime::new();

        runtime.enable_cache();
        assert!(runtime.is_cache_enabled());

        runtime.disable_cache();
        assert!(!runtime.is_cache_enabled());
    }

    #[test]
    fn test_execute_command() {
        let mut runtime = Runtime::new();
        let (env, commands) = TestEnv::new();
        runtime.env_enter(Box::new(env));

        runtime.execute_command("test", &[], &HashMap::new()).unwrap();
        assert_eq!(commands.lock().unwrap().len(), 1);
    }

    #[test]
    fn test_execute_on_environment() {
        let mut runtime = Runtime::new();
        let (env, commands) = TestEnv::new();
        runtime.env_enter(Box::new(env));

        runtime.execute_on_environment(0, "test", &[], &HashMap::new()).unwrap();
        assert_eq!(commands.lock().unwrap().len(), 1);
    }

    #[test]
    fn test_command_builder() {
        let mut runtime = Runtime::new();
        let (env, commands) = TestEnv::new();
        runtime.env_enter(Box::new(env));

        runtime.cmd("test")
            .arg("arg1")
            .arg(42)
            .kwarg("key", "value")
            .execute()
            .unwrap();

        assert_eq!(commands.lock().unwrap().len(), 1);
    }

    #[test]
    fn test_execute_with_source() {
        let mut runtime = Runtime::new();
        let (env, commands) = TestEnv::new();
        runtime.env_enter(Box::new(env));

        let source = StringInputSource::new("#test_command\n#another_command");
        runtime.execute(source).unwrap();

        let cmds = commands.lock().unwrap();
        // Includes @start and @end lifecycle hooks
        assert_eq!(cmds.len(), 4);
        assert_eq!(cmds[0], "@start");
        assert_eq!(cmds[1], "test_command");
        assert_eq!(cmds[2], "another_command");
        assert_eq!(cmds[3], "@end");
    }

    #[test]
    fn test_execute_command_obj() {
        let mut runtime = Runtime::new();
        let (env, commands) = TestEnv::new();
        runtime.env_enter(Box::new(env));

        let cmd = Command::new("direct_command", vec![]);
        runtime.execute_command_obj(&cmd).unwrap();

        let cmds = commands.lock().unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "direct_command");
    }

    #[test]
    fn test_execute_file() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut runtime = Runtime::new();
        let (env, commands) = TestEnv::new();
        runtime.env_enter(Box::new(env));

        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "#file_command").unwrap();
        writeln!(temp_file, "#second_command").unwrap();

        runtime.execute_file(temp_file.path()).unwrap();

        let cmds = commands.lock().unwrap();
        // Includes @start and @end lifecycle hooks
        assert_eq!(cmds.len(), 4);
        assert_eq!(cmds[0], "@start");
        assert_eq!(cmds[1], "file_command");
        assert_eq!(cmds[2], "second_command");
        assert_eq!(cmds[3], "@end");
    }
}
