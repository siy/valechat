pub mod chat_view;
pub mod conversation_list;
pub mod input_box;
pub mod status_bar;
pub mod help_popup;

use ratatui::{layout::Rect, Frame};
use crate::tui::{Event, Theme};

pub use input_box::InputBox;
pub use status_bar::StatusBar;
pub use help_popup::HelpPopup;

/// Base trait for all TUI components
pub trait Component {
    /// Render the component
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme);
    
    /// Handle input events
    fn handle_event(&mut self, event: &Event) -> bool;
    
    /// Update component state
    #[allow(dead_code)]
    fn update(&mut self) {}
    
    /// Get component title for debugging
    #[allow(dead_code)]
    fn title(&self) -> &str {
        "Component"
    }
}