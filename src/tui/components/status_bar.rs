use ratatui::{
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::{components::Component, Event, Theme};

// Constants for repeated strings
const SEPARATOR: &str = " │ ";
const DEFAULT_COST: &str = "$0.00";
const STATUS_MESSAGE_WIDTH: usize = 40;

// Connection status text constants
const CONNECTION_CONNECTED: &str = "Connected";
const CONNECTION_CONNECTING: &str = "Connecting...";
const CONNECTION_DISCONNECTED: &str = "Disconnected";

#[derive(Debug, Clone)]
pub struct KeyHint {
    pub key: String,
    pub action: String,
}

impl KeyHint {
    pub fn new(key: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            action: action.into(),
        }
    }
}

pub struct StatusBar {
    status_message: String,
    model_info: String,
    cost_info: String,
    connection_status: ConnectionStatus,
    key_hints: Vec<KeyHint>,
    conversation_cost: f64,
    session_cost: f64,
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
            cost_info: DEFAULT_COST.to_string(),
            connection_status: ConnectionStatus::Disconnected,
            key_hints: vec![
                KeyHint::new("F1", "Help"),
                KeyHint::new("Tab", "Switch Panel"),
                KeyHint::new("Ctrl+Q", "Quit"),
            ],
            conversation_cost: 0.0,
            session_cost: 0.0,
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

    pub fn set_cost_info_string(&mut self, cost_info: String) {
        self.cost_info = cost_info;
    }

    #[allow(dead_code)]
    pub fn set_connection_status(&mut self, status: ConnectionStatus) {
        self.connection_status = status;
    }

    pub fn set_key_hints(&mut self, hints: Vec<KeyHint>) {
        self.key_hints = hints;
    }

    #[allow(dead_code)]
    pub fn update_conversation_cost(&mut self, cost: f64) {
        self.conversation_cost = cost;
        self.update_cost_display();
    }

    #[allow(dead_code)]
    pub fn update_session_cost(&mut self, cost: f64) {
        self.session_cost = cost;
        self.update_cost_display();
    }

    fn update_cost_display(&mut self) {
        if self.session_cost > 0.0 {
            self.cost_info = format!("Conv: ${:.4} | Total: ${:.4}", self.conversation_cost, self.session_cost);
        } else if self.conversation_cost > 0.0 {
            self.cost_info = format!("${:.4}", self.conversation_cost);
        } else {
            self.cost_info = DEFAULT_COST.to_string();
        }
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
        let available_width = area.width as usize;
        
        // Build status line sections based on available width
        let mut spans = Vec::new();
        
        // Status message with fixed width (highest priority)
        let truncated_status = if self.status_message.len() > STATUS_MESSAGE_WIDTH {
            format!("{}…", &self.status_message[..STATUS_MESSAGE_WIDTH.saturating_sub(1)])
        } else {
            format!("{:<width$}", self.status_message, width = STATUS_MESSAGE_WIDTH)
        };
        spans.push(Span::styled(truncated_status, theme.normal()));
        
        // Model info (high priority)
        if available_width > 60 {
            spans.push(Span::raw(SEPARATOR));
            spans.push(Span::styled(&self.model_info, theme.accent()));
        }
        
        // Cost info (medium priority)
        if available_width > 80 && self.cost_info != DEFAULT_COST {
            spans.push(Span::raw(SEPARATOR));
            spans.push(Span::styled(&self.cost_info, theme.warning()));
        }
        
        // Connection status (medium priority)
        if available_width > 100 {
            spans.push(Span::raw(SEPARATOR));
            spans.push(Span::styled(conn_symbol, conn_style));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                match &self.connection_status {
                    ConnectionStatus::Connected => CONNECTION_CONNECTED,
                    ConnectionStatus::Connecting => CONNECTION_CONNECTING,
                    ConnectionStatus::Disconnected => CONNECTION_DISCONNECTED,
                    ConnectionStatus::Error(err) => err,
                },
                theme.secondary()
            ));
        } else if available_width > 70 {
            // Just show the connection indicator symbol
            spans.push(Span::raw(SEPARATOR));
            spans.push(Span::styled(conn_symbol, conn_style));
        }
        
        // Key hints (low priority, shown on the right)
        if available_width > 120 && !self.key_hints.is_empty() {
            let hints_text = self.key_hints.iter()
                .map(|hint| format!("{}: {}", hint.key, hint.action))
                .collect::<Vec<_>>()
                .join(SEPARATOR);
            
            // Calculate space needed for hints
            let left_text_len: usize = spans.iter()
                .map(|span| span.content.chars().count())
                .sum();
            
            let hints_len = hints_text.chars().count();
            
            if left_text_len + hints_len + 5 < available_width {
                // Add spacing to push hints to the right
                let padding = available_width - left_text_len - hints_len - 5;
                spans.push(Span::raw(" ".repeat(padding)));
                spans.push(Span::raw(SEPARATOR));
                spans.push(Span::styled(hints_text, theme.secondary()));
            }
        }
        
        let status_line = Line::from(spans);

        let paragraph = Paragraph::new(status_line)
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