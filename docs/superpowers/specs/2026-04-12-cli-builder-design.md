# CLI Builder Design

## Context

Add a `build_cli<T>()` function/macro to koilang that constructs a CLI main function from an Env struct implementing `CommandHandler`. The CLI should support script file execution, REPL mode, and pipe/stdin input, with configurable ParserConfig via CLI flags.

## Design

### Input Modes (mutually exclusive)

| Mode | Usage |
|------|-------|
| Script file | `cli script.koi` |
| Inline script | `cli -e '#greet "Alice"'` |
| REPL | `cli` (no args) or `cli --repl` |
| Stdin/Pipe | `cat script.koi \| cli` |

When stdin is a TTY and no args provided → REPL.
When stdin is not a TTY → read from stdin.

### ParserConfig CLI Flags

| Flag | Type | Default |
|------|------|---------|
| `--command-threshold <N>` | usize | 1 |
| `--skip-annotations` | flag | false |
| `--no-skip-annotations` | | |
| `--convert-number-command` | flag | true |
| `--no-convert-number-command` | | |
| `--preserve-indent` | flag | false |
| `--no-preserve-indent` | | |
| `--preserve-empty-lines` | flag | false |
| `--no-preserve-empty-lines` | | |
| `-h, --help` | | |
| `-V, --version` | | |

Flag negation via `--no-*` variant.

### Builder API

```rust
use koilang::build_cli;
use koicore::parser::ParserConfig;

build_cli::<MyEnv>()
    .parser_config_defaults(ParserConfig {
        command_threshold: 2,
        ..Default::default()
    })
    .run();
```

### Config Priority

1. CLI flags override defaults
2. `.parser_config_defaults()` overrides ParserConfig defaults

### REPL Behavior

- Reads line-by-line from stdin
- `exit` command or Ctrl+D exits
- Ctrl+C interrupts current execution
- Each line is executed immediately

### Error Handling

- Exit code 0 on success
- Exit code 1 on KoiError
- Exit code 2 on CLI argument parse error

## Module Structure

```
src/
  cli.rs           # build_cli(), CliBuilder, main CLI logic
  lib.rs           # re-export build_cli
```

## Implementation Notes

- Use `clap` or `argg` for argument parsing (std-friendly, minimal deps)
- Use `Koicore::Parser` with `TextInputSource` for script execution
- REPL uses `Koicore::Parser` in incremental mode or line-by-line execution
