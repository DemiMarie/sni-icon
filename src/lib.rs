pub mod client;
pub mod server;

use dbus::blocking::{Connection, SyncConnection};
use dbus::channel::MatchingReceiver;
use dbus::message::{MatchRule, SignalArgs};
use dbus_crossroads::Crossroads;
use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;

use client::item::StatusNotifierItem;
use client::watcher::StatusNotifierWatcher;

use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Debug, bincode::Decode, bincode::Encode)]
pub enum IconType {
    Normal,
    Overlay,
    Attention,
}

#[derive(Debug, bincode::Decode, bincode::Encode)]
pub enum ClientEvent {
    Create {
        category: String,
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
    Activate,
    ContextMenu,
    SecondaryActivate,
    Scroll { delta: i32, orientation: String },
}

#[derive(Debug, bincode::Decode, bincode::Encode)]
pub struct IconClientEvent {
    pub id: String,
    pub event: ClientEvent,
}

#[derive(Debug, bincode::Decode, bincode::Encode)]
pub struct IconServerEvent {
    pub id: String,
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
