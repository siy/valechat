use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Debug)]
pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub border: Color,
    pub highlight: Color,
    pub secondary: Color,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            bg: Color::Rgb(40, 44, 52),       // Dark background
            fg: Color::Rgb(171, 178, 191),    // Light text
            accent: Color::Rgb(97, 175, 239), // Blue accent
            success: Color::Rgb(152, 195, 121), // Green
            warning: Color::Rgb(229, 192, 123), // Yellow
            error: Color::Rgb(224, 108, 117),   // Red
            border: Color::Rgb(92, 99, 112),    // Border gray
            highlight: Color::Rgb(86, 182, 194), // Cyan highlight
            secondary: Color::Rgb(130, 137, 151), // Secondary text
        }
    }

    #[allow(dead_code)]
    pub fn light() -> Self {
        Self {
            bg: Color::Rgb(250, 250, 250),    // Light background
            fg: Color::Rgb(60, 60, 60),       // Dark text
            accent: Color::Rgb(0, 122, 255),  // Blue accent
            success: Color::Rgb(40, 167, 69), // Green
            warning: Color::Rgb(255, 193, 7), // Yellow
            error: Color::Rgb(220, 53, 69),   // Red
            border: Color::Rgb(200, 200, 200), // Border gray
            highlight: Color::Rgb(23, 162, 184), // Cyan highlight
            secondary: Color::Rgb(108, 117, 125), // Secondary text
        }
    }

    #[allow(dead_code)]
    pub fn matrix() -> Self {
        Self {
            bg: Color::Black,
            fg: Color::Green,
            accent: Color::Rgb(0, 255, 0),
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            border: Color::Green,
            highlight: Color::Rgb(0, 255, 0),
            secondary: Color::Rgb(0, 150, 0),
        }
    }

    // Style helpers
    pub fn normal(&self) -> Style {
        Style::default().fg(self.fg).bg(self.bg)
    }

    pub fn accent(&self) -> Style {
        Style::default().fg(self.accent)
    }

    pub fn success(&self) -> Style {
        Style::default().fg(self.success)
    }

    pub fn warning(&self) -> Style {
        Style::default().fg(self.warning)
    }

    pub fn error(&self) -> Style {
        Style::default().fg(self.error)
    }

    pub fn highlight(&self) -> Style {
        Style::default().fg(self.highlight).add_modifier(Modifier::BOLD)
    }

    pub fn secondary(&self) -> Style {
        Style::default().fg(self.secondary)
    }

    pub fn border(&self) -> Style {
        Style::default().fg(self.border)
    }

    pub fn selected(&self) -> Style {
        Style::default().fg(self.bg).bg(self.accent)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}