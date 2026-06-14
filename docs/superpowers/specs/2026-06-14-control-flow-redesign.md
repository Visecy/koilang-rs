# Control Flow Redesign for Runtime Jump Logic

## Problem

The current runtime uses `KoiError::JumpRequest` as an error variant to implement jump control flow. This has several issues:

1. **Semantic misuse**: `JumpRequest` is not an error but is forced into the `Error` trait hierarchy via thiserror.
2. **Fragile error handling**: Callers must explicitly match and transparently pass `JumpRequest` in error paths (e.g., `execute_command_internal`), otherwise jumps are silently swallowed as real errors.
3. **Incomplete cache probing**: `scan_and_jump` only scans already-cached commands. When the target hasn't been parsed yet, the scan fails. `probe_until` is a stub that does nothing.
4. **Offset wrap-around**: `scan_and_jump` computes `(pos as i32 + offset) as usize` without bounds checking, causing silent wrap-around on negative results. (Already fixed separately.)

## Design

### Core Types

Replace `KoiError::JumpRequest` with a `ControlFlow`-based dispatch result:

```rust
/// Control flow break signals for the execution loop.
enum FlowBreak<E = KoiError> {
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
type DispatchResult<E = KoiError> = std::ops::ControlFlow<FlowBreak<E>>;
```

Key design decisions:
- **Generic `E`**: `FlowBreak` is parameterized over the error type, defaulting to `KoiError`. This allows external consumers to use custom error types.
- **`ProbeNeeded` carries strategy**: The strategy closure is passed through `FlowBreak` itself, not stored on `Runtime`. This keeps the control flow signal self-contained and avoids coupling probe state to the runtime struct.
- **`std::ops::ControlFlow`**: Reuses the standard library type which already implements `Try`, enabling `?` syntax.

### `FromResidual` for `?` Support

```rust
impl<E> FromResidual<Result<Infallible, E>> for DispatchResult<E> {
    fn from_residual(residual: Result<Infallible, E>) -> Self {
        match residual {
            Err(e) => ControlFlow::Break(FlowBreak::Error(e)),
            Ok(i) => match i {},
        }
    }
}
```

This allows `?` to propagate `Result` errors as `FlowBreak::Error` within `DispatchResult`-returning functions.

### `CommandHandler` Trait Change

```rust
fn handle_command(
    &mut self,
    name: &str,
    args: &[Value],
    kwargs: &HashMap<String, Value>,
    runtime: &mut Runtime,
) -> DispatchResult;  // was Result<()>
```

Handler usage comparison:

```rust
// Before
fn handle_command(&mut self, name: &str, ...) -> Result<()> {
    match name {
        "greet" => { println!("hi"); Ok(()) }
        "jump" => runtime.jump_to_position(42),
        _ => Err(KoiError::command_not_found(name)),
    }
}

// After
fn handle_command(&mut self, name: &str, ...) -> DispatchResult {
    match name {
        "greet" => { println!("hi"); ControlFlow::Continue(()) }
        "jump" => runtime.jump_to_position(42),  // returns DispatchResult directly
        _ => ControlFlow::Break(FlowBreak::Error(KoiError::command_not_found(name))),
    }
}
```

### Jump Method Changes

```rust
// jump_to_position: returns DispatchResult
pub fn jump_to_position(&self, position: usize) -> DispatchResult {
    if !self.cache_enabled {
        return ControlFlow::Break(FlowBreak::Error(
            KoiError::runtime("Cache must be enabled for jumps")
        ));
    }
    ControlFlow::Break(FlowBreak::Jump(position))
}

// jump_to_label: delegates to jump_to_position
pub fn jump_to_label(&self, label: &str) -> DispatchResult {
    match self.label_index.get(label) {
        Some(&pos) => self.jump_to_position(pos),
        None => ControlFlow::Break(FlowBreak::Error(
            KoiError::runtime(format!("Label '{}' not found", label))
        )),
    }
}

// scan_and_jump: returns ProbeNeeded when target not in cache
pub fn scan_and_jump<F>(&mut self, mut strategy: F, offset: i32) -> DispatchResult
where
    F: FnMut(&Command, usize) -> bool + 'static,
{
    if !self.cache_enabled {
        return ControlFlow::Break(FlowBreak::Error(
            KoiError::runtime("Cache must be enabled for scan_and_jump")
        ));
    }

    for pos in self.current_position + 1..self.command_cache.len() {
        if strategy(&self.command_cache[pos], pos) {
            let target = pos as i64 + offset as i64;
            if target < 0 || target as usize >= self.command_cache.len() {
                return ControlFlow::Break(FlowBreak::Error(
                    KoiError::runtime(format!(
                        "Jump target position {} out of bounds (0..{})",
                        target, self.command_cache.len()
                    ))
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

// probe_until: scan existing cache, then ProbeNeeded with offset=0
pub fn probe_until<F>(&mut self, mut strategy: F) -> DispatchResult
where
    F: FnMut(&Command, usize) -> bool + 'static,
{
    if !self.cache_enabled {
        return ControlFlow::Break(FlowBreak::Error(
            KoiError::runtime("Cache must be enabled for probe_until")
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

### `execution_loop` Changes

```rust
fn execution_loop<S>(&mut self, parser: &mut Parser<S>) -> Result<()>
where
    S: TextInputSource,
{
    loop {
        let cmd = if self.cache_enabled && self.current_position < self.command_cache.len() {
            self.command_cache[self.current_position].clone()
        } else {
            match parser.next_command() {
                Ok(Some(cmd)) => {
                    if self.cache_enabled {
                        self.command_cache.push(cmd.clone());
                    }
                    cmd
                }
                Ok(None) => break,
                Err(e) => return Err(KoiError::Parse(*e)),
            }
        };

        self.current_command = Some(cmd.clone());

        match self.dispatch(&cmd) {
            ControlFlow::Continue(()) => {
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

### `KoiError` Changes

Remove from `error.rs`:
- `KoiError::JumpRequest` variant
- `KoiError::jump_request()` constructor
- `KoiError::is_jump_request()` method
- `KoiError::jump_position()` method

### `execute_command_internal` Simplification

The current code must explicitly pass through `JumpRequest`:

```rust
// Before
match env.handle_command(&name, &args, &kwargs, runtime_ref) {
    Ok(()) => return Ok(()),
    Err(KoiError::CommandNotFound { .. }) => continue,
    Err(e) => return Err(e),  // JumpRequest leaks through here!
}
```

With `DispatchResult`, the flow is explicit:

```rust
// After
match env.handle_command(&name, &args, &kwargs, runtime_ref) {
    ControlFlow::Continue(()) => return ControlFlow::Continue(()),
    ControlFlow::Break(FlowBreak::Error(KoiError::CommandNotFound { .. })) => continue,
    other => return other,  // Jump, ProbeNeeded, other errors — all propagated correctly
}
```

## File Change Summary

| File | Changes |
|------|---------|
| `error.rs` | Remove `JumpRequest` variant, `jump_request()`, `is_jump_request()`, `jump_position()` |
| `runtime.rs` | Add `FlowBreak<E>`, `DispatchResult<E>`, `FromResidual` impl; change `CommandHandler` return type; rewrite `execution_loop` with `ProbeNeeded` handling; change `jump_to_position`/`jump_to_label`/`scan_and_jump`/`probe_until` return types |
| `handler.rs` | Update `CommandHandler::handle_command` signature to return `DispatchResult` |

## Non-Goals

- Middleware chain implementation (out of scope)
- Named parameter (`kwargs`) parsing (existing TODO)
- Non-cache mode `current_position` increment fix (separate concern)
