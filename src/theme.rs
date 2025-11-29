use ratatui::style::{Color, Style};

/// Theme configuration for the editor
#[derive(Clone, Debug)]
pub struct Theme {
    /// Background color for the editor
    pub background: Color,

    /// Foreground (text) color for the status bar
    pub status_bar_fg: Color,

    /// Background color for the status bar
    pub status_bar_bg: Color,

    /// Color for the current file name in the status bar
    pub filename_color: Color,

    /// Cursor color (used when rendering the terminal cursor)
    pub cursor_color: Color,

    /// Foreground color for active selection
    pub selection_fg: Color,

    /// Background color for active selection
    pub selection_bg: Color,

    /// Foreground color for highlighted text (InlineStyle::Highlight)
    pub highlight_fg: Color,

    /// Background color for highlighted text (InlineStyle::Highlight)
    pub highlight_bg: Color,

    /// Color for links
    pub link_color: Color,

    /// Foreground color for reveal tags
    pub reveal_tag_fg: Color,

    /// Background color for reveal tags
    pub reveal_tag_bg: Color,

    /// Foreground color for the scrollbar knob
    pub scrollbar_knob_fg: Color,

    /// Background color for the scrollbar knob
    pub scrollbar_knob_bg: Color,

    /// Foreground color for the scrollbar track
    pub scrollbar_track_fg: Color,

    /// Background color for the scrollbar track
    pub scrollbar_track_bg: Color,

    /// Foreground color for menu items
    pub menu_fg: Color,

    /// Background color for menu
    pub menu_bg: Color,

    /// Foreground color for disabled menu items
    pub menu_disabled_fg: Color,

    /// Foreground color for selected menu entry
    pub menu_selected_fg: Color,

    /// Background color for selected menu entry
    pub menu_selected_bg: Color,

    /// Foreground color for disabled selected menu entry
    pub menu_selected_disabled_fg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: Color::Reset,
            status_bar_fg: Color::White,
            status_bar_bg: Color::Blue,
            filename_color: Color::LightYellow,
            cursor_color: Color::Reset,
            selection_fg: Color::White,
            selection_bg: Color::LightBlue,
            highlight_fg: Color::Black,
            highlight_bg: Color::LightYellow,
            link_color: Color::Blue,
            reveal_tag_fg: Color::Black,
            reveal_tag_bg: Color::Gray,
            scrollbar_knob_fg: Color::Reset,
            scrollbar_knob_bg: Color::Reset,
            scrollbar_track_fg: Color::Reset,
            scrollbar_track_bg: Color::Reset,
            menu_fg: Color::White,
            menu_bg: Color::Black,
            menu_disabled_fg: Color::DarkGray,
            menu_selected_fg: Color::White,
            menu_selected_bg: Color::LightBlue,
            menu_selected_disabled_fg: Color::DarkGray,
        }
    }
}

impl Theme {
    /// Create a new theme with default colors
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the style for the status bar
    pub fn status_bar_style(&self) -> Style {
        Style::default()
            .fg(self.status_bar_fg)
            .bg(self.status_bar_bg)
    }

    /// Get the style for the filename in the status bar
    pub fn filename_style(&self) -> Style {
        Style::default().fg(self.filename_color)
    }

    /// Get the style for selected text (uses reversed colors by default)
    pub fn selection_style(&self) -> Style {
        Style::default().fg(self.selection_fg).bg(self.selection_bg)
    }

    /// Get the style for highlighted text
    pub fn highlight_style(&self) -> Style {
        Style::default().fg(self.highlight_fg).bg(self.highlight_bg)
    }

    /// Get the style for links
    pub fn link_style(&self) -> Style {
        Style::default().fg(self.link_color)
    }

    /// Get the style for reveal tags
    pub fn reveal_tag_style(&self) -> Style {
        Style::default()
            .fg(self.reveal_tag_fg)
            .bg(self.reveal_tag_bg)
    }

    /// Get the style for the scrollbar knob
    pub fn scrollbar_knob_style(&self) -> Style {
        Style::default()
            .fg(self.scrollbar_knob_fg)
            .bg(self.scrollbar_knob_bg)
    }

    /// Get the style for the scrollbar track
    pub fn scrollbar_track_style(&self) -> Style {
        Style::default()
            .fg(self.scrollbar_track_fg)
            .bg(self.scrollbar_track_bg)
    }

    /// Get the style for the menu/popup
    pub fn menu_style(&self) -> Style {
        Style::default().fg(self.menu_fg).bg(self.menu_bg)
    }

    /// Get the style for a disabled menu item
    pub fn menu_disabled_style(&self) -> Style {
        Style::default().fg(self.menu_disabled_fg)
    }

    /// Get the style for a selected menu entry
    pub fn menu_selected_style(&self) -> Style {
        Style::default()
            .fg(self.menu_selected_fg)
            .bg(self.menu_selected_bg)
    }

    /// Get the style for a disabled selected menu entry
    pub fn menu_selected_disabled_style(&self) -> Style {
        Style::default()
            .fg(self.menu_selected_disabled_fg)
            .bg(self.menu_selected_bg)
    }
}
