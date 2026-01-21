//! External diff renderer support.
//!
//! This module provides integration with external diff rendering tools like delta,
//! diff-so-fancy, etc. It executes the external renderer and converts ANSI output
//! to ratatui Text for display.

use ansi_to_tui::IntoText;
use ratatui::text::Text;
use std::process::{Command, Stdio};

use crate::config::DiffConfig;

/// Result of rendering a diff patch
pub enum RenderResult<'a> {
    /// Successfully rendered with external renderer
    Rendered(Text<'a>),
    /// External renderer not available, use fallback
    Fallback,
}

/// Render a patch using the configured external renderer.
///
/// Returns `RenderResult::Rendered` if the external renderer succeeds,
/// or `RenderResult::Fallback` if the renderer is not available or fails.
pub fn render_with_external<'a>(patch: &str, config: &DiffConfig) -> RenderResult<'a> {
    // "builtin" means use the internal renderer
    if config.renderer == "builtin" || config.renderer.is_empty() {
        return RenderResult::Fallback;
    }

    // Build command with renderer-specific arguments
    let output = match build_renderer_command(&config.renderer, config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            // Write patch to stdin
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(patch.as_bytes());
            }

            match child.wait_with_output() {
                Ok(output) if output.status.success() => output.stdout,
                _ => return RenderResult::Fallback,
            }
        }
        Err(_) => return RenderResult::Fallback,
    };

    // Convert ANSI output to ratatui Text
    match output.into_text() {
        Ok(text) => RenderResult::Rendered(text),
        Err(_) => RenderResult::Fallback,
    }
}

/// Build a Command for the specified renderer with appropriate arguments.
fn build_renderer_command(renderer: &str, config: &DiffConfig) -> Command {
    let mut cmd = Command::new(renderer);

    match renderer {
        "delta" => {
            // delta-specific options
            if config.side_by_side {
                cmd.arg("--side-by-side");
            }
            if config.line_numbers {
                cmd.arg("--line-numbers");
            }
            // Force color output even when not a TTY
            cmd.arg("--color-only");
        }
        "diff-so-fancy" => {
            // diff-so-fancy doesn't have many options, just pass through
        }
        "bat" => {
            // bat can be used as a diff viewer
            cmd.args(["--language", "diff", "--color", "always", "--style", "plain"]);
        }
        _ => {
            // For unknown renderers, just try to run them as-is
        }
    }

    cmd
}

/// Check if an external renderer is available on the system.
#[allow(dead_code)]
pub fn is_renderer_available(renderer: &str) -> bool {
    if renderer == "builtin" || renderer.is_empty() {
        return true;
    }

    Command::new(renderer)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
