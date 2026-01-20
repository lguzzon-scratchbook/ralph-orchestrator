//! Stream handler trait and implementations for processing Claude stream events.
//!
//! The `StreamHandler` trait abstracts over how stream events are displayed,
//! allowing for different output strategies (console, quiet, TUI, etc.).

use ansi_to_tui::IntoText;
use crossterm::{
    QueueableCommand,
    style::{self, Color},
};
use ratatui::{
    style::{Color as RatatuiColor, Style},
    text::{Line, Span},
};
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use termimad::MadSkin;

/// Detects if text contains ANSI escape sequences.
///
/// Checks for the common ANSI escape sequence prefix `\x1b[` (ESC + `[`)
/// which is used for colors, formatting, and cursor control.
#[inline]
pub(crate) fn contains_ansi(text: &str) -> bool {
    text.contains("\x1b[")
}

/// Session completion result data.
#[derive(Debug, Clone)]
pub struct SessionResult {
    pub duration_ms: u64,
    pub total_cost_usd: f64,
    pub num_turns: u32,
    pub is_error: bool,
}

/// Renders streaming output with colors and markdown.
pub struct PrettyStreamHandler {
    stdout: io::Stdout,
    verbose: bool,
    /// Buffer for accumulating text before markdown rendering
    text_buffer: String,
    /// Skin for markdown rendering
    skin: MadSkin,
}

impl PrettyStreamHandler {
    /// Creates a new pretty handler.
    pub fn new(verbose: bool) -> Self {
        Self {
            stdout: io::stdout(),
            verbose,
            text_buffer: String::new(),
            skin: MadSkin::default(),
        }
    }

    /// Flush buffered text as rendered markdown.
    fn flush_text_buffer(&mut self) {
        if self.text_buffer.is_empty() {
            return;
        }
        // Render markdown to string, then write
        let rendered = self.skin.term_text(&self.text_buffer);
        let _ = self.stdout.write(rendered.to_string().as_bytes());
        let _ = self.stdout.flush();
        self.text_buffer.clear();
    }
}

impl StreamHandler for PrettyStreamHandler {
    fn on_text(&mut self, text: &str) {
        // Buffer text for markdown rendering
        self.text_buffer.push_str(text);
    }

    fn on_tool_result(&mut self, _id: &str, output: &str) {
        if self.verbose {
            let _ = self
                .stdout
                .queue(style::SetForegroundColor(Color::DarkGrey));
            let _ = self
                .stdout
                .write(format!(" \u{2713} {}\n", truncate(output, 200)).as_bytes());
            let _ = self.stdout.queue(style::ResetColor);
            let _ = self.stdout.flush();
        }
    }

    fn on_error(&mut self, error: &str) {
        let _ = self.stdout.queue(style::SetForegroundColor(Color::Red));
        let _ = self
            .stdout
            .write(format!("\n\u{2717} Error: {}\n", error).as_bytes());
        let _ = self.stdout.queue(style::ResetColor);
        let _ = self.stdout.flush();
    }

    fn on_complete(&mut self, result: &SessionResult) {
        // Flush any remaining buffered text
        self.flush_text_buffer();

        let _ = self.stdout.write(b"\n");
        let color = if result.is_error {
            Color::Red
        } else {
            Color::Green
        };
        let _ = self.stdout.queue(style::SetForegroundColor(color));
        let _ = self.stdout.write(
            format!(
                "Duration: {}ms | Cost: ${:.4} | Turns: {}\n",
                result.duration_ms, result.total_cost_usd, result.num_turns
            )
            .as_bytes(),
        );
        let _ = self.stdout.queue(style::ResetColor);
        let _ = self.stdout.flush();
    }

    fn on_tool_call(&mut self, name: &str, _id: &str, input: &serde_json::Value) {
        // Flush any buffered text before showing tool call
        self.flush_text_buffer();

        // ⚙️ [ToolName]
        let _ = self.stdout.queue(style::SetForegroundColor(Color::Blue));
        let _ = self.stdout.write(format!("\u{2699} [{}]", name).as_bytes());

        if let Some(summary) = format_tool_summary(name, input) {
            let _ = self
                .stdout
                .queue(style::SetForegroundColor(Color::DarkGrey));
            let _ = self.stdout.write(format!(" {}\n", summary).as_bytes());
        } else {
            let _ = self.stdout.write(b"\n");
        }
        let _ = self.stdout.queue(style::ResetColor);
        let _ = self.stdout.flush();
    }
}

/// Handler for streaming output events from Claude.
///
/// Implementors receive events as Claude processes and can format/display
/// them in various ways (console output, TUI updates, logging, etc.).
pub trait StreamHandler: Send {
    /// Called when Claude emits text.
    fn on_text(&mut self, text: &str);

    /// Called when Claude invokes a tool.
    ///
    /// # Arguments
    /// * `name` - Tool name (e.g., "Read", "Bash", "Grep")
    /// * `id` - Unique tool invocation ID
    /// * `input` - Tool input parameters as JSON (file paths, commands, patterns, etc.)
    fn on_tool_call(&mut self, name: &str, id: &str, input: &serde_json::Value);

    /// Called when a tool returns results (verbose only).
    fn on_tool_result(&mut self, id: &str, output: &str);

    /// Called when an error occurs.
    fn on_error(&mut self, error: &str);

    /// Called when session completes (verbose only).
    fn on_complete(&mut self, result: &SessionResult);
}

/// Writes streaming output to stdout/stderr.
///
/// In normal mode, displays assistant text and tool invocations.
/// In verbose mode, also displays tool results and session summary.
pub struct ConsoleStreamHandler {
    verbose: bool,
    stdout: io::Stdout,
    stderr: io::Stderr,
}

impl ConsoleStreamHandler {
    /// Creates a new console handler.
    ///
    /// # Arguments
    /// * `verbose` - If true, shows tool results and session summary.
    pub fn new(verbose: bool) -> Self {
        Self {
            verbose,
            stdout: io::stdout(),
            stderr: io::stderr(),
        }
    }
}

impl StreamHandler for ConsoleStreamHandler {
    fn on_text(&mut self, text: &str) {
        let _ = writeln!(self.stdout, "Claude: {}", text);
    }

    fn on_tool_call(&mut self, name: &str, _id: &str, input: &serde_json::Value) {
        match format_tool_summary(name, input) {
            Some(summary) => {
                let _ = writeln!(self.stdout, "[Tool] {}: {}", name, summary);
            }
            None => {
                let _ = writeln!(self.stdout, "[Tool] {}", name);
            }
        }
    }

    fn on_tool_result(&mut self, _id: &str, output: &str) {
        if self.verbose {
            let _ = writeln!(self.stdout, "[Result] {}", truncate(output, 200));
        }
    }

    fn on_error(&mut self, error: &str) {
        // Write to both stdout (inline) and stderr (for separation)
        let _ = writeln!(self.stdout, "[Error] {}", error);
        let _ = writeln!(self.stderr, "[Error] {}", error);
    }

    fn on_complete(&mut self, result: &SessionResult) {
        if self.verbose {
            let _ = writeln!(
                self.stdout,
                "\n--- Session Complete ---\nDuration: {}ms | Cost: ${:.4} | Turns: {}",
                result.duration_ms, result.total_cost_usd, result.num_turns
            );
        }
    }
}

/// Suppresses all streaming output (for CI/silent mode).
pub struct QuietStreamHandler;

impl StreamHandler for QuietStreamHandler {
    fn on_text(&mut self, _: &str) {}
    fn on_tool_call(&mut self, _: &str, _: &str, _: &serde_json::Value) {}
    fn on_tool_result(&mut self, _: &str, _: &str) {}
    fn on_error(&mut self, _: &str) {}
    fn on_complete(&mut self, _: &SessionResult) {}
}

/// Converts text to styled ratatui Lines, handling both ANSI and markdown.
///
/// When text contains ANSI escape sequences (e.g., from CLI tools like Kiro),
/// uses `ansi_to_tui` to preserve colors and formatting. Otherwise, uses
/// `tui_markdown` to parse markdown syntax into styled Lines.
fn text_to_lines(text: &str) -> Vec<Line<'static>> {
    if text.is_empty() {
        return Vec::new();
    }

    // Check if text contains ANSI escape sequences
    if contains_ansi(text) {
        // Parse ANSI codes to ratatui Text
        match text.into_text() {
            Ok(parsed_text) => {
                // Convert Text to owned Lines
                parsed_text
                    .lines
                    .into_iter()
                    .map(|line| {
                        let owned_spans: Vec<Span<'static>> = line
                            .spans
                            .into_iter()
                            .map(|span| Span::styled(span.content.into_owned(), span.style))
                            .collect();
                        Line::from(owned_spans)
                    })
                    .collect()
            }
            Err(_) => {
                // Fallback: treat as plain text if ANSI parsing fails
                vec![Line::from(text.to_string())]
            }
        }
    } else {
        // No ANSI codes - use markdown parsing
        markdown_to_lines(text)
    }
}

/// Converts markdown text to styled ratatui Lines.
///
/// Uses `tui_markdown` to parse markdown and produce properly styled
/// Lines with bold, italic, code, and header formatting.
fn markdown_to_lines(text: &str) -> Vec<Line<'static>> {
    if text.is_empty() {
        return Vec::new();
    }

    // Parse markdown using tui-markdown
    let parsed_text = tui_markdown::from_str(text);

    // Convert Text to owned Lines
    parsed_text
        .lines
        .into_iter()
        .map(|line| {
            // Convert each span to owned
            let owned_spans: Vec<Span<'static>> = line
                .spans
                .into_iter()
                .map(|span| Span::styled(span.content.into_owned(), span.style))
                .collect();
            Line::from(owned_spans)
        })
        .collect()
}

/// Renders streaming output as ratatui Lines for TUI display.
///
/// This handler produces output visually equivalent to `PrettyStreamHandler`
/// but stores it as `Line<'static>` objects for rendering in a ratatui-based TUI.
///
/// Text content is parsed as markdown, producing styled output for bold, italic,
/// code, headers, etc. Tool calls and errors bypass markdown parsing to preserve
/// their explicit styling.
pub struct TuiStreamHandler {
    /// Buffer for accumulating markdown text
    markdown_buffer: String,
    /// Verbose mode (show tool results)
    verbose: bool,
    /// Collected output lines (markdown lines + tool/error lines)
    lines: Arc<Mutex<Vec<Line<'static>>>>,
    /// Lines that are not markdown (tool calls, errors, etc.)
    /// These are appended after markdown lines on each re-parse
    non_markdown_lines: Vec<Line<'static>>,
    /// Maximum line length before truncation
    max_line_length: usize,
}

impl TuiStreamHandler {
    /// Creates a new TUI handler.
    ///
    /// # Arguments
    /// * `verbose` - If true, shows tool results and session summary.
    pub fn new(verbose: bool) -> Self {
        Self {
            markdown_buffer: String::new(),
            verbose,
            lines: Arc::new(Mutex::new(Vec::new())),
            non_markdown_lines: Vec::new(),
            max_line_length: 200,
        }
    }

    /// Creates a TUI handler with shared lines storage.
    ///
    /// Use this to share output lines with the TUI application.
    pub fn with_lines(verbose: bool, lines: Arc<Mutex<Vec<Line<'static>>>>) -> Self {
        Self {
            markdown_buffer: String::new(),
            verbose,
            lines,
            non_markdown_lines: Vec::new(),
            max_line_length: 200,
        }
    }

    /// Returns a clone of the collected lines.
    pub fn get_lines(&self) -> Vec<Line<'static>> {
        self.lines.lock().unwrap().clone()
    }

    /// Flushes any buffered markdown text by re-parsing and updating lines.
    pub fn flush_text_buffer(&mut self) {
        self.update_lines();
    }

    /// Re-parses the text buffer and updates the shared lines.
    ///
    /// This replaces all lines with: parsed text lines + non-text lines.
    /// Text is parsed as ANSI if escape codes are detected, otherwise as markdown.
    fn update_lines(&mut self) {
        let mut all_lines = text_to_lines(&self.markdown_buffer);

        // Truncate long lines
        for line in &mut all_lines {
            let total_len: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
            if total_len > self.max_line_length {
                // Truncate the line content
                let mut remaining = self.max_line_length;
                let mut new_spans = Vec::new();
                for span in line.spans.drain(..) {
                    let char_count = span.content.chars().count();
                    if remaining == 0 {
                        break;
                    } else if char_count <= remaining {
                        remaining -= char_count;
                        new_spans.push(span);
                    } else {
                        // Truncate this span
                        let truncated: String = span.content.chars().take(remaining).collect();
                        new_spans.push(Span::styled(truncated + "...", span.style));
                        break;
                    }
                }
                line.spans = new_spans;
            }
        }

        // Append non-markdown lines (tool calls, errors, etc.)
        all_lines.extend(self.non_markdown_lines.clone());

        // Update shared lines
        *self.lines.lock().unwrap() = all_lines;
    }

    /// Adds a non-markdown line (tool call, error, etc.) and updates display.
    fn add_non_markdown_line(&mut self, line: Line<'static>) {
        self.non_markdown_lines.push(line);
        self.update_lines();
    }
}

impl StreamHandler for TuiStreamHandler {
    fn on_text(&mut self, text: &str) {
        // Append text to markdown buffer
        self.markdown_buffer.push_str(text);

        // Re-parse and update lines on each text chunk
        // This handles streaming markdown correctly
        self.update_lines();
    }

    fn on_tool_call(&mut self, name: &str, _id: &str, input: &serde_json::Value) {
        // Build spans: ⚙️ [ToolName] summary
        let mut spans = vec![Span::styled(
            format!("\u{2699} [{}]", name),
            Style::default().fg(RatatuiColor::Blue),
        )];

        if let Some(summary) = format_tool_summary(name, input) {
            spans.push(Span::styled(
                format!(" {}", summary),
                Style::default().fg(RatatuiColor::DarkGray),
            ));
        }

        self.add_non_markdown_line(Line::from(spans));
    }

    fn on_tool_result(&mut self, _id: &str, output: &str) {
        if self.verbose {
            let line = Line::from(Span::styled(
                format!(" \u{2713} {}", truncate(output, 200)),
                Style::default().fg(RatatuiColor::DarkGray),
            ));
            self.add_non_markdown_line(line);
        }
    }

    fn on_error(&mut self, error: &str) {
        let line = Line::from(Span::styled(
            format!("\n\u{2717} Error: {}", error),
            Style::default().fg(RatatuiColor::Red),
        ));
        self.add_non_markdown_line(line);
    }

    fn on_complete(&mut self, result: &SessionResult) {
        // Flush any remaining buffered text
        self.flush_text_buffer();

        // Add blank line
        self.add_non_markdown_line(Line::from(""));

        // Add summary with color based on error status
        let color = if result.is_error {
            RatatuiColor::Red
        } else {
            RatatuiColor::Green
        };
        let summary = format!(
            "Duration: {}ms | Cost: ${:.4} | Turns: {}",
            result.duration_ms, result.total_cost_usd, result.num_turns
        );
        let line = Line::from(Span::styled(summary, Style::default().fg(color)));
        self.add_non_markdown_line(line);
    }
}

/// Extracts the most relevant field from tool input for display.
///
/// Returns a human-readable summary (file path, command, pattern, etc.) based on the tool type.
/// Returns `None` for unknown tools or if the expected field is missing.
fn format_tool_summary(name: &str, input: &serde_json::Value) -> Option<String> {
    match name {
        "Read" | "Edit" | "Write" => input.get("file_path")?.as_str().map(|s| s.to_string()),
        "Bash" => {
            let cmd = input.get("command")?.as_str()?;
            Some(truncate(cmd, 60))
        }
        "Grep" => input.get("pattern")?.as_str().map(|s| s.to_string()),
        "Glob" => input.get("pattern")?.as_str().map(|s| s.to_string()),
        "Task" => input.get("description")?.as_str().map(|s| s.to_string()),
        "WebFetch" => input.get("url")?.as_str().map(|s| s.to_string()),
        "WebSearch" => input.get("query")?.as_str().map(|s| s.to_string()),
        "LSP" => {
            let op = input.get("operation")?.as_str()?;
            let file = input.get("filePath")?.as_str()?;
            Some(format!("{} @ {}", op, file))
        }
        "NotebookEdit" => input.get("notebook_path")?.as_str().map(|s| s.to_string()),
        "TodoWrite" => Some("updating todo list".to_string()),
        _ => None,
    }
}

/// Truncates a string to approximately `max_len` characters, adding "..." if truncated.
///
/// Uses `char_indices` to find a valid UTF-8 boundary, ensuring we never slice
/// in the middle of a multi-byte character.
fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        // Find the byte index of the max_len-th character
        let byte_idx = s
            .char_indices()
            .nth(max_len)
            .map(|(idx, _)| idx)
            .unwrap_or(s.len());
        format!("{}...", &s[..byte_idx])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_console_handler_verbose_shows_results() {
        let mut handler = ConsoleStreamHandler::new(true);
        let bash_input = json!({"command": "ls -la"});

        // These calls should not panic
        handler.on_text("Hello");
        handler.on_tool_call("Bash", "tool_1", &bash_input);
        handler.on_tool_result("tool_1", "output");
        handler.on_complete(&SessionResult {
            duration_ms: 1000,
            total_cost_usd: 0.01,
            num_turns: 1,
            is_error: false,
        });
    }

    #[test]
    fn test_console_handler_normal_skips_results() {
        let mut handler = ConsoleStreamHandler::new(false);
        let read_input = json!({"file_path": "src/main.rs"});

        // These should not show tool results
        handler.on_text("Hello");
        handler.on_tool_call("Read", "tool_1", &read_input);
        handler.on_tool_result("tool_1", "output"); // Should be silent
        handler.on_complete(&SessionResult {
            duration_ms: 1000,
            total_cost_usd: 0.01,
            num_turns: 1,
            is_error: false,
        }); // Should be silent
    }

    #[test]
    fn test_quiet_handler_is_silent() {
        let mut handler = QuietStreamHandler;
        let empty_input = json!({});

        // All of these should be no-ops
        handler.on_text("Hello");
        handler.on_tool_call("Read", "tool_1", &empty_input);
        handler.on_tool_result("tool_1", "output");
        handler.on_error("Something went wrong");
        handler.on_complete(&SessionResult {
            duration_ms: 1000,
            total_cost_usd: 0.01,
            num_turns: 1,
            is_error: false,
        });
    }

    #[test]
    fn test_truncate_helper() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("this is a long string", 10), "this is a ...");
    }

    #[test]
    fn test_truncate_utf8_boundaries() {
        // Arrow → is 3 bytes (U+2192: E2 86 92)
        let with_arrows = "→→→→→→→→→→";
        // Should truncate at character boundary, not byte boundary
        assert_eq!(truncate(with_arrows, 5), "→→→→→...");

        // Mixed ASCII and multi-byte
        let mixed = "a→b→c→d→e";
        assert_eq!(truncate(mixed, 5), "a→b→c...");

        // Emoji (4-byte characters)
        let emoji = "🎉🎊🎁🎈🎄";
        assert_eq!(truncate(emoji, 3), "🎉🎊🎁...");
    }

    #[test]
    fn test_format_tool_summary_file_tools() {
        assert_eq!(
            format_tool_summary("Read", &json!({"file_path": "src/main.rs"})),
            Some("src/main.rs".to_string())
        );
        assert_eq!(
            format_tool_summary("Edit", &json!({"file_path": "/path/to/file.txt"})),
            Some("/path/to/file.txt".to_string())
        );
        assert_eq!(
            format_tool_summary("Write", &json!({"file_path": "output.json"})),
            Some("output.json".to_string())
        );
    }

    #[test]
    fn test_format_tool_summary_bash_truncates() {
        let short_cmd = json!({"command": "ls -la"});
        assert_eq!(
            format_tool_summary("Bash", &short_cmd),
            Some("ls -la".to_string())
        );

        let long_cmd = json!({"command": "this is a very long command that should be truncated because it exceeds sixty characters"});
        let result = format_tool_summary("Bash", &long_cmd).unwrap();
        assert!(result.ends_with("..."));
        assert!(result.len() <= 70); // 60 chars + "..."
    }

    #[test]
    fn test_format_tool_summary_search_tools() {
        assert_eq!(
            format_tool_summary("Grep", &json!({"pattern": "TODO"})),
            Some("TODO".to_string())
        );
        assert_eq!(
            format_tool_summary("Glob", &json!({"pattern": "**/*.rs"})),
            Some("**/*.rs".to_string())
        );
    }

    #[test]
    fn test_format_tool_summary_unknown_tool_returns_none() {
        assert_eq!(
            format_tool_summary("UnknownTool", &json!({"some_field": "value"})),
            None
        );
    }

    #[test]
    fn test_format_tool_summary_missing_field_returns_none() {
        // Read without file_path
        assert_eq!(
            format_tool_summary("Read", &json!({"wrong_field": "value"})),
            None
        );
        // Bash without command
        assert_eq!(format_tool_summary("Bash", &json!({})), None);
    }

    // ========================================================================
    // TuiStreamHandler Tests
    // ========================================================================

    mod tui_stream_handler {
        use super::*;
        use ratatui::style::{Color, Modifier};

        /// Helper to collect lines from TuiStreamHandler
        fn collect_lines(handler: &TuiStreamHandler) -> Vec<ratatui::text::Line<'static>> {
            handler.lines.lock().unwrap().clone()
        }

        #[test]
        fn text_creates_line_on_newline() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When on_text("hello\n") is called
            handler.on_text("hello\n");

            // Then a Line with "hello" content is produced
            let lines = collect_lines(&handler);
            assert_eq!(lines.len(), 1);
            assert_eq!(lines[0].to_string(), "hello");
        }

        #[test]
        fn partial_text_buffering() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When on_text("hel") then on_text("lo\n") is called
            // Note: With markdown parsing, partial text is rendered immediately
            // (markdown doesn't require newlines for paragraphs)
            handler.on_text("hel");
            handler.on_text("lo\n");

            // Then the combined "hello" text is present
            let lines = collect_lines(&handler);
            let full_text: String = lines.iter().map(|l| l.to_string()).collect();
            assert!(
                full_text.contains("hello"),
                "Combined text should contain 'hello'. Lines: {:?}",
                lines
            );
        }

        #[test]
        fn tool_call_produces_formatted_line() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When on_tool_call("Read", "id", &json!({"file_path": "src/main.rs"})) is called
            handler.on_tool_call("Read", "tool_1", &json!({"file_path": "src/main.rs"}));

            // Then a Line starting with "⚙️" and containing "Read" and file path is produced
            let lines = collect_lines(&handler);
            assert_eq!(lines.len(), 1);
            let line_text = lines[0].to_string();
            assert!(
                line_text.contains('\u{2699}'),
                "Should contain gear emoji: {}",
                line_text
            );
            assert!(
                line_text.contains("Read"),
                "Should contain tool name: {}",
                line_text
            );
            assert!(
                line_text.contains("src/main.rs"),
                "Should contain file path: {}",
                line_text
            );
        }

        #[test]
        fn tool_result_verbose_shows_content() {
            // Given TuiStreamHandler with verbose=true
            let mut handler = TuiStreamHandler::new(true);

            // When on_tool_result(...) is called
            handler.on_tool_result("tool_1", "file contents here");

            // Then result content appears in output
            let lines = collect_lines(&handler);
            assert_eq!(lines.len(), 1);
            let line_text = lines[0].to_string();
            assert!(
                line_text.contains('\u{2713}'),
                "Should contain checkmark: {}",
                line_text
            );
            assert!(
                line_text.contains("file contents here"),
                "Should contain result content: {}",
                line_text
            );
        }

        #[test]
        fn tool_result_quiet_is_silent() {
            // Given TuiStreamHandler with verbose=false
            let mut handler = TuiStreamHandler::new(false);

            // When on_tool_result(...) is called
            handler.on_tool_result("tool_1", "file contents here");

            // Then no output is produced
            let lines = collect_lines(&handler);
            assert!(
                lines.is_empty(),
                "verbose=false should not produce tool result output"
            );
        }

        #[test]
        fn error_produces_red_styled_line() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When on_error("fail") is called
            handler.on_error("Something went wrong");

            // Then a Line with red foreground style is produced
            let lines = collect_lines(&handler);
            assert_eq!(lines.len(), 1);
            let line_text = lines[0].to_string();
            assert!(
                line_text.contains('\u{2717}'),
                "Should contain X mark: {}",
                line_text
            );
            assert!(
                line_text.contains("Error"),
                "Should contain 'Error': {}",
                line_text
            );
            assert!(
                line_text.contains("Something went wrong"),
                "Should contain error message: {}",
                line_text
            );

            // Check style is red
            let first_span = &lines[0].spans[0];
            assert_eq!(
                first_span.style.fg,
                Some(Color::Red),
                "Error line should have red foreground"
            );
        }

        #[test]
        fn text_truncation_utf8_safe() {
            // Given TuiStreamHandler with max_line_length configured
            let mut handler = TuiStreamHandler::new(false);

            // When on_text() receives a very long string (500+ chars)
            let long_string: String = "a".repeat(500) + "\n";
            handler.on_text(&long_string);

            // Then line is truncated and ends with "..." and is UTF-8 safe
            let lines = collect_lines(&handler);
            assert_eq!(lines.len(), 1);
            let line_text = lines[0].to_string();

            // Should be truncated (default 200 chars for display)
            assert!(
                line_text.len() < 500,
                "Line should be truncated: len={}",
                line_text.len()
            );
            assert!(
                line_text.ends_with("..."),
                "Truncated line should end with ...: {}",
                line_text
            );
        }

        #[test]
        fn multiple_lines_in_single_text_call() {
            // When text contains multiple newlines
            let mut handler = TuiStreamHandler::new(false);
            handler.on_text("line1\nline2\nline3\n");

            // Then all text content is present
            // Note: Markdown parsing may combine lines into paragraphs differently
            let lines = collect_lines(&handler);
            let full_text: String = lines
                .iter()
                .map(|l| l.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            assert!(
                full_text.contains("line1")
                    && full_text.contains("line2")
                    && full_text.contains("line3"),
                "All lines should be present. Lines: {:?}",
                lines
            );
        }

        #[test]
        fn tool_call_flushes_text_buffer() {
            // Given buffered text
            let mut handler = TuiStreamHandler::new(false);
            handler.on_text("partial text");

            // When tool call arrives
            handler.on_tool_call("Read", "id", &json!({}));

            // Then buffered text is flushed as a line before tool call line
            let lines = collect_lines(&handler);
            assert_eq!(lines.len(), 2);
            assert_eq!(lines[0].to_string(), "partial text");
            assert!(lines[1].to_string().contains('\u{2699}'));
        }

        #[test]
        fn on_complete_flushes_buffer_and_shows_summary() {
            // Given buffered text and verbose mode
            let mut handler = TuiStreamHandler::new(true);
            handler.on_text("final output");

            // When on_complete is called
            handler.on_complete(&SessionResult {
                duration_ms: 1500,
                total_cost_usd: 0.0025,
                num_turns: 3,
                is_error: false,
            });

            // Then buffer is flushed and summary line appears
            let lines = collect_lines(&handler);
            assert!(lines.len() >= 2, "Should have at least 2 lines");
            assert_eq!(lines[0].to_string(), "final output");

            // Find summary line
            let summary = lines.last().unwrap().to_string();
            assert!(
                summary.contains("1500"),
                "Should contain duration: {}",
                summary
            );
            assert!(
                summary.contains("0.0025"),
                "Should contain cost: {}",
                summary
            );
            assert!(summary.contains('3'), "Should contain turns: {}", summary);
        }

        #[test]
        fn on_complete_error_uses_red_style() {
            let mut handler = TuiStreamHandler::new(true);
            handler.on_complete(&SessionResult {
                duration_ms: 1000,
                total_cost_usd: 0.01,
                num_turns: 1,
                is_error: true,
            });

            let lines = collect_lines(&handler);
            assert!(!lines.is_empty());

            // Last line should be red styled for error
            let last_line = lines.last().unwrap();
            assert_eq!(
                last_line.spans[0].style.fg,
                Some(Color::Red),
                "Error completion should have red foreground"
            );
        }

        #[test]
        fn on_complete_success_uses_green_style() {
            let mut handler = TuiStreamHandler::new(true);
            handler.on_complete(&SessionResult {
                duration_ms: 1000,
                total_cost_usd: 0.01,
                num_turns: 1,
                is_error: false,
            });

            let lines = collect_lines(&handler);
            assert!(!lines.is_empty());

            // Last line should be green styled for success
            let last_line = lines.last().unwrap();
            assert_eq!(
                last_line.spans[0].style.fg,
                Some(Color::Green),
                "Success completion should have green foreground"
            );
        }

        #[test]
        fn tool_call_with_no_summary_shows_just_name() {
            let mut handler = TuiStreamHandler::new(false);
            handler.on_tool_call("UnknownTool", "id", &json!({}));

            let lines = collect_lines(&handler);
            assert_eq!(lines.len(), 1);
            let line_text = lines[0].to_string();
            assert!(line_text.contains("UnknownTool"));
            // Should not crash or show "null" for missing summary
        }

        #[test]
        fn get_lines_returns_clone_of_internal_lines() {
            let mut handler = TuiStreamHandler::new(false);
            handler.on_text("test\n");

            let lines1 = handler.get_lines();
            let lines2 = handler.get_lines();

            // Both should have same content
            assert_eq!(lines1.len(), lines2.len());
            assert_eq!(lines1[0].to_string(), lines2[0].to_string());
        }

        // =====================================================================
        // Markdown Rendering Tests
        // =====================================================================

        #[test]
        fn markdown_bold_text_renders_with_bold_modifier() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When on_text("**important**\n") is called
            handler.on_text("**important**\n");

            // Then the text "important" appears with BOLD modifier
            let lines = collect_lines(&handler);
            assert!(!lines.is_empty(), "Should have at least one line");

            // Find a span containing "important" and check it's bold
            let has_bold = lines.iter().any(|line| {
                line.spans.iter().any(|span| {
                    span.content.contains("important")
                        && span.style.add_modifier.contains(Modifier::BOLD)
                })
            });
            assert!(
                has_bold,
                "Should have bold 'important' span. Lines: {:?}",
                lines
            );
        }

        #[test]
        fn markdown_italic_text_renders_with_italic_modifier() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When on_text("*emphasized*\n") is called
            handler.on_text("*emphasized*\n");

            // Then the text "emphasized" appears with ITALIC modifier
            let lines = collect_lines(&handler);
            assert!(!lines.is_empty(), "Should have at least one line");

            let has_italic = lines.iter().any(|line| {
                line.spans.iter().any(|span| {
                    span.content.contains("emphasized")
                        && span.style.add_modifier.contains(Modifier::ITALIC)
                })
            });
            assert!(
                has_italic,
                "Should have italic 'emphasized' span. Lines: {:?}",
                lines
            );
        }

        #[test]
        fn markdown_inline_code_renders_with_distinct_style() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When on_text("`code`\n") is called
            handler.on_text("`code`\n");

            // Then the text "code" appears with distinct styling (different from default)
            let lines = collect_lines(&handler);
            assert!(!lines.is_empty(), "Should have at least one line");

            let has_code_style = lines.iter().any(|line| {
                line.spans.iter().any(|span| {
                    span.content.contains("code")
                        && (span.style.fg.is_some() || span.style.bg.is_some())
                })
            });
            assert!(
                has_code_style,
                "Should have styled 'code' span. Lines: {:?}",
                lines
            );
        }

        #[test]
        fn markdown_header_renders_content() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When on_text("## Section Title\n") is called
            handler.on_text("## Section Title\n");

            // Then "Section Title" appears in the output
            // Note: tui-markdown may or may not apply bold/color to headers
            // depending on its default stylesheet
            let lines = collect_lines(&handler);
            assert!(!lines.is_empty(), "Should have at least one line");

            let has_header_content = lines.iter().any(|line| {
                line.spans
                    .iter()
                    .any(|span| span.content.contains("Section Title"))
            });
            assert!(
                has_header_content,
                "Should have header content. Lines: {:?}",
                lines
            );
        }

        #[test]
        fn markdown_streaming_continuity_handles_split_formatting() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When markdown arrives in chunks: "**bo" then "ld**\n"
            handler.on_text("**bo");
            handler.on_text("ld**\n");

            // Then the complete "bold" text renders with BOLD modifier
            let lines = collect_lines(&handler);

            let has_bold = lines.iter().any(|line| {
                line.spans
                    .iter()
                    .any(|span| span.style.add_modifier.contains(Modifier::BOLD))
            });
            assert!(
                has_bold,
                "Split markdown should still render bold. Lines: {:?}",
                lines
            );
        }

        #[test]
        fn markdown_mixed_content_renders_correctly() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When on_text() receives mixed markdown
            handler.on_text("Normal **bold** and *italic* text\n");

            // Then appropriate spans have appropriate styling
            let lines = collect_lines(&handler);
            assert!(!lines.is_empty(), "Should have at least one line");

            let has_bold = lines.iter().any(|line| {
                line.spans.iter().any(|span| {
                    span.content.contains("bold")
                        && span.style.add_modifier.contains(Modifier::BOLD)
                })
            });
            let has_italic = lines.iter().any(|line| {
                line.spans.iter().any(|span| {
                    span.content.contains("italic")
                        && span.style.add_modifier.contains(Modifier::ITALIC)
                })
            });

            assert!(has_bold, "Should have bold span. Lines: {:?}", lines);
            assert!(has_italic, "Should have italic span. Lines: {:?}", lines);
        }

        #[test]
        fn markdown_tool_call_styling_preserved() {
            // Given TuiStreamHandler with markdown text then tool call
            let mut handler = TuiStreamHandler::new(false);

            // When markdown text followed by tool call
            handler.on_text("**bold**\n");
            handler.on_tool_call("Read", "id", &json!({"file_path": "src/main.rs"}));

            // Then tool call still has blue styling
            let lines = collect_lines(&handler);
            assert!(lines.len() >= 2, "Should have at least 2 lines");

            // Last line should be the tool call with blue color
            let tool_line = lines.last().unwrap();
            let has_blue = tool_line
                .spans
                .iter()
                .any(|span| span.style.fg == Some(Color::Blue));
            assert!(
                has_blue,
                "Tool call should preserve blue styling. Line: {:?}",
                tool_line
            );
        }

        #[test]
        fn markdown_error_styling_preserved() {
            // Given TuiStreamHandler with markdown text then error
            let mut handler = TuiStreamHandler::new(false);

            // When markdown text followed by error
            handler.on_text("**bold**\n");
            handler.on_error("Something went wrong");

            // Then error still has red styling
            let lines = collect_lines(&handler);
            assert!(lines.len() >= 2, "Should have at least 2 lines");

            // Last line should be the error with red color
            let error_line = lines.last().unwrap();
            let has_red = error_line
                .spans
                .iter()
                .any(|span| span.style.fg == Some(Color::Red));
            assert!(
                has_red,
                "Error should preserve red styling. Line: {:?}",
                error_line
            );
        }

        #[test]
        fn markdown_partial_formatting_does_not_crash() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When incomplete markdown is sent and flushed
            handler.on_text("**unclosed bold");
            handler.flush_text_buffer();

            // Then no panic occurs and text is present
            let lines = collect_lines(&handler);
            // Should have some output (either the partial text or nothing)
            // Main assertion is that we didn't panic
            let _ = lines; // Use the variable to avoid warning
        }

        // =====================================================================
        // ANSI Color Preservation Tests
        // =====================================================================

        #[test]
        fn ansi_green_text_produces_green_style() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When on_text receives ANSI green text
            handler.on_text("\x1b[32mgreen text\x1b[0m\n");

            // Then the text should have green foreground color
            let lines = collect_lines(&handler);
            assert!(!lines.is_empty(), "Should have at least one line");

            let has_green = lines.iter().any(|line| {
                line.spans
                    .iter()
                    .any(|span| span.style.fg == Some(Color::Green))
            });
            assert!(
                has_green,
                "Should have green styled span. Lines: {:?}",
                lines
            );
        }

        #[test]
        fn ansi_bold_text_produces_bold_modifier() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When on_text receives ANSI bold text
            handler.on_text("\x1b[1mbold text\x1b[0m\n");

            // Then the text should have BOLD modifier
            let lines = collect_lines(&handler);
            assert!(!lines.is_empty(), "Should have at least one line");

            let has_bold = lines.iter().any(|line| {
                line.spans
                    .iter()
                    .any(|span| span.style.add_modifier.contains(Modifier::BOLD))
            });
            assert!(has_bold, "Should have bold styled span. Lines: {:?}", lines);
        }

        #[test]
        fn ansi_mixed_styles_preserved() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When on_text receives mixed ANSI styles (bold + green)
            handler.on_text("\x1b[1;32mbold green\x1b[0m normal\n");

            // Then the text should have appropriate styles
            let lines = collect_lines(&handler);
            assert!(!lines.is_empty(), "Should have at least one line");

            // Check for green color
            let has_styled = lines.iter().any(|line| {
                line.spans.iter().any(|span| {
                    span.style.fg == Some(Color::Green)
                        || span.style.add_modifier.contains(Modifier::BOLD)
                })
            });
            assert!(
                has_styled,
                "Should have styled span with color or bold. Lines: {:?}",
                lines
            );
        }

        #[test]
        fn ansi_plain_text_renders_without_crash() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When on_text receives plain text (no ANSI)
            handler.on_text("plain text without ansi\n");

            // Then text renders normally (fallback to markdown)
            let lines = collect_lines(&handler);
            assert!(!lines.is_empty(), "Should have at least one line");

            let full_text: String = lines.iter().map(|l| l.to_string()).collect();
            assert!(
                full_text.contains("plain text"),
                "Should contain the text. Lines: {:?}",
                lines
            );
        }

        #[test]
        fn ansi_red_error_text_produces_red_style() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When on_text receives ANSI red text (like error output)
            handler.on_text("\x1b[31mError: something failed\x1b[0m\n");

            // Then the text should have red foreground color
            let lines = collect_lines(&handler);
            assert!(!lines.is_empty(), "Should have at least one line");

            let has_red = lines.iter().any(|line| {
                line.spans
                    .iter()
                    .any(|span| span.style.fg == Some(Color::Red))
            });
            assert!(has_red, "Should have red styled span. Lines: {:?}", lines);
        }

        #[test]
        fn ansi_cyan_text_produces_cyan_style() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When on_text receives ANSI cyan text
            handler.on_text("\x1b[36mcyan text\x1b[0m\n");

            // Then the text should have cyan foreground color
            let lines = collect_lines(&handler);
            assert!(!lines.is_empty(), "Should have at least one line");

            let has_cyan = lines.iter().any(|line| {
                line.spans
                    .iter()
                    .any(|span| span.style.fg == Some(Color::Cyan))
            });
            assert!(has_cyan, "Should have cyan styled span. Lines: {:?}", lines);
        }

        #[test]
        fn ansi_underline_produces_underline_modifier() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When on_text receives ANSI underlined text
            handler.on_text("\x1b[4munderlined\x1b[0m\n");

            // Then the text should have UNDERLINED modifier
            let lines = collect_lines(&handler);
            assert!(!lines.is_empty(), "Should have at least one line");

            let has_underline = lines.iter().any(|line| {
                line.spans
                    .iter()
                    .any(|span| span.style.add_modifier.contains(Modifier::UNDERLINED))
            });
            assert!(
                has_underline,
                "Should have underlined styled span. Lines: {:?}",
                lines
            );
        }

        #[test]
        fn ansi_multiline_preserves_colors() {
            // Given TuiStreamHandler
            let mut handler = TuiStreamHandler::new(false);

            // When on_text receives multiple ANSI-colored lines
            handler.on_text("\x1b[32mline 1 green\x1b[0m\n\x1b[31mline 2 red\x1b[0m\n");

            // Then both colors should be present
            let lines = collect_lines(&handler);
            assert!(lines.len() >= 2, "Should have at least two lines");

            let has_green = lines.iter().any(|line| {
                line.spans
                    .iter()
                    .any(|span| span.style.fg == Some(Color::Green))
            });
            let has_red = lines.iter().any(|line| {
                line.spans
                    .iter()
                    .any(|span| span.style.fg == Some(Color::Red))
            });

            assert!(has_green, "Should have green line. Lines: {:?}", lines);
            assert!(has_red, "Should have red line. Lines: {:?}", lines);
        }
    }
}

// =========================================================================
// ANSI Detection Tests (module-level)
// =========================================================================

#[cfg(test)]
mod ansi_detection_tests {
    use super::*;

    #[test]
    fn contains_ansi_with_color_code() {
        assert!(contains_ansi("\x1b[32mgreen\x1b[0m"));
    }

    #[test]
    fn contains_ansi_with_bold() {
        assert!(contains_ansi("\x1b[1mbold\x1b[0m"));
    }

    #[test]
    fn contains_ansi_plain_text_returns_false() {
        assert!(!contains_ansi("hello world"));
    }

    #[test]
    fn contains_ansi_markdown_returns_false() {
        assert!(!contains_ansi("**bold** and *italic*"));
    }

    #[test]
    fn contains_ansi_empty_string_returns_false() {
        assert!(!contains_ansi(""));
    }

    #[test]
    fn contains_ansi_with_escape_in_middle() {
        assert!(contains_ansi("prefix \x1b[31mred\x1b[0m suffix"));
    }
}
