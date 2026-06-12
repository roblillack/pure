//! TurboVision-style menu bar: menu definitions and navigation state.
//!
//! The bar is hidden during normal editing and overlays the top row of the
//! screen while active. It is activated with F10 (bar only) or with a menu's
//! Alt accelerator (bar plus open drop-down), and driven entirely with the
//! keyboard: Left/Right switch menus, Up/Down move within a drop-down,
//! Return activates the selected item, Esc closes the bar.

/// An application-level command reachable from the menu bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppAction {
    Save,
    Quit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    InsertLineBreak,
    InsertSiblingParagraph,
    FormattingMenu,
    ToggleRevealCodes,
}

pub struct MenuBarItem {
    pub label: &'static str,
    /// Shortcut text shown right-aligned in the drop-down (display only).
    pub shortcut: Option<&'static str>,
    /// `None` marks the item as disabled (shown, but not activatable).
    pub action: Option<AppAction>,
}

pub enum MenuBarEntry {
    Item(MenuBarItem),
    Separator,
}

pub struct MenuDef {
    pub title: &'static str,
    /// Char index into `title` of the highlighted accelerator letter.
    pub accel_index: usize,
    pub entries: &'static [MenuBarEntry],
}

impl MenuDef {
    pub fn accel(&self) -> char {
        self.title
            .chars()
            .nth(self.accel_index)
            .expect("accelerator index within menu title")
            .to_ascii_lowercase()
    }
}

const fn item(
    label: &'static str,
    shortcut: Option<&'static str>,
    action: AppAction,
) -> MenuBarEntry {
    MenuBarEntry::Item(MenuBarItem {
        label,
        shortcut,
        action: Some(action),
    })
}

pub const MENU_BAR: &[MenuDef] = &[
    MenuDef {
        title: "File",
        accel_index: 0,
        entries: &[
            item("Save", Some("^S"), AppAction::Save),
            MenuBarEntry::Separator,
            item("Quit", Some("^Q"), AppAction::Quit),
        ],
    },
    MenuDef {
        title: "Edit",
        accel_index: 0,
        entries: &[
            item("Undo", Some("^Z"), AppAction::Undo),
            item("Redo", Some("^Y"), AppAction::Redo),
            MenuBarEntry::Separator,
            item("Cut", Some("^X"), AppAction::Cut),
            item("Copy", Some("^C"), AppAction::Copy),
            item("Paste", Some("^V"), AppAction::Paste),
        ],
    },
    MenuDef {
        title: "Insert",
        accel_index: 0,
        entries: &[
            item("Line Break", Some("^J"), AppAction::InsertLineBreak),
            item(
                "Sibling Paragraph",
                Some("^P"),
                AppAction::InsertSiblingParagraph,
            ),
        ],
    },
    MenuDef {
        title: "Format",
        accel_index: 1,
        entries: &[item(
            "Formatting Menu...",
            Some("^Space"),
            AppAction::FormattingMenu,
        )],
    },
    MenuDef {
        title: "View",
        accel_index: 0,
        entries: &[item(
            "Reveal Codes",
            Some("F9"),
            AppAction::ToggleRevealCodes,
        )],
    },
];

/// Find the menu whose accelerator letter matches `ch` (case-insensitive).
pub fn menu_with_accel(ch: char) -> Option<usize> {
    let ch = ch.to_ascii_lowercase();
    MENU_BAR.iter().position(|menu| menu.accel() == ch)
}

/// Column offset of a menu title within the bar (the bar starts with one
/// leading space and every title is padded with one space on each side).
pub fn menu_title_offset(index: usize) -> usize {
    1 + MENU_BAR[..index]
        .iter()
        .map(|menu| menu.title.chars().count() + 2)
        .sum::<usize>()
}

/// Navigation state of the active menu bar.
pub struct MenuBarState {
    selected_menu: usize,
    /// Selected entry index in the open drop-down, or `None` while only the
    /// bar itself is active.
    selected_item: Option<usize>,
}

impl MenuBarState {
    /// Bar active, no drop-down open (F10).
    pub fn new() -> Self {
        Self {
            selected_menu: 0,
            selected_item: None,
        }
    }

    /// Bar active with `menu`'s drop-down open (Alt accelerator).
    pub fn open_at(menu: usize) -> Self {
        let mut state = Self {
            selected_menu: menu,
            selected_item: None,
        };
        state.open_dropdown();
        state
    }

    pub fn selected_menu(&self) -> usize {
        self.selected_menu
    }

    pub fn dropdown_item(&self) -> Option<usize> {
        self.selected_item
    }

    fn entries(&self) -> &'static [MenuBarEntry] {
        MENU_BAR[self.selected_menu].entries
    }

    /// Open the drop-down of the selected menu, selecting the first enabled
    /// item.
    pub fn open_dropdown(&mut self) {
        self.selected_item = self
            .entries()
            .iter()
            .position(|entry| matches!(entry, MenuBarEntry::Item(item) if item.action.is_some()))
            .or(Some(0));
    }

    /// Move to the previous/next menu, keeping a drop-down open if one was.
    pub fn move_menu(&mut self, delta: i32) {
        let len = MENU_BAR.len() as i32;
        self.selected_menu = (self.selected_menu as i32 + delta).rem_euclid(len) as usize;
        if self.selected_item.is_some() {
            self.open_dropdown();
        }
    }

    pub fn select_menu(&mut self, menu: usize) {
        self.selected_menu = menu;
        self.open_dropdown();
    }

    /// Move the drop-down selection up/down, skipping separators.
    pub fn move_item(&mut self, delta: i32) {
        let Some(current) = self.selected_item else {
            return;
        };
        let entries = self.entries();
        let len = entries.len() as i32;
        let mut idx = current as i32;
        for _ in 0..len {
            idx = (idx + delta).rem_euclid(len);
            if matches!(entries[idx as usize], MenuBarEntry::Item(_)) {
                self.selected_item = Some(idx as usize);
                break;
            }
        }
    }

    /// The action of the currently selected drop-down item, if it is enabled.
    pub fn selected_action(&self) -> Option<AppAction> {
        let index = self.selected_item?;
        match self.entries().get(index) {
            Some(MenuBarEntry::Item(item)) => item.action,
            _ => None,
        }
    }
}

impl Default for MenuBarState {
    fn default() -> Self {
        Self::new()
    }
}
