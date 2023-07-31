pub mod client;
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
