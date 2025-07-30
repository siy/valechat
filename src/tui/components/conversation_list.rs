use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

use crate::tui::{components::Component, Event, Theme};

#[derive(Clone, Debug)]
pub struct ConversationItem {
    pub id: String,
    pub title: String,
    pub message_count: usize,
    pub updated_at: i64,
    pub total_cost: f64,
}

pub struct ConversationList {
    conversations: Vec<ConversationItem>,
    state: ListState,
    is_focused: bool,
}

impl ConversationList {
    pub fn new() -> Self {
        let mut state = ListState::default();
        state.select(Some(0));
        
        Self {
            conversations: Vec::new(),
            state,
            is_focused: false,
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

    pub fn set_conversations(&mut self, conversations: Vec<ConversationItem>) {
        let selected = self.state.selected().unwrap_or(0);
        self.conversations = conversations;
        
        if !self.conversations.is_empty() {
            let new_selected = selected.min(self.conversations.len().saturating_sub(1));
            self.state.select(Some(new_selected));
        } else {
            self.state.select(None);
        }
    }

    pub fn get_selected_conversation(&self) -> Option<&ConversationItem> {
        self.state.selected()
            .and_then(|i| self.conversations.get(i))
    }

    #[allow(dead_code)]
    pub fn get_selected_index(&self) -> Option<usize> {
        self.state.selected()
    }

    pub fn next(&mut self) {
        if self.conversations.is_empty() {
            return;
        }
        
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.conversations.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        if self.conversations.is_empty() {
            return;
        }
        
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.conversations.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn format_time_ago(timestamp: i64) -> String {
        let now = chrono::Utc::now().timestamp();
        let diff = now - timestamp;
        
        if diff < 60 {
            "now".to_string()
        } else if diff < 3600 {
            format!("{}m", diff / 60)
        } else if diff < 86400 {
            format!("{}h", diff / 3600)
        } else {
            format!("{}d", diff / 86400)
        }
    }

    fn format_cost(cost: f64) -> String {
        if cost < 0.01 {
            "<$0.01".to_string()
        } else {
            format!("${:.2}", cost)
        }
    }

    pub fn add_conversation(&mut self, conversation: ConversationItem) {
        self.conversations.insert(0, conversation); // Add at beginning
        if self.conversations.len() == 1 {
            self.state.select(Some(0));
        }
    }

    pub fn select_conversation(&mut self, conversation_id: &str) {
        if let Some(index) = self.conversations.iter().position(|c| c.id == conversation_id) {
            self.state.select(Some(index));
        }
    }
}

impl Component for ConversationList {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let border_style = if self.is_focused {
            theme.accent()
        } else {
            theme.border()
        };

        let items: Vec<ListItem> = self.conversations
            .iter()
            .map(|conv| {
                let time_ago = Self::format_time_ago(conv.updated_at);
                let cost = Self::format_cost(conv.total_cost);
                
                // Truncate title to fit
                let max_title_len = area.width.saturating_sub(15) as usize; // Leave space for metadata
                let title = if conv.title.len() > max_title_len {
                    format!("{}...", &conv.title[..max_title_len.saturating_sub(3)])
                } else {
                    conv.title.clone()
                };

                let line = Line::from(vec![
                    Span::styled(title, theme.normal()),
                    Span::raw(" "),
                    Span::styled(format!("({})", conv.message_count), theme.secondary()),
                    Span::raw(" "),
                    Span::styled(time_ago, theme.secondary()),
                    Span::raw(" "),
                    Span::styled(cost, theme.warning()),
                ]);

                ListItem::new(line)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(border_style)
                    .title(" Conversations ")
            )
            .highlight_style(theme.selected())
            .highlight_symbol("► ");

        frame.render_stateful_widget(list, area, &mut self.state);
    }

    fn handle_event(&mut self, event: &Event) -> bool {
        if !self.is_focused {
            return false;
        }

        match event {
            Event::Key(KeyEvent { code, .. }) => {
                match code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.previous();
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.next();
                        true
                    }
                    KeyCode::Enter => {
                        // Signal that a conversation was selected
                        false // Let parent handle conversation selection
                    }
                    KeyCode::Delete | KeyCode::Char('d') => {
                        // Signal conversation deletion request
                        false // Let parent handle deletion
                    }
                    KeyCode::Char('r') => {
                        // Signal conversation rename request
                        false // Let parent handle rename
                    }
                    KeyCode::Char('n') => {
                        // Signal new conversation request
                        false // Let parent handle new conversation
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn title(&self) -> &str {
        "ConversationList"
    }
}