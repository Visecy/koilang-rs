# Control Flow Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `KoiError::JumpRequest` with `std::ops::ControlFlow<FlowBreak<E>>` for proper control flow signaling, and implement `probe_until` with on-demand cache filling.

**Architecture:** Define `FlowBreak<E>` enum with `Jump`, `ProbeNeeded`, and `Error` variants. `DispatchResult` is a type alias for `ControlFlow<FlowBreak<E>>`. Handler trait returns `DispatchResult`. `execution_loop` handles `ProbeNeeded` by filling cache from parser. Macro generates `DispatchResult`-returning code.

**Tech Stack:** Rust, `std::ops::ControlFlow` + `Try` trait, `koicore` parser

---

### Task 1: Add `FlowBreak` and `DispatchResult` types to `error.rs`

**Files:**
- Modify: `src/error.rs`

- [ ] **Step 1: Add `FlowBreak` enum and `DispatchResult` type alias after the `KoiError` enum and before `impl KoiError`**

Add these types after line 38 (after the `KoiError` enum closing brace):

```rust
/// Control flow break signals for the execution loop.
///
/// This enum represents non-continuation outcomes from command dispatch:
/// jumps, cache probing requests, and errors.
pub enum FlowBreak<E = KoiError> {
    /// Jump to a specific position in the command cache.
    Jump(usize),
    /// Need more commands in cache before proceeding.
    /// Carries the strategy and offset so the execution loop can continue
    /// filling the cache until the target is found.
    ProbeNeeded {
        strategy: Box<dyn FnMut(&Command, usize) -> bool>,
        offset: i32,
    },
    /// An error occurred.
    Error(E),
}

/// Result of command dispatch with control flow support.
///
/// Uses `std::ops::ControlFlow` to distinguish between normal continuation
/// (`Continue(())`) and control flow breaks (`Jump`, `ProbeNeeded`, `Error`).
pub type DispatchResult<E = KoiError> = std::ops::ControlFlow<FlowBreak<E>>;
```

Add the `Command` import at the top of the file (after `use std::io;`):

```rust
use koicore::command::Command;
```

- [ ] **Step 2: Add `FromResidual` implementation for `DispatchResult` after the `DispatchResult` type alias**

```rust
impl<E> std::ops::FromResidual<std::result::Result<std::convert::Infallible, E>> for DispatchResult<E> {
    fn from_residual(residual: std::result::Result<std::convert::Infallible, E>) -> Self {
        match residual {
            Err(e) => std::ops::ControlFlow::Break(FlowBreak::Error(e)),
            Ok(i) => match i {},
        }
    }
}
```

- [ ] **Step 3: Remove `JumpRequest` variant from `KoiError`**

Remove lines 23-29 (the `JumpRequest` variant and its doc comment):

```rust
// REMOVE THIS BLOCK:
    /// Jump request for control flow.
    /// This is not a real error but a control flow mechanism.
    #[error("Jump to position {position}")]
    JumpRequest {
        /// Target position to jump to.
        position: usize,
    },
```

- [ ] **Step 4: Remove `jump_request`, `is_jump_request`, and `jump_position` methods from `impl KoiError`**

Remove lines 55-71 (the three methods):

```rust
// REMOVE THIS BLOCK:
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
```

- [ ] **Step 5: Remove the `test_jump_request` test**

Remove lines 97-101:

```rust
// REMOVE THIS BLOCK:
    #[test]
    fn test_jump_request() {
        let err = KoiError::jump_request(42);
        assert!(err.is_jump_request());
        assert_eq!(err.jump_position(), Some(42));
    }
```

- [ ] **Step 6: Run `cargo check` to verify compilation**

Run: `cd /home/ovizro/Code/koilang-rs && cargo check 2>&1`
Expected: Compile errors in `runtime.rs` and `handler.rs` (referencing removed `JumpRequest` and old `Result<()>` signatures). This is expected — we'll fix them in subsequent tasks.

- [ ] **Step 7: Commit**

```bash
git add src/error.rs
git commit -m "feat: add FlowBreak/DispatchResult types, remove JumpRequest from KoiError"
```

---

### Task 2: Update `CommandHandler` trait to return `DispatchResult`

**Files:**
- Modify: `src/handler.rs`

- [ ] **Step 1: Change the import from `Result` to `DispatchResult` and `FlowBreak`**

Replace line 57:

```rust
use crate::error::Result;
```

with:

```rust
use crate::error::{DispatchResult, FlowBreak, KoiError};
```

- [ ] **Step 2: Update `CommandHandler::handle_command` return type**

Replace lines 113-119:

```rust
    fn handle_command(
        &mut self,
        name: &str,
        args: &[Value],
        kwargs: &HashMap<String, Value>,
        runtime: &mut Runtime,
    ) -> Result<()>;
```

with:

```rust
    fn handle_command(
        &mut self,
        name: &str,
        args: &[Value],
        kwargs: &HashMap<String, Value>,
        runtime: &mut Runtime,
    ) -> DispatchResult;
```

- [ ] **Step 3: Update doc comment on `handle_command`**

Replace lines 94-97:

```rust
    /// Returns `Ok(())` if the command was handled successfully, or an error
    /// if something went wrong.
```

with:

```rust
    /// Returns `Continue(())` if the command was handled normally,
    /// or a `Break` with the appropriate control flow signal (jump, probe, or error).
```

- [ ] **Step 4: Update `dispatch_to_handler` return type**

Replace lines 126-134:

```rust
pub(crate) fn dispatch_to_handler(
    handler: &mut dyn CommandHandler,
    name: &str,
    args: &[Value],
    kwargs: &HashMap<String, Value>,
    runtime: &mut Runtime,
) -> Result<()> {
    handler.handle_command(name, args, kwargs, runtime)
}
```

with:

```rust
pub(crate) fn dispatch_to_handler(
    handler: &mut dyn CommandHandler,
    name: &str,
    args: &[Value],
    kwargs: &HashMap<String, Value>,
    runtime: &mut Runtime,
) -> DispatchResult {
    handler.handle_command(name, args, kwargs, runtime)
}
```

- [ ] **Step 5: Update `TestHandler` in tests**

Replace lines 145-159:

```rust
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
```

with:

```rust
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
                std::ops::ControlFlow::Break(FlowBreak::Error(KoiError::runtime("test failure")))
            } else {
                std::ops::ControlFlow::Continue(())
            }
        }
    }
```

- [ ] **Step 6: Update doc example in module-level comment**

Replace lines 26-52 (the `Result<()>` return in the doc example):

```rust
    ) -> Result<()> {
```

with:

```rust
    ) -> DispatchResult {
```

And replace the match arms in the doc example:

```rust
            "increment" => {
                self.counter += 1;
                println!("Counter: {}", self.counter);
                Ok(())
            }
            "@start" => {
                println!("Environment started");
                Ok(())
            }
            "@end" => {
                println!("Environment ended");
                Ok(())
            }
            "@text" => {
                let content = args.get(0)
                    .map(|v| match v {
                        Value::String(s) => s.as_str(),
                        _ => "",
                    })
                    .unwrap_or("");
                println!("Text: {}", content);
                Ok(())
            }
            _ => Err(koilang::KoiError::command_not_found(name)),
```

with:

```rust
            "increment" => {
                self.counter += 1;
                println!("Counter: {}", self.counter);
                std::ops::ControlFlow::Continue(())
            }
            "@start" => {
                println!("Environment started");
                std::ops::ControlFlow::Continue(())
            }
            "@end" => {
                println!("Environment ended");
                std::ops::ControlFlow::Continue(())
            }
            "@text" => {
                let content = args.get(0)
                    .map(|v| match v {
                        Value::String(s) => s.as_str(),
                        _ => "",
                    })
                    .unwrap_or("");
                println!("Text: {}", content);
                std::ops::ControlFlow::Continue(())
            }
            _ => std::ops::ControlFlow::Break(koilang::FlowBreak::Error(koilang::KoiError::command_not_found(name))),
```

- [ ] **Step 7: Commit**

```bash
git add src/handler.rs
git commit -m "feat: update CommandHandler trait to return DispatchResult"
```

---

### Task 3: Update `koilang-macros` to generate `DispatchResult`-returning code

**Files:**
- Modify: `koilang-macros/src/lib.rs`

- [ ] **Step 1: Update the generated `handle_command` return type**

Replace lines 447-454 in the `command_handler` function:

```rust
        impl #koi::CommandHandler for #self_ty {
            fn handle_command(
                &mut self,
                name: &str,
                args: &[#koi::Value],
                _kwargs: &::std::collections::HashMap<String, #koi::Value>,
                runtime: &mut #koi::Runtime,
            ) -> #koi::Result<()> {
```

with:

```rust
        impl #koi::CommandHandler for #self_ty {
            fn handle_command(
                &mut self,
                name: &str,
                args: &[#koi::Value],
                _kwargs: &::std::collections::HashMap<String, #koi::Value>,
                runtime: &mut #koi::Runtime,
            ) -> #koi::DispatchResult {
```

- [ ] **Step 2: Update match arm to return `Continue(())` instead of `Ok(())`**

Replace line 437:

```rust
                self.#method_ident(#(#arg_expressions),*);
                Ok(())
```

with:

```rust
                self.#method_ident(#(#arg_expressions),*);
                ::std::ops::ControlFlow::Continue(())
```

- [ ] **Step 3: Update the fallback match arm to return `FlowBreak::Error`**

Replace line 457:

```rust
                    _ => Err(#koi::KoiError::command_not_found(name)),
```

with:

```rust
                    _ => ::std::ops::ControlFlow::Break(#koi::FlowBreak::Error(#koi::KoiError::command_not_found(name))),
```

- [ ] **Step 4: Update error returns in `generate_arg_extraction`**

All `return Err(#koi::KoiError::runtime(...))` in the `generate_arg_extraction` function need to change to `return ::std::ops::ControlFlow::Break(#koi::FlowBreak::Error(#koi::KoiError::runtime(...)))`. There are multiple occurrences. Replace all instances of:

```rust
return Err(#koi::KoiError::runtime(
```

with:

```rust
return ::std::ops::ControlFlow::Break(#koi::FlowBreak::Error(#koi::KoiError::runtime(
```

And add the corresponding closing parenthesis. For each such line, the pattern changes from:

```rust
return Err(#koi::KoiError::runtime(
    format!("type mismatch for argument {}: expected ..., got {}", #index, type_name)
));
```

to:

```rust
return ::std::ops::ControlFlow::Break(#koi::FlowBreak::Error(#koi::KoiError::runtime(
    format!("type mismatch for argument {}: expected ..., got {}", #index, type_name)
)));
```

There are 7 occurrences in `generate_arg_extraction` (String, &str, i32/i64, f64 with allow_int_to_float, f64 without, bool, and one more). Each `return Err(#koi::KoiError::runtime(` becomes `return ::std::ops::ControlFlow::Break(#koi::FlowBreak::Error(#koi::KoiError::runtime(` and each `));` closing becomes `)));`.

- [ ] **Step 5: Commit**

```bash
git add koilang-macros/src/lib.rs
git commit -m "feat: update macro to generate DispatchResult-returning code"
```

---

### Task 4: Update `Runtime` methods in `runtime.rs`

**Files:**
- Modify: `src/runtime.rs`

- [ ] **Step 1: Update imports**

Replace line 6:

```rust
use crate::error::{KoiError, Result};
```

with:

```rust
use crate::error::{DispatchResult, FlowBreak, KoiError, Result};
```

- [ ] **Step 2: Update `jump_to_position` to return `DispatchResult`**

Replace lines 142-153:

```rust
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
```

with:

```rust
    /// Jump to a specific position in the command cache.
    ///
    /// This returns a `Break(FlowBreak::Jump)` signal handled by the execution loop.
    pub fn jump_to_position(&self, position: usize) -> DispatchResult {
        if !self.cache_enabled {
            return ControlFlow::Break(FlowBreak::Error(
                KoiError::runtime("Cache must be enabled for jumps"),
            ));
        }

        ControlFlow::Break(FlowBreak::Jump(position))
    }
```

- [ ] **Step 3: Update `jump_to_label` to return `DispatchResult`**

Replace lines 155-167:

```rust
    pub fn jump_to_label(&self, label: &str) -> Result<()> {
        match self.label_index.get(label) {
            Some(&pos) => self.jump_to_position(pos),
            None => Err(KoiError::runtime(
                format!("Label '{}' not found", label),
            )),
        }
    }
```

with:

```rust
    pub fn jump_to_label(&self, label: &str) -> DispatchResult {
        match self.label_index.get(label) {
            Some(&pos) => self.jump_to_position(pos),
            None => ControlFlow::Break(FlowBreak::Error(
                KoiError::runtime(format!("Label '{}' not found", label)),
            )),
        }
    }
```

- [ ] **Step 4: Update `scan_and_jump` to return `DispatchResult` and emit `ProbeNeeded`**

Replace lines 169-201:

```rust
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
                let target = pos as i64 + offset as i64;
                if target < 0 || target as usize >= self.command_cache.len() {
                    return Err(KoiError::runtime(
                        format!("Jump target position {} out of bounds (0..{})", target, self.command_cache.len()),
                    ));
                }
                return self.jump_to_position(target as usize);
            }
        }

        Err(KoiError::runtime("Jump target not found"))
    }
```

with:

```rust
    /// Scan forward and jump to a command matching the strategy.
    ///
    /// If the target is not yet in the cache, returns `ProbeNeeded` so the
    /// execution loop can fill the cache from the parser and retry.
    ///
    /// # Arguments
    ///
    /// * `strategy` - Function that returns true when the target is found
    /// * `offset` - Offset to apply to the found position
    pub fn scan_and_jump<F>(&mut self, mut strategy: F, offset: i32) -> DispatchResult
    where
        F: FnMut(&Command, usize) -> bool + 'static,
    {
        if !self.cache_enabled {
            return ControlFlow::Break(FlowBreak::Error(
                KoiError::runtime("Cache must be enabled for scan_and_jump"),
            ));
        }

        for pos in self.current_position + 1..self.command_cache.len() {
            let cmd = &self.command_cache[pos];
            if strategy(cmd, pos) {
                let target = pos as i64 + offset as i64;
                if target < 0 || target as usize >= self.command_cache.len() {
                    return ControlFlow::Break(FlowBreak::Error(
                        KoiError::runtime(format!(
                            "Jump target position {} out of bounds (0..{})",
                            target, self.command_cache.len()
                        )),
                    ));
                }
                return ControlFlow::Break(FlowBreak::Jump(target as usize));
            }
        }

        // Target not in cache yet — signal the execution loop to keep parsing
        ControlFlow::Break(FlowBreak::ProbeNeeded {
            strategy: Box::new(strategy),
            offset,
        })
    }
```

- [ ] **Step 5: Update `probe_until` to return `DispatchResult` and emit `ProbeNeeded`**

Replace lines 203-216:

```rust
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
```

with:

```rust
    /// Probe (fill cache) until a condition is met, without jumping.
    ///
    /// If the target is not yet in the cache, returns `ProbeNeeded` with
    /// `offset: 0` so the execution loop can fill the cache from the parser.
    pub fn probe_until<F>(&mut self, mut strategy: F) -> DispatchResult
    where
        F: FnMut(&Command, usize) -> bool + 'static,
    {
        if !self.cache_enabled {
            return ControlFlow::Break(FlowBreak::Error(
                KoiError::runtime("Cache must be enabled for probe_until"),
            ));
        }

        for pos in self.current_position + 1..self.command_cache.len() {
            if strategy(&self.command_cache[pos], pos) {
                return ControlFlow::Continue(());
            }
        }

        ControlFlow::Break(FlowBreak::ProbeNeeded {
            strategy: Box::new(strategy),
            offset: 0,
        })
    }
```

- [ ] **Step 6: Update `jump_to_matching` return type**

Replace lines 218-256:

```rust
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
```

with:

```rust
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
    ) -> DispatchResult {
```

- [ ] **Step 7: Update `execute_command_internal` to return `DispatchResult`**

Replace lines 290-322:

```rust
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
```

with:

```rust
    /// Execute a command, searching the environment stack.
    fn execute_command_internal(&mut self, cmd: &Command) -> DispatchResult {
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
                ControlFlow::Continue(()) => return ControlFlow::Continue(()),
                ControlFlow::Break(FlowBreak::Error(KoiError::CommandNotFound { .. })) => continue,
                other => return other,
            }
        }

        // Special case: @annotation is silently ignored if not handled
        if name == "@annotation" {
            return ControlFlow::Continue(());
        }

        ControlFlow::Break(FlowBreak::Error(KoiError::command_not_found(name)))
    }
```

- [ ] **Step 8: Update `dispatch` to return `DispatchResult`**

Replace lines 324-329:

```rust
    /// Dispatch a command through the middleware chain.
    fn dispatch(&mut self, cmd: &Command) -> Result<()> {
        // For simplicity, execute directly without middleware chain for now
        // A full implementation would build a proper middleware chain
        self.execute_command_internal(cmd)
    }
```

with:

```rust
    /// Dispatch a command through the middleware chain.
    fn dispatch(&mut self, cmd: &Command) -> DispatchResult {
        // For simplicity, execute directly without middleware chain for now
        // A full implementation would build a proper middleware chain
        self.execute_command_internal(cmd)
    }
```

- [ ] **Step 9: Update `dispatch_args` to return `DispatchResult`**

Replace lines 331-345:

```rust
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
```

with:

```rust
    /// Dispatch a command by name and arguments.
    fn dispatch_args(
        &mut self,
        name: &str,
        args: &[Value],
        _kwargs: &HashMap<String, Value>,
    ) -> DispatchResult {
        // Create a temporary command
        use koicore::command::Parameter;
        let params: Vec<Parameter> = args.iter().cloned().map(Parameter::from).collect();
        let cmd = Command::new(name, params);

        // Execute directly without middleware for internal calls
        self.execute_command_internal(&cmd)
    }
```

- [ ] **Step 10: Update `execution_loop` to handle `DispatchResult`**

Replace lines 396-438:

```rust
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
```

with:

```rust
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
                ControlFlow::Continue(()) => {
                    // Move to next position
                    self.current_position += 1;
                }
                ControlFlow::Break(FlowBreak::Jump(position)) => {
                    self.current_position = position;
                }
                ControlFlow::Break(FlowBreak::ProbeNeeded { mut strategy, offset }) => {
                    // Fill cache from parser until strategy matches or EOF
                    loop {
                        match parser.next_command() {
                            Ok(Some(cmd)) => {
                                self.command_cache.push(cmd);
                                let pos = self.command_cache.len() - 1;
                                if strategy(&self.command_cache[pos], pos) {
                                    let target = pos as i64 + offset as i64;
                                    if target < 0 || target as usize >= self.command_cache.len() {
                                        return Err(KoiError::runtime(format!(
                                            "Jump target position {} out of bounds (0..{})",
                                            target, self.command_cache.len()
                                        )));
                                    }
                                    self.current_position = target as usize;
                                    break;
                                }
                                // Strategy not matched, continue filling cache
                            }
                            Ok(None) => {
                                return Err(KoiError::runtime(
                                    "Jump target not found: end of input"
                                ));
                            }
                            Err(e) => return Err(KoiError::Parse(*e)),
                        }
                    }
                }
                ControlFlow::Break(FlowBreak::Error(e)) => return Err(e),
            }
        }

        Ok(())
    }
```

- [ ] **Step 11: Update `run_with_parser` lifecycle calls**

The `env.handle_command("@start", ...)` and `env.handle_command("@end", ...)` calls in `run_with_parser` (lines 374, 389) now return `DispatchResult`. Since lifecycle hooks should never trigger jumps, we can ignore the result. The existing `let _ =` already handles this — no change needed.

- [ ] **Step 12: Update `env_exit` lifecycle call**

Line 277: `env.handle_command("@end", &[], &HashMap::new(), self)` now returns `DispatchResult`. The existing `let _ =` already handles this — no change needed.

- [ ] **Step 13: Update `execute_on_environment` return type**

Replace lines 569-587:

```rust
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
```

with:

```rust
    pub fn execute_on_environment(
        &mut self,
        index: usize,
        name: &str,
        args: &[Value],
        kwargs: &HashMap<String, Value>,
    ) -> DispatchResult {
        if index >= self.env_stack.len() {
            return ControlFlow::Break(FlowBreak::Error(
                KoiError::runtime(format!("Environment index {} out of bounds", index)),
            ));
        }

        // Use raw pointer to work around borrow checker
        let runtime_ptr = self as *mut Runtime;
        let env = &mut self.env_stack[index];
        let runtime_ref = unsafe { &mut *runtime_ptr };
        env.handle_command(name, args, kwargs, runtime_ref)
    }
```

- [ ] **Step 14: Update `execute_command_obj` return type**

Replace lines 547-549:

```rust
    pub fn execute_command_obj(&mut self, cmd: &Command) -> Result<()> {
        self.dispatch(cmd)
    }
```

with:

```rust
    pub fn execute_command_obj(&mut self, cmd: &Command) -> DispatchResult {
        self.dispatch(cmd)
    }
```

- [ ] **Step 15: Update `TestEnv` in `runtime.rs` tests**

Replace lines 711-722:

```rust
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
```

with:

```rust
    impl CommandHandler for TestEnv {
        fn handle_command(
            &mut self,
            name: &str,
            _args: &[Value],
            _kwargs: &HashMap<String, Value>,
            _runtime: &mut Runtime,
        ) -> DispatchResult {
            self.commands.lock().unwrap().push(name.to_string());
            ControlFlow::Continue(())
        }
    }
```

- [ ] **Step 16: Commit**

```bash
git add src/runtime.rs
git commit -m "feat: update Runtime methods to use DispatchResult, implement ProbeNeeded handling"
```

---

### Task 5: Update `lib.rs` exports and integration tests

**Files:**
- Modify: `src/lib.rs`

- [ ] **Step 1: Update public exports**

Replace line 131:

```rust
pub use error::{KoiError, Result};
```

with:

```rust
pub use error::{DispatchResult, FlowBreak, KoiError, Result};
```

- [ ] **Step 2: Update `TestEnv` in `lib.rs` integration tests**

Replace lines 154-165:

```rust
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
```

with:

```rust
    impl CommandHandler for TestEnv {
        fn handle_command(
            &mut self,
            name: &str,
            _args: &[Value],
            _kwargs: &HashMap<String, Value>,
            _runtime: &mut Runtime,
        ) -> DispatchResult {
            self.commands.push(name.to_string());
            std::ops::ControlFlow::Continue(())
        }
    }
```

- [ ] **Step 3: Commit**

```bash
git add src/lib.rs
git commit -m "feat: export DispatchResult/FlowBreak, update integration tests"
```

---

### Task 6: Build and test

**Files:**
- None (verification only)

- [ ] **Step 1: Run `cargo check`**

Run: `cd /home/ovizro/Code/koilang-rs && cargo check 2>&1`
Expected: No errors

- [ ] **Step 2: Run `cargo test`**

Run: `cd /home/ovizro/Code/koilang-rs && cargo test 2>&1`
Expected: All tests pass

- [ ] **Step 3: Fix any compilation or test failures**

If any tests fail or compilation errors remain, fix them and re-run.

- [ ] **Step 4: Final commit if fixes were needed**

```bash
git add -A
git commit -m "fix: resolve compilation/test issues from control flow redesign"
```
