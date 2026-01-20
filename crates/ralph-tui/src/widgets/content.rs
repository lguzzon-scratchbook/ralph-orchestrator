//! Content pane widget for rendering iteration output.
//!
//! This widget replaces the VT100 terminal widget with a simpler line-based
//! renderer that displays formatted Lines from an IterationBuffer.

use crate::state::IterationBuffer;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

/// Widget that renders the content of an iteration buffer.
///
/// The widget displays the visible lines from the buffer (respecting scroll offset)
/// and optionally highlights search matches.
pub struct ContentPane<'a> {
    /// Reference to the iteration buffer to render
    buffer: &'a IterationBuffer,
    /// Optional search query for highlighting matches
    search_query: Option<&'a str>,
}

impl<'a> ContentPane<'a> {
    /// Creates a new ContentPane for the given iteration buffer.
    pub fn new(buffer: &'a IterationBuffer) -> Self {
        Self {
            buffer,
            search_query: None,
        }
    }

    /// Sets the search query for highlighting matches.
    pub fn with_search(mut self, query: &'a str) -> Self {
        if !query.is_empty() {
            self.search_query = Some(query);
        }
        self
    }
}

impl Widget for ContentPane<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Get visible lines from the buffer (now returns owned Vec due to interior mutability)
        let visible = self.buffer.visible_lines(area.height as usize);

        for (row_idx, line) in visible.iter().enumerate() {
            let y = area.y + row_idx as u16;
            if y >= area.y + area.height {
                break;
            }

            // Apply search highlighting if we have a query
            let rendered_line = if let Some(query) = self.search_query {
                highlight_search_matches(line, query)
            } else {
                line.clone()
            };

            // Render the line into the buffer
            let mut x = area.x;
            for span in &rendered_line.spans {
                let content = span.content.as_ref();
                for ch in content.chars() {
                    if x >= area.x + area.width {
                        break;
                    }
                    buf[(x, y)].set_char(ch).set_style(span.style);
                    x += 1;
                }
            }
        }
    }
}

/// Highlights search matches in a line with a distinct style.
fn highlight_search_matches(line: &Line<'static>, query: &str) -> Line<'static> {
    if query.is_empty() {
        return line.clone();
    }

    let query_lower = query.to_lowercase();
    let highlight_style = Style::default()
        .fg(Color::Black)
        .bg(Color::Yellow)
        .add_modifier(Modifier::REVERSED);

    let mut new_spans = Vec::new();

    for span in &line.spans {
        let content = span.content.as_ref();
        let content_lower = content.to_lowercase();
        let mut last_end = 0;

        // Find all matches in this span's content
        for (match_start, _) in content_lower.match_indices(&query_lower) {
            let match_end = match_start + query.len();

            // Add the part before the match with original style
            if match_start > last_end {
                new_spans.push(Span::styled(
                    content[last_end..match_start].to_string(),
                    span.style,
                ));
            }

            // Add the matched part with highlight style
            new_spans.push(Span::styled(
                content[match_start..match_end].to_string(),
                highlight_style,
            ));

            last_end = match_end;
        }

        // Add any remaining content after the last match
        if last_end < content.len() {
            new_spans.push(Span::styled(content[last_end..].to_string(), span.style));
        } else if last_end == 0 {
            // No matches found, keep original span
            new_spans.push(span.clone());
        }
    }

    Line::from(new_spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    /// Helper to render ContentPane and return buffer content as strings
    fn render_content_pane(
        buffer: &IterationBuffer,
        search: Option<&str>,
        width: u16,
        height: u16,
    ) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let mut widget = ContentPane::new(buffer);
                if let Some(q) = search {
                    widget = widget.with_search(q);
                }
                f.render_widget(widget, f.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        // Extract lines from the buffer
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    /// Helper to check if a cell has the highlight style
    fn has_highlight_style(
        buffer: &IterationBuffer,
        search: &str,
        width: u16,
        height: u16,
        x: u16,
        y: u16,
    ) -> bool {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let widget = ContentPane::new(buffer).with_search(search);
                f.render_widget(widget, f.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        let cell = &buf[(x, y)];
        // Check for highlight: typically reverse or yellow background
        cell.modifier.contains(Modifier::REVERSED)
            || cell.bg == Color::Yellow
            || cell.fg == Color::Black
    }

    // =========================================================================
    // Acceptance Criteria 1: Renders Lines
    // =========================================================================

    #[test]
    fn renders_lines_when_viewport_fits_all() {
        // Given a buffer with 3 lines
        let mut buffer = IterationBuffer::new(1);
        buffer.append_line(Line::from("first line"));
        buffer.append_line(Line::from("second line"));
        buffer.append_line(Line::from("third line"));

        // When ContentPane renders with viewport height >= 3
        let lines = render_content_pane(&buffer, None, 40, 5);

        // Then all 3 lines are visible in the output
        assert!(
            lines[0].contains("first line"),
            "first line should be visible, got: {:?}",
            lines
        );
        assert!(
            lines[1].contains("second line"),
            "second line should be visible, got: {:?}",
            lines
        );
        assert!(
            lines[2].contains("third line"),
            "third line should be visible, got: {:?}",
            lines
        );
    }

    #[test]
    fn renders_lines_preserves_styling() {
        // Given a buffer with styled lines
        let mut buffer = IterationBuffer::new(1);
        buffer.append_line(Line::from(vec![
            Span::styled("error: ", Style::default().fg(Color::Red)),
            Span::raw("something went wrong"),
        ]));

        // When ContentPane renders
        let backend = TestBackend::new(40, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let widget = ContentPane::new(&buffer);
                f.render_widget(widget, f.area());
            })
            .unwrap();

        // Then the styled spans are rendered (check color of first cell)
        let buf = terminal.backend().buffer();
        // The 'e' in 'error' should be red
        assert_eq!(
            buf[(0, 0)].fg,
            Color::Red,
            "styled span should preserve color"
        );
    }

    // =========================================================================
    // Acceptance Criteria 2: Respects Scroll Offset
    // =========================================================================

    #[test]
    fn respects_scroll_offset() {
        // Given a buffer with 10 lines and scroll_offset 5
        let mut buffer = IterationBuffer::new(1);
        for i in 0..10 {
            buffer.append_line(Line::from(format!("line {}", i)));
        }
        buffer.scroll_offset = 5;

        // When ContentPane renders with viewport height 5
        let lines = render_content_pane(&buffer, None, 40, 5);

        // Then lines 5-9 are shown (not 0-4)
        assert!(
            lines[0].contains("line 5"),
            "should show line 5 first, got: {:?}",
            lines
        );
        assert!(
            lines[4].contains("line 9"),
            "should show line 9 last, got: {:?}",
            lines
        );
        assert!(
            !lines.iter().any(|l| l.contains("line 0")),
            "line 0 should not be visible"
        );
    }

    #[test]
    fn scroll_offset_at_end_shows_last_lines() {
        let mut buffer = IterationBuffer::new(1);
        for i in 0..10 {
            buffer.append_line(Line::from(format!("line {}", i)));
        }
        buffer.scroll_bottom(3); // viewport 3, should show lines 7-9

        let lines = render_content_pane(&buffer, None, 40, 3);

        assert!(
            lines[0].contains("line 7"),
            "first visible should be line 7, got: {:?}",
            lines
        );
        assert!(
            lines[2].contains("line 9"),
            "last visible should be line 9, got: {:?}",
            lines
        );
    }

    // =========================================================================
    // Acceptance Criteria 3: Search Highlight
    // =========================================================================

    #[test]
    fn search_highlights_matches() {
        // Given a buffer with lines containing "foo"
        let mut buffer = IterationBuffer::new(1);
        buffer.append_line(Line::from("this contains foo in the middle"));
        buffer.append_line(Line::from("no match here"));
        buffer.append_line(Line::from("foo at start"));

        // When ContentPane renders with search query "foo"
        // Then "foo" spans are highlighted (different style)
        // Check that the 'f' in 'foo' at position 14 (line 0) has highlight style
        assert!(
            has_highlight_style(&buffer, "foo", 40, 3, 14, 0),
            "search match 'foo' should be highlighted"
        );
    }

    #[test]
    fn search_highlights_multiple_matches_per_line() {
        let mut buffer = IterationBuffer::new(1);
        buffer.append_line(Line::from("foo and another foo here"));

        // Both occurrences should be highlighted
        assert!(
            has_highlight_style(&buffer, "foo", 40, 1, 0, 0),
            "first 'foo' should be highlighted"
        );
        assert!(
            has_highlight_style(&buffer, "foo", 40, 1, 16, 0),
            "second 'foo' should be highlighted"
        );
    }

    #[test]
    fn search_case_insensitive() {
        let mut buffer = IterationBuffer::new(1);
        buffer.append_line(Line::from("Contains FOO uppercase"));

        // Search for lowercase should match uppercase
        assert!(
            has_highlight_style(&buffer, "foo", 40, 1, 9, 0),
            "case-insensitive search should highlight FOO"
        );
    }

    #[test]
    fn empty_search_query_no_highlight() {
        let mut buffer = IterationBuffer::new(1);
        buffer.append_line(Line::from("some text"));

        // Empty search shouldn't highlight anything
        let backend = TestBackend::new(40, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let widget = ContentPane::new(&buffer).with_search("");
                f.render_widget(widget, f.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        // No cells should have highlight modifier
        for x in 0..40 {
            assert!(
                !buf[(x, 0)].modifier.contains(Modifier::REVERSED),
                "empty search should not highlight"
            );
        }
    }

    // =========================================================================
    // Acceptance Criteria 4: Empty Buffer Handling
    // =========================================================================

    #[test]
    fn empty_buffer_renders_without_panic() {
        // Given an empty IterationBuffer
        let buffer = IterationBuffer::new(1);

        // When ContentPane renders
        // Then no panic occurs and empty area is shown
        let lines = render_content_pane(&buffer, None, 40, 5);

        // All lines should be empty (spaces)
        for line in &lines {
            assert!(
                line.trim().is_empty(),
                "empty buffer should render blank lines, got: {:?}",
                line
            );
        }
    }

    #[test]
    fn empty_buffer_with_search_renders_without_panic() {
        let buffer = IterationBuffer::new(1);

        // Should not panic even with search query on empty buffer
        let lines = render_content_pane(&buffer, Some("search"), 40, 5);

        for line in &lines {
            assert!(line.trim().is_empty());
        }
    }

    // =========================================================================
    // Acceptance Criteria 5: Widget Integration
    // =========================================================================

    #[test]
    fn widget_fills_provided_rect() {
        let mut buffer = IterationBuffer::new(1);
        buffer.append_line(Line::from("test"));

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        // Render into a specific sub-area
        let area = Rect::new(5, 5, 30, 10);
        terminal
            .draw(|f| {
                let widget = ContentPane::new(&buffer);
                f.render_widget(widget, area);
            })
            .unwrap();

        // Content should be at position (5, 5), not (0, 0)
        let buf = terminal.backend().buffer();
        assert_eq!(buf[(5, 5)].symbol(), "t", "content should start at area.x");
    }

    #[test]
    fn widget_truncates_lines_to_area_width() {
        let mut buffer = IterationBuffer::new(1);
        buffer.append_line(Line::from(
            "this is a very long line that exceeds the width",
        ));

        // Render with narrow width
        let lines = render_content_pane(&buffer, None, 20, 1);

        // Line should be truncated to 20 chars
        assert_eq!(lines[0].len(), 20, "line should be truncated to area width");
    }
}
