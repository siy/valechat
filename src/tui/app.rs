use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::tui::{
    components::{
        chat_view::{ChatMessage, ChatView, MessageRole},
        conversation_list::{ConversationItem, ConversationList},
        Component, HelpPopup, InputBox, StatusBar
    },
    Event, Theme,
};
use valechat::{app::AppState, chat::{types::{ChatSession, MessageRole as ChatMessageRole}}};

#[derive(Clone, Debug, PartialEq)]
pub enum FocusedPanel {
    ConversationList,
    ChatView,
    InputBox,
}

pub struct App {
    // Components
    conversation_list: ConversationList,
    chat_view: ChatView,
    input_box: InputBox,
    status_bar: StatusBar,
    help_popup: HelpPopup,
    
    // State
    focused_panel: FocusedPanel,
    theme: Theme,
    should_quit: bool,
    
    // Rename mode state
    rename_mode: Option<RenameMode>,
    
    // Backend integration
    app_state: Arc<AppState>,
    event_sender: mpsc::UnboundedSender<Event>,
    
    // Preferences
    preferred_provider: Option<String>,
    #[allow(dead_code)]
    preferred_model: Option<String>,
}

#[derive(Debug, Clone)]
struct RenameMode {
    conversation_id: String,
    input_box: InputBox,
}

impl App {
    pub fn new(
        app_state: Arc<AppState>, 
        event_sender: mpsc::UnboundedSender<Event>,
        preferred_provider: Option<String>,
        preferred_model: Option<String>,
    ) -> Self {
        let mut app = Self {
            conversation_list: ConversationList::new(),
            chat_view: ChatView::new(),
            input_box: InputBox::new(),
            status_bar: StatusBar::new(),
            help_popup: HelpPopup::new(),
            focused_panel: FocusedPanel::ConversationList,
            theme: Theme::dark(),
            should_quit: false,
            rename_mode: None,
            app_state,
            event_sender,
            preferred_provider,
            preferred_model,
        };

        // Set initial focus
        app.update_focus();
        app
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub async fn handle_event(&mut self, event: Event) {
        // Help popup takes priority
        if self.help_popup.is_visible() && self.help_popup.handle_event(&event) {
            return;
        }

        // Rename mode takes priority over normal panel handling
        if self.rename_mode.is_some() {
            if let Event::Key(key) = event {
                if self.handle_rename_mode_keys(key).await {
                    return;
                }
            }
        }

        match event {
            Event::Key(key) => {
                if self.handle_global_keys(key) {
                    return;
                }
                self.handle_panel_specific_keys(key).await;
            }
            Event::SendMessage(content) => {
                self.send_message(content).await;
            }
            Event::MessageReceived(conversation_id, content) => {
                self.handle_message_received(conversation_id, content).await;
            }
            Event::ConversationCreated(id, title) => {
                self.handle_conversation_created(id, title).await;
            }
            Event::ConversationDeleted(id) => {
                self.handle_conversation_deleted(id).await;
            }
            Event::ConversationRenamed(id, new_title) => {
                self.handle_conversation_renamed(id, new_title).await;
            }
            Event::Error(error) => {
                self.status_bar.set_status(format!("Error: {}", error));
            }
            Event::StatusUpdate(status) => {
                self.status_bar.set_status(status);
            }
            _ => {}
        }
    }

    fn handle_global_keys(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) |
            (KeyCode::Char('q'), KeyModifiers::CONTROL) => {
                self.should_quit = true;
                true
            }
            (KeyCode::F(1), _) => {
                self.help_popup.toggle();
                true
            }
            (KeyCode::Tab, KeyModifiers::NONE) => {
                self.next_panel();
                true
            }
            (KeyCode::BackTab, _) => {
                self.previous_panel();
                true
            }
            _ => false,
        }
    }

    async fn handle_panel_specific_keys(&mut self, key: KeyEvent) {
        // First, let the focused component handle the event
        let handled = match self.focused_panel {
            FocusedPanel::ConversationList => {
                self.conversation_list.handle_event(&Event::Key(key))
            }
            FocusedPanel::ChatView => {
                self.chat_view.handle_event(&Event::Key(key))
            }
            FocusedPanel::InputBox => {
                self.input_box.handle_event(&Event::Key(key))
            }
        };

        if handled {
            return;
        }

        // Handle actions that weren't handled by components
        match self.focused_panel {
            FocusedPanel::ConversationList => {
                match key.code {
                    KeyCode::Enter => {
                        if let Some(conversation) = self.conversation_list.get_selected_conversation() {
                            self.load_conversation(conversation.id.clone()).await;
                        }
                    }
                    KeyCode::Char('n') => {
                        self.create_new_conversation().await;
                    }
                    KeyCode::Delete | KeyCode::Char('d') => {
                        if let Some(conversation) = self.conversation_list.get_selected_conversation() {
                            self.delete_conversation(conversation.id.clone()).await;
                        }
                    }
                    KeyCode::Char('r') => {
                        if let Some(conversation) = self.conversation_list.get_selected_conversation() {
                            self.rename_conversation(conversation.id.clone()).await;
                        }
                    }
                    _ => {}
                }
            }
            FocusedPanel::InputBox => {
                if key.code == KeyCode::Enter && !key.modifiers.contains(KeyModifiers::SHIFT) {
                    let content = self.input_box.get_content();
                    if !content.trim().is_empty() {
                        self.input_box.clear();
                        let _ = self.event_sender.send(Event::SendMessage(content));
                    }
                }
            }
            _ => {}
        }
    }

    fn next_panel(&mut self) {
        self.focused_panel = match self.focused_panel {
            FocusedPanel::ConversationList => FocusedPanel::ChatView,
            FocusedPanel::ChatView => FocusedPanel::InputBox,
            FocusedPanel::InputBox => FocusedPanel::ConversationList,
        };
        self.update_focus();
    }

    fn previous_panel(&mut self) {
        self.focused_panel = match self.focused_panel {
            FocusedPanel::ConversationList => FocusedPanel::InputBox,
            FocusedPanel::ChatView => FocusedPanel::ConversationList,
            FocusedPanel::InputBox => FocusedPanel::ChatView,
        };
        self.update_focus();
    }

    fn update_focus(&mut self) {
        // Unfocus all components
        self.conversation_list.unfocus();
        self.chat_view.unfocus();
        self.input_box.unfocus();

        // Focus the current panel
        match self.focused_panel {
            FocusedPanel::ConversationList => self.conversation_list.focus(),
            FocusedPanel::ChatView => self.chat_view.focus(),
            FocusedPanel::InputBox => self.input_box.focus(),
        }
    }

    async fn load_conversations(&mut self) {
        match self.app_state.get_conversation_repo().list_conversations(None, None).await {
            Ok(conversations) => {
                let mut items: Vec<ConversationItem> = Vec::new();
                for conv in conversations {
                    // Get actual message count
                    let message_count = match self.app_state.get_message_repo().get_messages(&conv.id).await {
                        Ok(messages) => messages.len(),
                        Err(_) => 0,
                    };
                    
                    // For now, use a simplified cost calculation
                    // TODO: Implement proper conversation-specific cost tracking
                    let total_cost = 0.0;
                    
                    items.push(ConversationItem {
                        id: conv.id,
                        title: conv.title,
                        message_count,
                        updated_at: conv.updated_at.timestamp(),
                        total_cost,
                    });
                }

                self.conversation_list.set_conversations(items);
                self.status_bar.set_status("Conversations loaded".to_string());
            }
            Err(e) => {
                self.status_bar.set_status(format!("Error loading conversations: {}", e));
            }
        }
    }

    async fn load_conversation(&mut self, conversation_id: String) {
        match self.app_state.get_conversation_repo().get_conversation(&conversation_id).await {
            Ok(Some(conversation)) => {
                self.chat_view.set_conversation_title(conversation.title);
                self.chat_view.clear_messages();
                
                // Load messages for this conversation
                match self.app_state.get_message_repo().get_messages(&conversation_id).await {
                    Ok(messages) => {
                        let message_count = messages.len();
                        for message in messages {
                            let role = if message.role == ChatMessageRole::User { 
                                MessageRole::User 
                            } else { 
                                MessageRole::Assistant 
                            };
                            
                            let content = if let Some(text) = message.content.get_text() {
                                text.to_string()
                            } else {
                                "[Non-text content]".to_string()
                            };
                            
                            let chat_message = ChatMessage {
                                id: message.id,
                                role,
                                content,
                                timestamp: message.timestamp.timestamp(),
                                cost: None, // Will be populated from database if available
                                input_tokens: None, // Will be populated from database if available  
                                output_tokens: None, // Will be populated from database if available  
                                model_used: None, // Will be populated from database if available
                            };
                            self.chat_view.add_message(chat_message);
                        }
                        self.status_bar.set_status(format!("Loaded {} messages", message_count));
                    }
                    Err(e) => {
                        self.status_bar.set_status(format!("Error loading messages: {}", e));
                    }
                }
            }
            Ok(None) => {
                self.status_bar.set_status("Conversation not found".to_string());
            }
            Err(e) => {
                self.status_bar.set_status(format!("Error loading conversation: {}", e));
            }
        }
    }

    async fn create_new_conversation(&mut self) {
        self.status_bar.set_status("Creating new conversation...".to_string());
        
        // Create a new chat session
        let new_session = ChatSession::new(
            "New Conversation",
            "openai", // Default provider
            "gpt-3.5-turbo" // Default model
        );
        
        match self.app_state.get_conversation_repo().create_conversation(&new_session).await {
            Ok(()) => {
                // Add to conversation list
                let item = ConversationItem {
                    id: new_session.id.clone(),
                    title: new_session.title.clone(),
                    message_count: 0,
                    updated_at: new_session.updated_at.timestamp(),
                    total_cost: 0.0,
                };
                
                self.conversation_list.add_conversation(item);
                self.conversation_list.select_conversation(&new_session.id);
                
                // Clear chat view for new conversation
                self.chat_view.clear_messages();
                self.chat_view.set_conversation_title(new_session.title);
                
                self.status_bar.set_status("New conversation created".to_string());
            }
            Err(e) => {
                self.status_bar.set_status(format!("Error creating conversation: {}", e));
            }
        }
    }

    async fn delete_conversation(&mut self, conversation_id: String) {
        match self.app_state.get_conversation_repo().delete_conversation(&conversation_id).await {
            Ok(_) => {
                self.load_conversations().await;
                self.status_bar.set_status("Conversation deleted".to_string());
            }
            Err(e) => {
                self.status_bar.set_status(format!("Error deleting conversation: {}", e));
            }
        }
    }

    async fn send_message(&mut self, content: String) {
        // If no conversation is selected, create a new one first
        if self.conversation_list.get_selected_conversation().is_none() {
            self.create_new_conversation().await;
        }
        
        if let Some(current_conversation) = self.conversation_list.get_selected_conversation() {
            self.status_bar.set_status("Sending message...".to_string());
            
            // Add user message to chat view
            let user_message = ChatMessage {
                id: uuid::Uuid::new_v4().to_string(),
                role: MessageRole::User,
                content: content.clone(),
                timestamp: chrono::Utc::now().timestamp(),
                cost: None,
                input_tokens: Some(0),
                output_tokens: Some(0),
                model_used: Some("user".to_string()),
            };
            self.chat_view.add_message(user_message);
            
            // Send message through provider
            match self.app_state.send_message_with_provider(
                &current_conversation.id, 
                &content,
                self.preferred_provider.as_deref()
            ).await {
                Ok(response) => {
                    // Add assistant response to chat view
                    let assistant_message = ChatMessage {
                        id: uuid::Uuid::new_v4().to_string(),
                        role: MessageRole::Assistant,
                        content: response,
                        timestamp: chrono::Utc::now().timestamp(),
                        cost: None,
                        input_tokens: Some(0),
                        output_tokens: Some(0),
                        model_used: Some("assistant".to_string()),
                    };
                    self.chat_view.add_message(assistant_message);
                    self.status_bar.set_status("Message sent successfully".to_string());
                }
                Err(e) => {
                    self.status_bar.set_status(format!("Error sending message: {}", e));
                }
            }
        } else {
            self.status_bar.set_status("No conversation selected".to_string());
        }
    }

    async fn handle_message_received(&mut self, conversation_id: String, content: String) {
        // Add received message to chat view
        let message = ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            content,
            timestamp: chrono::Utc::now().timestamp(),
            cost: None,
            input_tokens: None,
            output_tokens: None,
            model_used: None,
        };
        
        // Only add if this is the currently selected conversation
        if let Some(current_conversation) = self.conversation_list.get_selected_conversation() {
            if current_conversation.id == conversation_id {
                self.chat_view.add_message(message);
            }
        }
        
        self.status_bar.set_status("Message received".to_string());
    }

    async fn handle_conversation_created(&mut self, _id: String, _title: String) {
        self.load_conversations().await;
    }

    async fn handle_conversation_deleted(&mut self, _id: String) {
        self.load_conversations().await;
    }

    async fn rename_conversation(&mut self, conversation_id: String) {
        // Get the current conversation to pre-fill the input with its current title
        if let Ok(Some(conversation)) = self.app_state.get_conversation_repo().get_conversation(&conversation_id).await {
            let mut input_box = InputBox::new();
            input_box.set_content(conversation.title);
            input_box.focus(); // Make sure it's focused for input
            
            self.rename_mode = Some(RenameMode {
                conversation_id,
                input_box,
            });
            
            self.status_bar.set_status("Enter new conversation name (Enter to save, Esc to cancel)".to_string());
        } else {
            self.status_bar.set_status("Error: Could not load conversation for renaming".to_string());
        }
    }

    async fn handle_rename_mode_keys(&mut self, key: KeyEvent) -> bool {
        if let Some(ref mut rename_mode) = self.rename_mode {
            match key.code {
                KeyCode::Enter => {
                    let new_title = rename_mode.input_box.get_content();
                    let conversation_id = rename_mode.conversation_id.clone();
                    
                    // Clear rename mode first
                    self.rename_mode = None;
                    
                    // Perform the actual rename
                    self.perform_rename(conversation_id, new_title).await;
                    true
                }
                KeyCode::Esc => {
                    // Cancel rename
                    self.rename_mode = None;
                    self.status_bar.set_status("Rename cancelled".to_string());
                    true
                }
                _ => {
                    // Pass other keys to the input box
                    rename_mode.input_box.handle_event(&Event::Key(key));
                    true
                }
            }
        } else {
            false
        }
    }

    async fn perform_rename(&mut self, conversation_id: String, new_title: String) {
        if new_title.trim().is_empty() {
            self.status_bar.set_status("Error: Conversation name cannot be empty".to_string());
            return;
        }

        match self.app_state.get_conversation_repo().update_conversation_title(&conversation_id, &new_title).await {
            Ok(_) => {
                // Reload conversation list to show updated title
                self.load_conversations().await;
                
                // If this conversation is currently loaded in chat view, update its title too
                if let Some(current_conversation) = self.conversation_list.get_selected_conversation() {
                    if current_conversation.id == conversation_id {
                        self.chat_view.set_conversation_title(new_title.clone());
                    }
                }
                
                self.status_bar.set_status(format!("Conversation renamed to '{}'", new_title));
            }
            Err(e) => {
                self.status_bar.set_status(format!("Error renaming conversation: {}", e));
            }
        }
    }

    async fn handle_conversation_renamed(&mut self, _id: String, _new_title: String) {
        self.load_conversations().await;
    }

    pub async fn initialize(&mut self) {
        self.load_conversations().await;
        self.status_bar.set_status("ValeChat initialized".to_string());
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),      // Main content
                Constraint::Length(1),   // Status bar
            ])
            .split(frame.size());

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(30),  // Sidebar
                Constraint::Min(1),      // Main area
            ])
            .split(chunks[0]);

        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),      // Chat view
                Constraint::Length(3),   // Input box
            ])
            .split(main_chunks[1]);

        // Render components
        self.conversation_list.render(frame, main_chunks[0], &self.theme);
        self.chat_view.render(frame, right_chunks[0], &self.theme);
        self.input_box.render(frame, right_chunks[1], &self.theme);
        self.status_bar.render(frame, chunks[1], &self.theme);

        // Render rename dialog if in rename mode
        if self.rename_mode.is_some() {
            self.render_rename_dialog(frame, main_chunks[0]);
        }

        // Render help popup last (on top)
        self.help_popup.render(frame, frame.size(), &self.theme);
    }

    fn render_rename_dialog(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        use ratatui::widgets::{Block, Borders, Clear, Paragraph};
        use ratatui::layout::Margin;
        use ratatui::text::{Line, Span};

        if let Some(ref mut rename_mode) = self.rename_mode {
            // Create a centered area for the rename dialog
            let dialog_area = ratatui::layout::Rect {
                x: area.x + 2,
                y: area.y + area.height / 2,
                width: area.width.saturating_sub(4),
                height: 3,
            };

            // Clear the background
            frame.render_widget(Clear, dialog_area);

            // Render the dialog border and title
            let dialog_block = Block::default()
                .borders(Borders::ALL)
                .title(" Rename Conversation ")
                .border_style(self.theme.accent());

            let input_area = dialog_area.inner(&Margin {
                vertical: 1,
                horizontal: 1,
            });

            frame.render_widget(dialog_block, dialog_area);

            // Manually render the input content without InputBox's own border
            let text = rename_mode.input_box.get_content();
            let cursor_pos = rename_mode.input_box.get_cursor_position();
            let available_width = input_area.width as usize;
            
            let (visible_text, cursor_in_view) = if text.is_empty() {
                (Line::from(Span::styled("Enter conversation name...", self.theme.secondary())), 0)
            } else {
                // Handle horizontal scrolling if text is too long
                let scroll_offset = if cursor_pos >= available_width {
                    cursor_pos.saturating_sub(available_width) + 1
                } else {
                    0
                };
                
                let chars: Vec<char> = text.chars().collect();
                let visible_chars = if scroll_offset > 0 {
                    let start = scroll_offset.min(chars.len());
                    let end = (start + available_width).min(chars.len());
                    chars[start..end].iter().collect::<String>()
                } else {
                    let end = available_width.min(chars.len());
                    chars[..end].iter().collect::<String>()
                };
                
                let cursor_in_view = cursor_pos.saturating_sub(scroll_offset);
                
                (Line::from(Span::styled(visible_chars, self.theme.normal())), cursor_in_view)
            };

            let paragraph = Paragraph::new(visible_text)
                .style(self.theme.normal());

            frame.render_widget(paragraph, input_area);

            // Show cursor at the correct position
            let cursor_x = input_area.x + (cursor_in_view as u16);
            let cursor_y = input_area.y;
            if cursor_x < input_area.x + input_area.width {
                frame.set_cursor(cursor_x, cursor_y);
            }
        }
    }
}