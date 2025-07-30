use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, MouseEvent};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::timeout;

#[derive(Clone, Debug)]
pub enum Event {
    /// Terminal tick event
    Tick,
    /// Key press event
    Key(KeyEvent),
    /// Mouse event
    #[allow(dead_code)]
    Mouse(MouseEvent),
    /// Terminal resize event
    #[allow(dead_code)]
    Resize(u16, u16),
    /// Application-specific events
    SendMessage(String),
    #[allow(dead_code)]
    MessageReceived(String, String), // conversation_id, content
    #[allow(dead_code)]
    ConversationCreated(String, String), // id, title
    #[allow(dead_code)]
    ConversationDeleted(String), // id
    #[allow(dead_code)]
    ConversationRenamed(String, String), // id, new_title
    #[allow(dead_code)]
    Error(String),
    #[allow(dead_code)]
    StatusUpdate(String),
}

pub struct EventHandler {
    sender: mpsc::UnboundedSender<Event>,
    receiver: mpsc::UnboundedReceiver<Event>,
    last_tick: Instant,
    tick_rate: Duration,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        Self {
            sender,
            receiver,
            last_tick: Instant::now(),
            tick_rate,
        }
    }

    pub fn sender(&self) -> mpsc::UnboundedSender<Event> {
        self.sender.clone()
    }

    pub async fn next(&mut self) -> Option<Event> {
        let _timeout_duration = self.tick_rate.saturating_sub(self.last_tick.elapsed());
        
        // Try to receive an event from the channel first
        if let Ok(event) = timeout(Duration::from_millis(10), self.receiver.recv()).await {
            return event;
        }

        // Check for terminal events
        if event::poll(Duration::from_millis(0)).unwrap_or(false) {
            match event::read() {
                Ok(CrosstermEvent::Key(key)) => return Some(Event::Key(key)),
                Ok(CrosstermEvent::Mouse(mouse)) => return Some(Event::Mouse(mouse)),
                Ok(CrosstermEvent::Resize(w, h)) => return Some(Event::Resize(w, h)),
                _ => {}
            }
        }

        // Send tick event if enough time has passed
        if self.last_tick.elapsed() >= self.tick_rate {
            self.last_tick = Instant::now();
            return Some(Event::Tick);
        }

        // Small delay to prevent busy waiting
        tokio::time::sleep(Duration::from_millis(10)).await;
        None
    }

    #[allow(dead_code)]
    pub fn send(&self, event: Event) {
        let _ = self.sender.send(event);
    }
}