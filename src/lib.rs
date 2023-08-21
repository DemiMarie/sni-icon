pub mod client;
pub mod names;
pub mod server;


#[derive(Debug, bincode::Decode, bincode::Encode, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum IconType {
    Normal = 1,
    Overlay = 2,
    Attention = 4,
    Status = 8,
    Title = 16,
}

#[derive(Debug, bincode::Decode, bincode::Encode, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum Event {
    Clicked,
    Hovered,
    Opened,
    Closed,
}

/// The following checks are used during insertion to ensure that the tree
/// invariants are maintained:
///
/// - The `id` field of an object being inserted must not exist in the tree.
/// - The depth of the object must not exceed 5.
/// - The parent of the object being inserted must be [`None`].  It is set to [`Some`]
///   after this check.
#[derive(Debug, bincode::Decode, bincode::Encode)]
pub enum DBusMenuEntry {
    /// Separator
    Separator { visible: bool },
    /// Standard
    Standard {
        /// Label
        label: String,
        /// Whether this entry is enabled.
        /// Not used.
        visible: bool,
        #[cfg(any())]
        /// Icon name, must be sanitized.  Not used.
        icon_name: String,
        /// PNG data, not used.
        #[cfg(any())]
        icon_data: Vec<u8>,
        /// Shortcut key.  Must be unique.  Not used.
        #[cfg(any())]
        shortcut: Vec<(Vec<ModifierKey>, char)>,
        /// Children of this object
        children: Vec<DBusMenuEntry>,
        /// Disposition of this entry
        disposition: Disposition,
        /// The ID of this entry
        id: Option<core::num::NonZeroI32>,
        /// The depth of this entry.  Used to limit recursion.
        depth: u32,
        /// [`None`] for freshly-created objects.  Otherwise, holds the parent ID.
        parent: Option<core::num::NonZeroI32>,
    },
}

/// Menu entry type
#[derive(Debug, bincode::Decode, bincode::Encode)]
pub enum MenuEntryType {
    /// Standard menu entry
    Standard,
    /// Separator
    Separator,
}

/// Modifier key
#[derive(Debug, bincode::Decode, bincode::Encode)]
pub enum ModifierKey {
    /// Control key
    Control,
    /// Alt key
    Alt,
    /// Shift key
    Shift,
    /// Super key
    Super,
}

/// Toggleable state.  The proxy enforces that at most one entry in a radio
/// menu is checked at any one time.  Trying to check a different entry
/// always causes the preceding one to be
#[derive(Debug, bincode::Decode, bincode::Encode)]
pub enum Togglable {
    /// Menu entry is a checkmark
    Checkmark { toggled: bool },
    /// Menu entry is a radio dialog
    Radio { toggled: bool },
    /// Menu entry cannot be toggled.
    NonToggleable,
}

#[derive(Debug, bincode::Decode, bincode::Encode)]
pub enum Disposition {
    Normal,
    Informative,
    Warning,
    Alert,
}

#[derive(Debug, bincode::Decode, bincode::Encode)]
pub enum ClientEvent {
    Create {
        category: String,
        app_id: String,
        has_menu: bool,
    },

    Title(Option<String>),
    Status(Option<String>),
    Icon {
        typ: IconType,
        data: Vec<IconData>,
    },

    RemoveIcon(IconType),

    Destroy,

    Tooltip {
        icon_data: Vec<IconData>,
        title: String,
        description: String,
    },

    RemoveTooltip,

    EnableMenu {
        revision: u32,
        entries: DBusMenuEntry,
    },
}

#[derive(Debug, bincode::Decode, bincode::Encode)]
pub enum ServerEvent {
    Activate { x: i32, y: i32 },
    ContextMenu { x: i32, y: i32 },
    SecondaryActivate { x: i32, y: i32 },
    Scroll { delta: i32, orientation: String },
}

#[derive(Debug, bincode::Decode, bincode::Encode)]
pub struct IconClientEvent {
    pub id: u64,
    pub event: ClientEvent,
}

#[derive(Debug, bincode::Decode, bincode::Encode)]
pub struct IconServerEvent {
    pub id: u64,
    pub event: ServerEvent,
}

#[derive(Debug, bincode::Decode, bincode::Encode)]
pub struct IconData {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, bincode::Decode, bincode::Encode)]
pub struct Tooltip {
    pub title: String,
    pub description: String,
    pub icon_data: Vec<IconData>,
}
