use ratatui::{
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::{components::Component, Event, Theme};

pub struct StatusBar {
    status_message: String,
    model_info: String,
    cost_info: String,
    connection_status: ConnectionStatus,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConnectionStatus {
    #[allow(dead_code)]
    Connected,
    #[allow(dead_code)]
    Connecting,
    Disconnected,
    #[allow(dead_code)]
    Error(String),
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            status_message: "Ready".to_string(),
            model_info: "No model selected".to_string(),
            cost_info: "$0.00".to_string(),
            connection_status: ConnectionStatus::Disconnected,
        }
    }

    pub fn set_status(&mut self, message: String) {
        self.status_message = message;
    }

    #[allow(dead_code)]
    pub fn set_model_info(&mut self, provider: &str, model: &str) {
        self.model_info = format!("{}: {}", provider, model);
    }

    #[allow(dead_code)]
    pub fn set_cost_info(&mut self, cost: f64) {
        self.cost_info = format!("${:.4}", cost);
    }

    #[allow(dead_code)]
    pub fn set_connection_status(&mut self, status: ConnectionStatus) {
        self.connection_status = status;
    }

    fn get_connection_indicator(&self, theme: &Theme) -> (String, Style) {
        match &self.connection_status {
            ConnectionStatus::Connected => ("●".to_string(), theme.success()),
            ConnectionStatus::Connecting => ("◐".to_string(), theme.warning()),
            ConnectionStatus::Disconnected => ("○".to_string(), theme.secondary()),
            ConnectionStatus::Error(_) => ("●".to_string(), theme.error()),
        }
    }
}

impl Component for StatusBar {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let (conn_symbol, conn_style) = self.get_connection_indicator(theme);
        
        let status_line = Line::from(vec![
            Span::styled(&self.status_message, theme.normal()),
            Span::raw(" | "),
            Span::styled(&self.model_info, theme.accent()),
            Span::raw(" | "),
            Span::styled(&self.cost_info, theme.warning()),
            Span::raw(" | "),
            Span::styled(conn_symbol, conn_style),
            Span::raw(" "),
            Span::styled(
                match &self.connection_status {
                    ConnectionStatus::Connected => "Connected",
                    ConnectionStatus::Connecting => "Connecting...",
                    ConnectionStatus::Disconnected => "Disconnected",
                    ConnectionStatus::Error(err) => err,
                },
                theme.secondary()
            ),
            // Help text on the right
            Span::raw(" | "),
            Span::styled("F1: Help", theme.secondary()),
            Span::raw(" | "),
            Span::styled("Ctrl+Q: Quit", theme.secondary()),
        ]);

        let paragraph = Paragraph::new(status_line)
            .block(Block::default().borders(Borders::TOP).border_style(theme.border()))
            .alignment(Alignment::Left);

        frame.render_widget(paragraph, area);
    }

    fn handle_event(&mut self, _event: &Event) -> bool {
        false // Status bar doesn't handle events
    }

    fn title(&self) -> &str {
        "StatusBar"
    }
}