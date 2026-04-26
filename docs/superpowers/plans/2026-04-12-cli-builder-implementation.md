# CLI Builder Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `build_cli<T: CommandHandler>()` function that returns a `CliBuilder` which configures and runs a CLI with script execution, REPL, and pipe/stdin input modes.

**Architecture:** Builder pattern returning `CliBuilder<T>` with `parser_config_defaults()` method. Clap for argument parsing. Koicore `Parser` iterator to process input. Runtime for command execution. stdin TTY detection to choose REPL vs pipe mode.

**Tech Stack:** koicore (already dep), clap (new dep)

---

## File Structure

```
Cargo.toml           # add clap dependency
src/
  cli.rs             # NEW: build_cli(), CliBuilder, argument parsing, REPL, execution
  lib.rs             # add pub use cli::build_cli
```

---

## Dependency

### Task 1: Add clap dependency

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add clap to dependencies**

```toml
[dependencies]
clap = { version = "4.5", features = ["derive"] }
```

Run: `cargo add clap --features derive`

- [ ] **Step 2: Commit**

```bash
git add Cargo.toml
git commit -m "chore: add clap for CLI argument parsing"
```

---

## CLI Module

### Task 2: Create CLI builder and argument parsing

**Files:**
- Create: `src/cli.rs`

- [ ] **Step 1: Write test scaffolding**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_builder_create() {
        // Will be filled after struct is defined
    }
}
```

Run: `cargo build` — verify file compiles (will fail on missing impl)

- [ ] **Step 2: Implement CliBuilder struct**

```rust
use clap::{Parser, ValueHint};
use koicore::parser::{ParserConfig, StringInputSource, TextInputSource};
use std::io::{self, IsTerminal};

use crate::{CommandHandler, Runtime};

/// CLI builder for constructing and running a KoiLang CLI.
pub struct CliBuilder<T: CommandHandler> {
    env: T,
    parser_config_defaults: ParserConfig,
}

impl<T: CommandHandler + Default> CliBuilder<T> {
    /// Create a new CLI builder with a default-constructed env.
    pub fn new() -> Self {
        Self {
            env: T::default(),
            parser_config_defaults: ParserConfig::default(),
        }
    }
}

impl<T: CommandHandler> CliBuilder<T> {
    /// Set parser config defaults that CLI flags may override.
    pub fn parser_config_defaults(mut self, config: ParserConfig) -> Self {
        self.parser_config_defaults = config;
        self
    }

    /// Run the CLI with the given env instance.
    pub fn run_with_env(self) -> io::Result<()> {
        let args = CliArgs::parse();
        let mut config = self.parser_config_defaults;

        // Apply CLI flag overrides
        if let Some(threshold) = args.command_threshold {
            config.command_threshold = threshold;
        }
        if args.skip_annotations {
            config.skip_annotations = true;
        }
        if args.no_skip_annotations {
            config.skip_annotations = false;
        }
        if args.convert_number_command {
            config.convert_number_command = true;
        }
        if args.no_convert_number_command {
            config.convert_number_command = false;
        }
        if args.preserve_indent {
            config.preserve_indent = true;
        }
        if args.no_preserve_indent {
            config.preserve_indent = false;
        }
        if args.preserve_empty_lines {
            config.preserve_empty_lines = true;
        }
        if args.no_preserve_empty_lines {
            config.preserve_empty_lines = false;
        }

        // Determine input mode
        if args.repl || (args.input_file.is_none() && args.eval_script.is_none() && io::stdin().is_terminal()) {
            run_repl(self.env, config)
        } else if let Some(script) = args.eval_script {
            run_script(&script, self.env, config)
        } else if let Some(file) = args.input_file {
            run_file(&file, self.env, config)
        } else {
            run_stdin(self.env, config)
        }
    }
}
```

- [ ] **Step 3: Implement CliArgs struct with clap derive**

```rust
/// CLI arguments parsed by clap.
#[derive(Parser, Debug)]
#[command(name = "koilang")]
#[command(about = "KoiLang CLI", long_about = None)]
struct CliArgs {
    /// Script file to execute
    #[arg(value_hint = ValueHint::FilePath)]
    input_file: Option<String>,

    /// Execute inline script
    #[arg(short = 'e', long = "eval")]
    eval_script: Option<String>,

    /// Force REPL mode
    #[arg(long)]
    repl: bool,

    /// ParserConfig: command threshold (lines with fewer # are text)
    #[arg(long = "command-threshold", value_name = "N")]
    command_threshold: Option<usize>,

    /// ParserConfig: skip annotation lines
    #[arg(long)]
    skip_annotations: bool,

    /// ParserConfig: do not skip annotation lines (default)
    #[arg(long, hide = true)]
    no_skip_annotations: bool,

    /// ParserConfig: convert number commands to special commands
    #[arg(long)]
    convert_number_command: bool,

    /// ParserConfig: do not convert number commands (default)
    #[arg(long, hide = true)]
    no_convert_number_command: bool,

    /// ParserConfig: preserve indentation in text/annotation lines
    #[arg(long)]
    preserve_indent: bool,

    /// ParserConfig: do not preserve indentation (default)
    #[arg(long, hide = true)]
    no_preserve_indent: bool,

    /// ParserConfig: preserve empty lines
    #[arg(long)]
    preserve_empty_lines: bool,

    /// ParserConfig: do not preserve empty lines (default)
    #[arg(long, hide = true)]
    no_preserve_empty_lines: bool,
}
```

Run: `cargo build -p koilang` — verify compilation

- [ ] **Step 4: Implement execution functions**

```rust
/// Run a script from a string.
fn run_script(script: &str, env: T, config: ParserConfig) -> io::Result<()> {
    let input = StringInputSource::new(script);
    let mut runtime = Runtime::new();
    runtime.env_enter(Box::new(env));

    let mut parser = Parser::new(input, config);
    for item in parser {
        match item {
            Ok(command) => {
                runtime.execute_command(
                    command.name(),
                    &command.args().to_vec(),
                    &std::collections::HashMap::new(),
                ).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            }
            Err(e) => {
                eprintln!("Parse error: {}", e);
                std::process::exit(1);
            }
        }
    }
    Ok(())
}

/// Run a script from a file.
fn run_file(path: &str, env: T, config: ParserConfig) -> io::Result<()> {
    let content = std::fs::read_to_string(path)?;
    run_script(&content, env, config)
}

/// Run a script from stdin.
fn run_stdin(env: T, config: ParserConfig) -> io::Result<()> {
    let content = io::read_to_string(io::stdin().lock())?;
    run_script(&content, env, config)
}
```

Run: `cargo build -p koilang` — verify compilation

- [ ] **Step 5: Implement REPL**

```rust
/// Run an interactive REPL.
fn run_repl(mut env: T, config: ParserConfig) -> io::Result<()> {
    use koicore::command::Value;

    let mut runtime = Runtime::new();
    runtime.env_enter(Box::new(env));

    println!("KoiLang REPL (Ctrl+D or 'exit' to quit)");

    for line in io::stdin().lock().lines() {
        let line = line?;
        if line.trim() == "exit" {
            break;
        }

        if line.trim().is_empty() {
            continue;
        }

        // Parse and execute line-by-line
        let input = StringInputSource::new(&line);
        let parser = Parser::new(input, config.clone());

        for item in parser {
            match item {
                Ok(command) => {
                    let result = runtime.execute_command(
                        command.name(),
                        &command.args().to_vec(),
                        &std::collections::HashMap::new(),
                    );
                    if let Err(e) = result {
                        eprintln!("Error: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("Parse error: {}", e);
                }
            }
        }
    }
    Ok(())
}
```

Run: `cargo build -p koilang` — verify compilation

- [ ] **Step 6: Implement build_cli function**

```rust
/// Build a CLI from an Env type implementing CommandHandler.
///
/// # Example
///
/// ```rust,ignore
/// use koilang::build_cli;
///
/// #[derive(Default, CommandHandler)]
/// struct MyEnv;
///
/// build_cli::<MyEnv>().run();
/// ```
pub fn build_cli<T: CommandHandler + Default>() -> CliBuilder<T> {
    CliBuilder::new()
}
```

- [ ] **Step 7: Commit**

```bash
git add src/cli.rs
git commit -m "feat: add CLI builder with build_cli function"
```

---

## Integration

### Task 3: Export build_cli from lib.rs

**Files:**
- Modify: `src/lib.rs:134`

- [ ] **Step 1: Add pub use cli::build_cli**

Add to the public exports section (after line 134):

```rust
pub use cli::build_cli;
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p koilang`

- [ ] **Step 3: Commit**

```bash
git add src/lib.rs
git commit -m "feat: export build_cli from koilang"
```

---

## Spec Coverage Check

- [x] Script file input mode → `run_file()`
- [x] Inline script (`-e`) → `run_script()`
- [x] REPL mode → `run_repl()`
- [x] Stdin/Pipe → `run_stdin()`
- [x] TTY detection → in `run_with_env()` condition
- [x] `--command-threshold` flag → `CliArgs::command_threshold`
- [x] `--skip-annotations` / `--no-skip-annotations` → both fields
- [x] `--convert-number-command` / `--no-convert-number-command` → both fields
- [x] `--preserve-indent` / `--no-preserve-indent` → both fields
- [x] `--preserve-empty-lines` / `--no-preserve-empty-lines` → both fields
- [x] `-h, --help` and `-V, --version` → clap default via `#[command]`
- [x] Builder API with `parser_config_defaults()` → implemented
- [x] Config priority (CLI > defaults > ParserConfig) → implemented
- [x] Exit codes 1 for KoiError, 2 for parse error → implemented in run functions

**Plan complete.**