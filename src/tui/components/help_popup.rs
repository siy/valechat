use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem},
    Frame,
};

use crate::tui::{components::Component, Event, Theme};

pub struct HelpPopup {
    is_visible: bool,
}

impl HelpPopup {
    pub fn new() -> Self {
        Self {
            is_visible: false,
        }
    }

    #[allow(dead_code)]
    pub fn show(&mut self) {
        self.is_visible = true;
    }

    pub fn hide(&mut self) {
        self.is_visible = false;
    }

    pub fn is_visible(&self) -> bool {
        self.is_visible
    }

    pub fn toggle(&mut self) {
        self.is_visible = !self.is_visible;
    }

    fn get_help_content() -> Vec<(&'static str, &'static str)> {
        vec![
            ("Navigation", ""),
            ("  Tab / Shift+Tab", "Switch between panels"),
            ("  Ctrl/Alt+1", "Go to Conversations"),
            ("  Ctrl/Alt+2", "Go to Chat View"),
            ("  Ctrl/Alt+3", "Go to Input Box"),
            ("  Escape", "Return to Conversations"),
            ("  Arrow keys / hjkl", "Navigate lists and messages"),
            ("  Page Up/Down", "Scroll messages quickly"),
            ("  Home/End (g/G)", "Go to top/bottom of messages"),
            ("", ""),
            ("Conversations", ""),
            ("  n", "New conversation"),
            ("  Ctrl+N", "New conversation (global)"),
            ("  Enter", "Select conversation"),
            ("  d", "Delete conversation"),
            ("  r", "Rename conversation"),
            ("", ""),
            ("Chat", ""),
            ("  Enter", "Send message"),
            ("  Shift+Enter", "New line in message"),
            ("  Tab (in input)", "Toggle multiline mode"),
            ("  /command", "Execute CLI commands (try /help)"),
            ("", ""),
            ("General", ""),
            ("  F1 / Ctrl+/", "Show/hide this help"),
            ("  Ctrl+M", "Toggle cost tracking details"),
            ("  Ctrl+C / Ctrl+Q", "Quit application"),
            ("  Ctrl+S", "Settings"),
            ("  Ctrl+E", "Export conversation"),
        ]
    }

    fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ])
            .split(r);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ])
            .split(popup_layout[1])[1]
    }
}

impl Component for HelpPopup {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if !self.is_visible {
            return;
        }

        let popup_area = Self::centered_rect(60, 70, area);

        // Clear the area
        frame.render_widget(Clear, popup_area);

        let help_content = Self::get_help_content();
        let items: Vec<ListItem> = help_content
            .iter()
            .map(|(key, description)| {
                if key.is_empty() {
                    ListItem::new(Line::from(""))
                } else if description.is_empty() {
                    // Section header
                    ListItem::new(Line::from(Span::styled(
                        *key,
                        theme.accent().add_modifier(Modifier::BOLD)
                    )))
                } else {
                    // Key binding
                    ListItem::new(Line::from(vec![
                        Span::styled(*key, theme.highlight()),
                        Span::raw(": "),
                        Span::styled(*description, theme.normal()),
                    ]))
                }
            })
            .collect();

        let help_list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(theme.accent())
                    .title(" Help - Press F1 or Esc to close ")
            )
            .style(theme.normal());

        frame.render_widget(help_list, popup_area);
    }

    fn handle_event(&mut self, event: &Event) -> bool {
        if !self.is_visible {
            return false;
        }

        match event {
            Event::Key(key) => {
                match (key.code, key.modifiers) {
                    (KeyCode::Esc, _) | (KeyCode::F(1), _) | (KeyCode::Char('q'), _) => {
                        self.hide();
                        true
                    }
                    (KeyCode::Char('/'), KeyModifiers::CONTROL) => {
                        self.hide();
                        true
                    }
                    _ => true, // Consume all events when visible
                }
            }
            _ => true,
        }
    }

    fn title(&self) -> &str {
        "HelpPopup"
    }
}