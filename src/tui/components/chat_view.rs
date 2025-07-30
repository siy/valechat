use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::{components::Component, Event, Theme};

#[derive(Clone, Debug)]
pub struct ChatMessage {
    #[allow(dead_code)]
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub timestamp: i64,
    pub model_used: Option<String>,
    #[allow(dead_code)]
    pub input_tokens: Option<i32>,
    #[allow(dead_code)]
    pub output_tokens: Option<i32>,
    pub cost: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    #[allow(dead_code)]
    System,
}

pub struct ChatView {
    messages: Vec<ChatMessage>,
    is_focused: bool,
    auto_scroll: bool,
    conversation_title: String,
    scroll_offset: usize,
}

impl ChatView {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            is_focused: false,
            auto_scroll: true,
            conversation_title: "No conversation selected".to_string(),
            scroll_offset: 0,
        }
    }

    pub fn focus(&mut self) {
        self.is_focused = true;
    }

    pub fn unfocus(&mut self) {
        self.is_focused = false;
    }

    #[allow(dead_code)]
    pub fn is_focused(&self) -> bool {
        self.is_focused
    }

    pub fn set_conversation_title(&mut self, title: String) {
        self.conversation_title = title;
    }

    #[allow(dead_code)]
    pub fn set_messages(&mut self, messages: Vec<ChatMessage>) {
        self.messages = messages;
        self.scroll_to_bottom();
    }

    pub fn add_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
        if self.auto_scroll {
            self.scroll_to_bottom();
        }
    }

    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.scroll_offset = 0;
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
            self.auto_scroll = false;
        }
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset += 1;
        // Check if we're still at the bottom after scrolling - this will be handled in render
        // but we disable auto_scroll to prevent immediate jump back to bottom
        self.auto_scroll = false;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = usize::MAX; // Will be clamped in render
        self.auto_scroll = true;
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
        self.auto_scroll = false;
    }

    fn format_timestamp(timestamp: i64) -> String {
        let datetime = chrono::DateTime::from_timestamp(timestamp, 0)
            .unwrap_or_else(chrono::Utc::now);
        datetime.format("%H:%M").to_string()
    }

    fn get_role_indicator(role: &MessageRole, theme: &Theme) -> (String, Style) {
        match role {
            MessageRole::User => ("👤".to_string(), theme.accent()),
            MessageRole::Assistant => ("🤖".to_string(), theme.success()),
            MessageRole::System => ("⚙️".to_string(), theme.warning()),
        }
    }

    fn wrap_text(text: &str, width: usize) -> Vec<String> {
        if width < 10 {
            return vec![text.to_string()];
        }

        let mut lines = Vec::new();
        for line in text.lines() {
            if line.len() <= width {
                lines.push(line.to_string());
            } else {
                let mut current_line = String::new();
                for word in line.split_whitespace() {
                    if current_line.len() + word.len() < width {
                        if !current_line.is_empty() {
                            current_line.push(' ');
                        }
                        current_line.push_str(word);
                    } else {
                        if !current_line.is_empty() {
                            lines.push(current_line);
                        }
                        current_line = word.to_string();
                    }
                }
                if !current_line.is_empty() {
                    lines.push(current_line);
                }
            }
        }
        
        if lines.is_empty() {
            lines.push(String::new());
        }
        
        lines
    }
}

impl Component for ChatView {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let border_style = if self.is_focused {
            theme.accent()
        } else {
            theme.border()
        };

        if self.messages.is_empty() {
            let empty_message = Paragraph::new("No messages yet. Start a conversation!")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(border_style)
                        .title(format!(" {} ", self.conversation_title))
                )
                .alignment(Alignment::Center)
                .style(theme.secondary());

            frame.render_widget(empty_message, area);
            return;
        }

        let content_width = area.width.saturating_sub(4) as usize; // Account for borders and padding
        let content_height = area.height.saturating_sub(2) as usize; // Account for borders

        // Generate all display lines
        let mut all_lines: Vec<Line> = Vec::new();
        
        for message in &self.messages {
            let (role_icon, role_style) = Self::get_role_indicator(&message.role, theme);
            let timestamp = Self::format_timestamp(message.timestamp);
            
            // Create header line
            let mut header_spans = vec![
                Span::styled(role_icon, role_style),
                Span::raw(" "),
                Span::styled(timestamp, theme.secondary()),
            ];

            // Add model and cost info for assistant messages
            if message.role == MessageRole::Assistant {
                if let Some(model) = &message.model_used {
                    header_spans.push(Span::raw(" | "));
                    header_spans.push(Span::styled(model, theme.secondary()));
                }
                
                if let Some(cost) = &message.cost {
                    if let Ok(cost_float) = cost.parse::<f64>() {
                        if cost_float > 0.0 {
                            header_spans.push(Span::raw(" | "));
                            header_spans.push(Span::styled(format!("${}", cost), theme.warning()));
                        }
                    }
                }
            }

            all_lines.push(Line::from(header_spans));
            
            // Wrap message content and add to lines
            let wrapped_lines = Self::wrap_text(&message.content, content_width.saturating_sub(2));
            for line in wrapped_lines {
                all_lines.push(Line::from(vec![
                    Span::raw("  "), // Indent content
                    Span::styled(line, theme.normal()),
                ]));
            }

            // Add separator
            all_lines.push(Line::from(""));
        }

        // Calculate scrolling
        let total_lines = all_lines.len();
        
        // Handle auto-scroll and bounds checking
        let max_scroll = total_lines.saturating_sub(content_height);
        
        if self.auto_scroll || self.scroll_offset >= total_lines {
            // Auto-scroll to bottom or clamp to maximum
            self.scroll_offset = max_scroll;
        } else if self.scroll_offset + content_height > total_lines {
            // Clamp scroll offset when at bottom
            self.scroll_offset = max_scroll;
            // Re-enable auto_scroll if user scrolled to the very bottom
            if self.scroll_offset == max_scroll {
                self.auto_scroll = true;
            }
        }

        // Get visible lines
        let visible_lines: Vec<Line> = if total_lines <= content_height {
            // All lines fit, no scrolling needed
            all_lines
        } else {
            // Take the visible window of lines
            let start = self.scroll_offset;
            let end = (start + content_height).min(total_lines);
            all_lines[start..end].to_vec()
        };

        // Create paragraph with visible lines
        let paragraph = Paragraph::new(visible_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(border_style)
                    .title(format!(" {} ", self.conversation_title))
            );

        frame.render_widget(paragraph, area);
    }

    fn handle_event(&mut self, event: &Event) -> bool {
        if !self.is_focused {
            return false;
        }

        match event {
            Event::Key(KeyEvent { code, .. }) => {
                match code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.scroll_up();
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.scroll_down();
                        true
                    }
                    KeyCode::Home | KeyCode::Char('g') => {
                        self.scroll_to_top();
                        true
                    }
                    KeyCode::End | KeyCode::Char('G') => {
                        self.scroll_to_bottom();
                        true
                    }
                    KeyCode::PageUp => {
                        for _ in 0..10 {
                            self.scroll_up();
                        }
                        true
                    }
                    KeyCode::PageDown => {
                        for _ in 0..10 {
                            self.scroll_down();
                        }
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn title(&self) -> &str {
        "ChatView"
    }
}