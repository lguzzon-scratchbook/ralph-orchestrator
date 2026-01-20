use crate::state::TuiState;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

/// Footer widget that adapts to terminal width.
pub struct Footer<'a> {
    state: &'a TuiState,
}

impl<'a> Footer<'a> {
    pub fn new(state: &'a TuiState) -> Self {
        Self { state }
    }
}

impl Widget for Footer<'_> {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        // Render block with top border as separator
        let block = Block::default().borders(Borders::TOP);
        let inner_area = block.inner(area);
        block.render(area, buf);

        // If search state has an active query, render search display
        if let Some(query) = &self.state.search_state.query {
            let match_info = if self.state.search_state.matches.is_empty() {
                "no matches".to_string()
            } else {
                format!(
                    "{}/{}",
                    self.state.search_state.current_match + 1,
                    self.state.search_state.matches.len()
                )
            };

            let line = Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    format!("Search: {} ", query),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(match_info, Style::default().fg(Color::Cyan)),
            ]);

            Paragraph::new(line).render(inner_area, buf);
            return;
        }

        // Show search input prompt (legacy fallback for when search_query is used)
        if !self.state.search_query.is_empty() {
            let prompt = if self.state.search_forward { "/" } else { "?" };
            let line = Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    format!("{}{}", prompt, self.state.search_query),
                    Style::default().fg(Color::Yellow),
                ),
            ]);

            Paragraph::new(line).render(inner_area, buf);
            return;
        }

        // Default footer with flexible layout
        // Build left content: optional alert + last event
        let mut left_spans = vec![Span::raw(" ")];

        // Show new iteration alert when viewing history and a new iteration arrived
        if let Some(iter_num) = self.state.new_iteration_alert
            && !self.state.following_latest
        {
            left_spans.push(Span::styled(
                format!("▶ New: iter {} ", iter_num),
                Style::default().fg(Color::Green),
            ));
            left_spans.push(Span::raw("│ "));
        }

        let last_event = self
            .state
            .last_event
            .as_ref()
            .map_or_else(|| "Last: —".to_string(), |e| format!("Last: {e}"));
        left_spans.push(Span::raw(last_event.clone()));

        let indicator_text = if self.state.pending_hat.is_none() {
            "■ done"
        } else if self.state.is_active() {
            "◉ active"
        } else {
            "◯ idle"
        };

        let indicator_style = if self.state.pending_hat.is_none() {
            Style::default().fg(Color::Blue)
        } else if self.state.is_active() {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM)
        };

        // Calculate left content width for layout
        let left_content_width: usize = left_spans.iter().map(|s| s.width()).sum();

        // Use horizontal layout: left content | flexible spacer | right indicator
        let chunks = Layout::horizontal([
            Constraint::Length(left_content_width as u16), // Alert + " Last: event"
            Constraint::Fill(1),                           // Flexible spacer
            Constraint::Length((indicator_text.len() + 2) as u16), // "indicator "
        ])
        .split(inner_area);

        // Render left side (alert + last event)
        let left = Line::from(left_spans);
        Paragraph::new(left).render(chunks[0], buf);

        // Render right side (indicator)
        let right = Line::from(vec![
            Span::styled(indicator_text, indicator_style),
            Span::raw(" "),
        ]);
        Paragraph::new(right).render(chunks[2], buf);
    }
}

/// Convenience function for rendering the footer.
pub fn render(state: &TuiState) -> Footer<'_> {
    Footer::new(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_to_string(state: &TuiState) -> String {
        render_to_string_with_width(state, 80)
    }

    fn render_to_string_with_width(state: &TuiState, width: u16) -> String {
        // Height of 2: 1 for top border + 1 for content
        let backend = TestBackend::new(width, 2);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let widget = render(state);
                f.render_widget(widget, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    // =========================================================================
    // Acceptance Criteria Tests (Task 06)
    // =========================================================================

    #[test]
    fn footer_shows_new_iteration_alert() {
        // Given new_iteration_alert = Some(5) and following_latest = false
        let mut state = TuiState::new();
        state.new_iteration_alert = Some(5);
        state.following_latest = false;

        // When footer renders
        let text = render_to_string(&state);

        // Then output contains "▶ New: iter 5"
        assert!(
            text.contains("▶ New: iter 5"),
            "should show new iteration alert, got: {}",
            text
        );
    }

    #[test]
    fn footer_no_alert_when_following() {
        // Given following_latest = true (even if new_iteration_alert has a value)
        let mut state = TuiState::new();
        state.new_iteration_alert = Some(5);
        state.following_latest = true;

        // When footer renders
        let text = render_to_string(&state);

        // Then no alert is shown
        assert!(
            !text.contains("▶ New:"),
            "should NOT show alert when following_latest=true, got: {}",
            text
        );
    }

    #[test]
    fn footer_shows_last_event() {
        // Given last_event = Some("build.done")
        let mut state = TuiState::new();
        state.last_event = Some("build.done".to_string());

        // When footer renders
        let text = render_to_string(&state);

        // Then output contains "build.done"
        assert!(
            text.contains("build.done"),
            "should show last event, got: {}",
            text
        );
    }

    #[test]
    fn footer_shows_activity_indicator_active() {
        // Given activity is ongoing (recent event)
        let mut state = TuiState::new();
        state.last_event = Some("build.task".to_string());
        state.last_event_at = Some(std::time::Instant::now());
        state.pending_hat = Some((ralph_proto::HatId::new("builder"), "🔨Builder".to_string()));

        // When footer renders
        let text = render_to_string(&state);

        // Then output contains ◉ (active indicator)
        assert!(
            text.contains('◉'),
            "should show active indicator, got: {}",
            text
        );
    }

    #[test]
    fn footer_shows_search_query() {
        // Given search_state has an active query
        let mut state = TuiState::new();
        state.search_state.query = Some("test".to_string());
        state.search_state.matches = vec![(0, 0), (1, 0)]; // 2 matches

        // When footer renders
        let text = render_to_string(&state);

        // Then output contains "Search: test 1/2"
        assert!(
            text.contains("Search: test"),
            "should show search query, got: {}",
            text
        );
        assert!(
            text.contains("1/2"),
            "should show match position, got: {}",
            text
        );
    }

    #[test]
    fn footer_shows_no_matches_when_empty() {
        // Given search with no matches
        let mut state = TuiState::new();
        state.search_state.query = Some("notfound".to_string());
        state.search_state.matches = vec![];

        // When footer renders
        let text = render_to_string(&state);

        // Then output contains "no matches"
        assert!(
            text.contains("no matches"),
            "should show no matches indicator, got: {}",
            text
        );
    }

    #[test]
    fn footer_shows_done_indicator_when_complete() {
        // Given pending_hat = None (task complete)
        let mut state = TuiState::new();
        state.pending_hat = None;
        state.last_event = Some("loop.terminate".to_string());

        // When footer renders
        let text = render_to_string(&state);

        // Then output contains ■ done
        assert!(
            text.contains('■') && text.contains("done"),
            "should show done indicator, got: {}",
            text
        );
    }

    #[test]
    fn footer_shows_idle_indicator() {
        // Given activity is not ongoing (no recent event)
        let mut state = TuiState::new();
        state.last_event = Some("old.event".to_string());
        state.last_event_at = Some(
            std::time::Instant::now()
                .checked_sub(std::time::Duration::from_secs(5))
                .unwrap(),
        );
        state.pending_hat = Some((ralph_proto::HatId::new("builder"), "🔨Builder".to_string()));

        // When footer renders
        let text = render_to_string(&state);

        // Then output contains ◯ (idle indicator)
        assert!(
            text.contains('◯'),
            "should show idle indicator when no recent activity, got: {}",
            text
        );
    }
}
