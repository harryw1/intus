use ratatui::style::{Color, Style};

#[derive(Debug, Clone)]
pub struct Theme {
    pub app_bg: Color,
    pub primary_fg: Color,
    pub secondary_fg: Color,
    
    // Header
    pub header_bg: Color,
    pub header_fg: Color,
    pub header_border: Color,

    // Footer / Status Bar
    pub status_bar_bg: Color,
    pub status_bar_fg: Color,

    // Chat
    pub user_bubble_bg: Color,
    pub user_bubble_fg: Color,
    pub user_bubble_border: Color,
    pub ai_bubble_bg: Color,
    pub ai_bubble_fg: Color,
    pub ai_bubble_border: Color,
    pub tool_bubble_bg: Color,
    pub tool_bubble_fg: Color,

    // Input
    pub input_border_normal: Color,
    pub input_border_active: Color,
    pub input_border_error: Color,

    // Modals
    pub modal_bg: Color,
    pub modal_border: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::terminal()
    }
}

impl Theme {
    pub fn modern() -> Self {
        Self {
            app_bg: Color::Reset,
            primary_fg: Color::White,
            secondary_fg: Color::DarkGray,

            // Header: Clean Slate
            header_bg: Color::Reset, 
            header_fg: Color::Blue,
            header_border: Color::DarkGray,

            // Footer: Distinct status bar
            status_bar_bg: Color::DarkGray,
            status_bar_fg: Color::White,

            // Chat: Minimalist
            // User: Soft Blue/Slate style
            user_bubble_bg: Color::Reset,
            user_bubble_fg: Color::Cyan, // Brighter for readability
            user_bubble_border: Color::Blue, 

            // AI: Clean White/Gray
            ai_bubble_bg: Color::Reset,
            ai_bubble_fg: Color::White, // Standard text
            ai_bubble_border: Color::DarkGray,

            // Tools: Distinct
            tool_bubble_bg: Color::Reset,
            tool_bubble_fg: Color::Yellow,

            // Input
            input_border_normal: Color::DarkGray,
            input_border_active: Color::Blue,
            input_border_error: Color::Red,

            // Modals
            modal_bg: Color::Reset,
            modal_border: Color::Yellow,
        }
    }

    pub fn terminal() -> Self {
        Self {
            app_bg: Color::Reset,
            primary_fg: Color::Green, // Classic Terminal Green
            secondary_fg: Color::DarkGray,

            // Header: Boxed
            header_bg: Color::Reset, 
            header_fg: Color::Green,
            header_border: Color::Green,

            // Footer: Simple
            status_bar_bg: Color::Reset,
            status_bar_fg: Color::Cyan, 

            // Chat: High Contrast
            // User: Cyan
            user_bubble_bg: Color::Reset,
            user_bubble_fg: Color::Cyan, 
            user_bubble_border: Color::Cyan, 

            // AI: Green
            ai_bubble_bg: Color::Reset,
            ai_bubble_fg: Color::Green, 
            ai_bubble_border: Color::Green,

            // Tools: Magenta/Yellow
            tool_bubble_bg: Color::Reset,
            tool_bubble_fg: Color::Magenta,

            // Input
            input_border_normal: Color::Green,
            input_border_active: Color::Cyan,
            input_border_error: Color::Red,

            // Modals
            modal_bg: Color::Reset,
            modal_border: Color::Green,
        }
    }

    // Helper to get a style for user text
    pub fn user_text(&self) -> Style {
        Style::default().fg(self.user_bubble_fg)
    }

    // Helper to get a style for AI text
    pub fn ai_text(&self) -> Style {
        Style::default().fg(self.ai_bubble_fg)
    }
    
    // Helper to get status bar style
    pub fn status_bar(&self) -> Style {
        Style::default().bg(self.status_bar_bg).fg(self.status_bar_fg)
    }
}
