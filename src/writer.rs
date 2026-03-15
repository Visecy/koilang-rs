//! Writer for programmatic KoiLang generation.
//!
//! This module provides the [`Writer`] struct which allows programmatic
//! generation of KoiLang scripts with support for formatting options and
//! indentation management.
//!
//! # Examples
//!
//! ```rust,ignore
//! use koilang_rs::Writer;
//! use koicore::writer::WriterConfig;
//!
//! let mut buffer = Vec::new();
//! let mut writer = Writer::new(&mut buffer, None).unwrap();
//!
//! writer.command("hello", &["World".into()], &HashMap::new()).unwrap();
//! writer.text("Some text content").unwrap();
//! writer.close().unwrap();
//! ```

use crate::error::{KoiError, Result};
use koicore::command::{Command, Parameter, Value};
use koicore::writer::{FormatterOptions, Writer as CoreWriter, WriterConfig};
use std::collections::{HashMap, HashSet};
use std::io::Write;

/// Writer for generating KoiLang output.
///
/// The writer provides methods to generate KoiLang commands, text content,
/// and annotations with support for formatting options and indentation.
///
/// # Examples
///
/// ```rust,ignore
/// use koilang_rs::Writer;
///
/// let mut buffer = Vec::new();
/// let mut writer = Writer::new(&mut buffer, None).unwrap();
///
/// writer.command("character", &["Alice".into(), "Hello!".into()], &HashMap::new()).unwrap();
/// writer.text("This is narrative text.").unwrap();
/// writer.close().unwrap();
///
/// let output = String::from_utf8(buffer).unwrap();
/// println!("{}", output);
/// ```
pub struct Writer<W: Write> {
    /// The underlying koicore writer.
    core_writer: CoreWriter<W>,

    /// Stack of temporary formatting options.
    options_stack: Vec<(FormatterOptions, Option<HashSet<String>>)>,
}

/// Proxy for writing with temporary formatting options.
///
/// This allows applying formatting options to a specific set of commands
/// or all commands written through the proxy.
///
/// # Examples
///
/// ```rust,ignore
/// use koilang_rs::Writer;
/// use koicore::writer::FormatterOptions;
///
/// let mut buffer = Vec::new();
/// let mut writer = Writer::new(&mut buffer, None).unwrap();
///
/// let opts = FormatterOptions::default();
/// writer.with_options(opts, None).command("test", &[], &HashMap::new()).unwrap();
/// ```
pub struct OptionsProxy<'a, W: Write> {
    writer: &'a mut Writer<W>,
    options: FormatterOptions,
    targets: Option<HashSet<String>>,
}

impl<W: Write> Writer<W> {
    /// Create a new writer.
    ///
    /// # Arguments
    ///
    /// * `writer` - The output writer
    /// * `config` - Optional writer configuration
    ///
    /// # Returns
    ///
    /// Returns `Ok(Writer)` on success, or an error if creation fails.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use koilang_rs::Writer;
    ///
    /// let mut buffer = Vec::new();
    /// let mut writer = Writer::new(&mut buffer, None).unwrap();
    /// ```
    pub fn new(writer: W, config: Option<WriterConfig>) -> Result<Self> {
        let core_config = config.unwrap_or_default();
        let core_writer = CoreWriter::new(writer, core_config);

        Ok(Self {
            core_writer,
            options_stack: Vec::new(),
        })
    }

    /// Flush the writer and ensure all output is written.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if flushing fails.
    pub fn flush(&mut self) -> Result<()> {
        // Note: koicore::Writer doesn't expose the underlying writer
        // This is a no-op for now
        Ok(())
    }

    /// Write a newline.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if writing fails.
    pub fn newline(&mut self) -> Result<()> {
        self.core_writer.newline().map_err(|e| {
            KoiError::runtime(format!("Failed to write newline: {}", e), 0)
        })
    }

    /// Increase the indentation level.
    pub fn inc_indent(&mut self) {
        self.core_writer.inc_indent();
    }

    /// Decrease the indentation level.
    pub fn dec_indent(&mut self) {
        self.core_writer.dec_indent();
    }

    /// Execute a function with increased indentation.
    ///
    /// The indentation is increased before calling the function and
    /// decreased after the function returns.
    ///
    /// # Arguments
    ///
    /// * `f` - The function to execute
    ///
    /// # Returns
    ///
    /// Returns the result of the function.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use koilang_rs::Writer;
    ///
    /// let mut buffer = Vec::new();
    /// let mut writer = Writer::new(&mut buffer, None).unwrap();
    ///
    /// writer.indent_scope(|w| {
    ///     w.command("inner", &[], &HashMap::new()).unwrap();
    /// });
    /// ```
    pub fn indent_scope<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.inc_indent();
        let result = f(self);
        self.dec_indent();
        result
    }

    /// Create a proxy for writing with temporary formatting options.
    ///
    /// # Arguments
    ///
    /// * `options` - The formatting options to apply
    /// * `target_commands` - Optional list of command names to apply options to
    ///
    /// # Returns
    ///
    /// Returns an `OptionsProxy` for writing with the specified options.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use koilang_rs::Writer;
    /// use koicore::writer::FormatterOptions;
    ///
    /// let mut buffer = Vec::new();
    /// let mut writer = Writer::new(&mut buffer, None).unwrap();
    ///
    /// let opts = FormatterOptions::default();
    /// writer.with_options(opts, Some(vec!["special".to_string()]))
    ///     .command("special", &[], &HashMap::new())
    ///     .unwrap();
    /// ```
    pub fn with_options(
        &mut self,
        options: FormatterOptions,
        target_commands: Option<Vec<String>>,
    ) -> OptionsProxy<'_, W> {
        let targets = target_commands.map(|v| v.into_iter().collect());
        OptionsProxy {
            writer: self,
            options,
            targets,
        }
    }

    /// Write a command.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to write
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if writing fails.
    pub fn write_command(&mut self, command: &Command) -> Result<()> {
        // Check if there are active options for this command
        let active_options = self.options_stack.iter().rev().find_map(|(opts, targets)| {
            if targets.is_none() || targets.as_ref().unwrap().contains(command.name()) {
                Some(opts.clone())
            } else {
                None
            }
        });

        if let Some(opts) = active_options {
            self.core_writer
                .write_command_with_options(command, Some(&opts), None)
                .map_err(|e| {
                    KoiError::runtime(format!("Failed to write command: {}", e), 0)
                })?;
        } else {
            self.core_writer.write_command(command).map_err(|e| {
                KoiError::runtime(format!("Failed to write command: {}", e), 0)
            })?;
        }

        Ok(())
    }

    /// Write a command with name and arguments.
    ///
    /// # Arguments
    ///
    /// * `name` - The command name
    /// * `args` - Positional arguments
    /// * `kwargs` - Named arguments
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if writing fails.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use koilang_rs::Writer;
    ///
    /// let mut buffer = Vec::new();
    /// let mut writer = Writer::new(&mut buffer, None).unwrap();
    ///
    /// writer.command("greet", &["Alice".into()], &HashMap::new()).unwrap();
    /// ```
    pub fn command(
        &mut self,
        name: &str,
        args: &[Value],
        kwargs: &HashMap<String, Value>,
    ) -> Result<()> {
        let params = build_params(args, kwargs);
        let cmd = Command::new(name, params);
        self.write_command(&cmd)
    }

    /// Write text content.
    ///
    /// # Arguments
    ///
    /// * `content` - The text content
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if writing fails.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use koilang_rs::Writer;
    ///
    /// let mut buffer = Vec::new();
    /// let mut writer = Writer::new(&mut buffer, None).unwrap();
    ///
    /// writer.text("This is narrative text.").unwrap();
    /// ```
    pub fn text(&mut self, content: &str) -> Result<()> {
        let cmd = Command::new_text(content);
        self.write_command(&cmd)
    }

    /// Write an annotation.
    ///
    /// # Arguments
    ///
    /// * `content` - The annotation content
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if writing fails.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use koilang_rs::Writer;
    ///
    /// let mut buffer = Vec::new();
    /// let mut writer = Writer::new(&mut buffer, None).unwrap();
    ///
    /// writer.annotation("This is an annotation.").unwrap();
    /// ```
    pub fn annotation(&mut self, content: &str) -> Result<()> {
        let cmd = Command::new_annotation(content);
        self.write_command(&cmd)
    }
}

impl<'a, W: Write> OptionsProxy<'a, W> {
    /// Write a command with the temporary formatting options.
    ///
    /// # Arguments
    ///
    /// * `name` - The command name
    /// * `args` - Positional arguments
    /// * `kwargs` - Named arguments
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if writing fails.
    pub fn command(
        &mut self,
        name: &str,
        args: &[Value],
        kwargs: &HashMap<String, Value>,
    ) -> Result<()> {
        if self.targets.is_none() || self.targets.as_ref().unwrap().contains(name) {
            let params = build_params(args, kwargs);
            let cmd = Command::new(name, params);
            self.writer
                .core_writer
                .write_command_with_options(&cmd, Some(&self.options), None)
                .map_err(|e| {
                    KoiError::runtime(format!("Failed to write command: {}", e), 0)
                })?;
        } else {
            self.writer.command(name, args, kwargs)?;
        }
        Ok(())
    }

    /// Write text content with the temporary formatting options.
    ///
    /// # Arguments
    ///
    /// * `content` - The text content
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if writing fails.
    pub fn text(&mut self, content: &str) -> Result<()> {
        let cmd = Command::new_text(content);
        self.writer
            .core_writer
            .write_command_with_options(&cmd, Some(&self.options), None)
            .map_err(|e| {
                KoiError::runtime(format!("Failed to write text: {}", e), 0)
            })
    }

    /// Write an annotation with the temporary formatting options.
    ///
    /// # Arguments
    ///
    /// * `content` - The annotation content
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if writing fails.
    pub fn annotation(&mut self, content: &str) -> Result<()> {
        let cmd = Command::new_annotation(content);
        self.writer
            .core_writer
            .write_command_with_options(&cmd, Some(&self.options), None)
            .map_err(|e| {
                KoiError::runtime(format!("Failed to write annotation: {}", e), 0)
            })
    }
}

impl<W: Write> Drop for Writer<W> {
    fn drop(&mut self) {
        // Attempt to flush the writer on drop, ignoring errors
        let _ = self.flush();
    }
}

/// Build parameters from positional and named arguments.
fn build_params(args: &[Value], kwargs: &HashMap<String, Value>) -> Vec<Parameter> {
    let mut params: Vec<Parameter> = args.iter().cloned().map(Parameter::from).collect();

    for (k, v) in kwargs {
        params.push(Parameter::from((k.as_str(), v.clone())));
    }

    params
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_writer_new() {
        let mut buffer = Vec::new();
        let writer = Writer::new(&mut buffer, None);
        assert!(writer.is_ok());
    }

}
