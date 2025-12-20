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
        Self::catppuccin_frappe()
    }
}

impl Theme {
    pub fn catppuccin_frappe() -> Self {
        // Catppuccin Frappe Palette
        let _rosewater = Color::Rgb(242, 213, 207);
        let _flamingo = Color::Rgb(238, 190, 190);
        let _pink = Color::Rgb(244, 184, 228);
        let mauve = Color::Rgb(202, 158, 230);
        let red = Color::Rgb(231, 130, 132);
        let _maroon = Color::Rgb(234, 153, 156);
        let peach = Color::Rgb(239, 159, 118);
        let yellow = Color::Rgb(229, 200, 144);
        let _green = Color::Rgb(166, 209, 137);
        let teal = Color::Rgb(129, 200, 190);
        let _sky = Color::Rgb(153, 209, 219);
        let _sapphire = Color::Rgb(133, 193, 220);
        let _blue = Color::Rgb(140, 170, 238);
        let lavender = Color::Rgb(186, 187, 241);
        let text = Color::Rgb(198, 208, 245);
        let _subtext1 = Color::Rgb(181, 191, 226);
        let subtext0 = Color::Rgb(165, 173, 206);
        let _overlay2 = Color::Rgb(148, 156, 187);
        let overlay0 = Color::Rgb(115, 121, 148);
        let surface0 = Color::Rgb(65, 69, 89);
        // let base = Color::Rgb(48, 52, 70); // We use Reset for transparency

        Self {
            app_bg: Color::Reset, // Transparent background
            primary_fg: text,
            secondary_fg: subtext0,

            // Header
            header_bg: Color::Reset,
            header_fg: lavender,
            header_border: overlay0,

            // Footer
            status_bar_bg: surface0, // Slight contrast for bar
            status_bar_fg: text,

            // Chat
            // User: Teal theme
            user_bubble_bg: Color::Reset,
            user_bubble_fg: text,
            user_bubble_border: teal,

            // AI: Lavender theme
            ai_bubble_bg: Color::Reset,
            ai_bubble_fg: text,
            ai_bubble_border: lavender,

            // Tool: Peach/Yellow
            tool_bubble_bg: Color::Reset,
            tool_bubble_fg: peach,

            // Input
            input_border_normal: overlay0,
            input_border_active: mauve, // Active focus is Mauve
            input_border_error: red,

            // Modals
            modal_bg: Color::Reset,
            modal_border: yellow,
        }
    }

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
