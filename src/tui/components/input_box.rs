use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use tui_input::{backend::crossterm::EventHandler, Input};

use crate::tui::{components::Component, Event, Theme};

#[derive(Debug, Clone)]
pub struct InputBox {
    input: Input,
    is_focused: bool,
    placeholder: String,
    is_multiline_mode: bool,
    lines: Vec<String>,
}

impl InputBox {
    pub fn new() -> Self {
        Self {
            input: Input::default(),
            is_focused: false,
            placeholder: "Type your message... (Enter: Send, Shift+Enter: New line)".to_string(),
            is_multiline_mode: false,
            lines: Vec::new(),
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

    pub fn clear(&mut self) {
        self.input.reset();
        self.lines.clear();
        self.is_multiline_mode = false;
    }

    pub fn get_content(&self) -> String {
        if self.is_multiline_mode {
            self.lines.join("\n")
        } else {
            self.input.value().to_string()
        }
    }

    pub fn set_content(&mut self, content: String) {
        self.input = Input::new(content);
        self.lines.clear();
        self.is_multiline_mode = false;
    }

    pub fn get_cursor_position(&self) -> usize {
        self.input.visual_cursor()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        if self.is_multiline_mode {
            self.lines.is_empty() || self.lines.iter().all(|line| line.trim().is_empty())
        } else {
            self.input.value().trim().is_empty()
        }
    }

    fn toggle_multiline_mode(&mut self) {
        if self.is_multiline_mode {
            // Switch back to single line mode
            let content = self.lines.join(" ");
            self.input = Input::new(content);
            self.lines.clear();
            self.is_multiline_mode = false;
        } else {
            // Switch to multiline mode
            if !self.input.value().is_empty() {
                self.lines = vec![self.input.value().to_string()];
            }
            self.input.reset();
            self.is_multiline_mode = true;
        }
    }

    fn handle_multiline_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Enter => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // Add new line
                    self.lines.push(String::new());
                } else {
                    // Send message
                    return false; // Let parent handle send
                }
                true
            }
            KeyCode::Backspace => {
                if self.lines.is_empty() {
                    return true;
                }
                
                let should_remove_line = {
                    let last_line = self.lines.last().unwrap();
                    last_line.is_empty() && self.lines.len() > 1
                };
                
                if should_remove_line {
                    self.lines.pop();
                } else if let Some(last_line) = self.lines.last_mut() {
                    last_line.pop();
                }
                true
            }
            KeyCode::Char(c) => {
                if let Some(last_line) = self.lines.last_mut() {
                    last_line.push(c);
                } else {
                    self.lines.push(c.to_string());
                }
                true
            }
            _ => false,
        }
    }
}

impl Component for InputBox {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let border_style = if self.is_focused {
            theme.accent()
        } else {
            theme.border()
        };

        let title = if self.is_multiline_mode {
            " Message (Multiline Mode) "
        } else {
            " Message "
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(title);

        if self.is_multiline_mode {
            let content = if self.lines.is_empty() {
                vec![Line::from(Span::styled(&self.placeholder, theme.secondary()))]
            } else {
                self.lines
                    .iter()
                    .map(|line| Line::from(line.clone()))
                    .collect()
            };

            let paragraph = Paragraph::new(content)
                .block(block)
                .wrap(Wrap { trim: false })
                .style(theme.normal());

            frame.render_widget(paragraph, area);
        } else {
            // Calculate available width for text (excluding borders)
            let available_width = area.width.saturating_sub(2) as usize;
            
            if self.input.value().is_empty() {
                let content = Line::from(Span::styled(&self.placeholder, theme.secondary()));
                let paragraph = Paragraph::new(content)
                    .block(block)
                    .style(theme.normal());
                frame.render_widget(paragraph, area);
            } else {
                let text = self.input.value();
                let cursor_pos = self.input.visual_cursor();
                
                // Calculate horizontal scroll offset
                let scroll_offset = if cursor_pos >= available_width {
                    cursor_pos.saturating_sub(available_width) + 1
                } else {
                    0
                };
                
                // Get the visible portion of the text using character indices for UTF-8 safety
                let chars: Vec<char> = text.chars().collect();
                let visible_text = if scroll_offset > 0 {
                    let start = scroll_offset.min(chars.len());
                    let end = (start + available_width).min(chars.len());
                    chars[start..end].iter().collect::<String>()
                } else {
                    let end = available_width.min(chars.len());
                    chars[..end].iter().collect::<String>()
                };
                
                let cursor_in_view = cursor_pos.saturating_sub(scroll_offset);
                
                let content = Line::from(visible_text);
                let paragraph = Paragraph::new(content)
                    .block(block)
                    .style(theme.normal());
                frame.render_widget(paragraph, area);

                if self.is_focused {
                    // Show cursor at the correct scrolled position
                    let cursor_x = area.x + 1 + cursor_in_view as u16;
                    let cursor_y = area.y + 1;
                    if cursor_x < area.x + area.width - 1 {
                        frame.set_cursor(cursor_x, cursor_y);
                    }
                }
            }
        }
    }

    fn handle_event(&mut self, event: &Event) -> bool {
        if !self.is_focused {
            return false;
        }

        match event {
            Event::Key(key) => {
                if self.is_multiline_mode {
                    return self.handle_multiline_key(*key);
                }

                match key.code {
                    KeyCode::Enter => {
                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                            self.toggle_multiline_mode();
                            true
                        } else {
                            false // Let parent handle send
                        }
                    }
                    KeyCode::Tab => {
                        self.toggle_multiline_mode();
                        true
                    }
                    _ => {
                        // Only consume regular input events, not control combinations
                        // Let global hotkeys (Ctrl+1, Ctrl+2, etc.) pass through
                        if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
                            false // Don't consume control/alt combinations
                        } else {
                            // Let tui-input handle regular input
                            self.input.handle_event(&crossterm::event::Event::Key(*key));
                            true
                        }
                    }
                }
            }
            _ => false,
        }
    }

    fn title(&self) -> &str {
        "InputBox"
    }
}