pub mod client;
pub mod server;

/*
   This example is a WIP demo of the "Crossroads" module, successor of the "Tree" module.

   This example creates a D-Bus server with the following functionality:
   It registers the "com.example.dbustest" name, creates a "/hello" object path,
   which has an "com.example.dbustest" interface.

   The interface has a "Hello" method (which takes no arguments and returns a string),
   and a "HelloHappened" signal (with a string argument) which is sent every time
   someone calls the "Hello" method.
*/
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

struct NotifierIcon {
    pub conn: Mutex<Sender<IconServerEvent>>,
    pub id: String,
    pub category: String,

    pub tooltip: Mutex<Option<Tooltip>>,
    pub title: Mutex<Option<String>>,
    pub status: Mutex<Option<String>>,

    pub icon: Mutex<Option<Vec<IconData>>>,
    pub attention_icon: Mutex<Option<Vec<IconData>>>,
    pub overlay_icon: Mutex<Option<Vec<IconData>>>,
}

impl server::item::StatusNotifierItem for Arc<NotifierIcon> {
    fn context_menu(&mut self, x_: i32, y_: i32) -> Result<(), dbus::MethodErr> {
        self.conn.lock().unwrap().send(IconServerEvent {
            id: self.id.clone(),
            event: ServerEvent::ContextMenu,
        }).unwrap();
        Ok(())
    }
    fn activate(&mut self, x_: i32, y_: i32) -> Result<(), dbus::MethodErr> {
        self.conn.lock().unwrap().send(IconServerEvent {
            id: self.id.clone(),
            event: ServerEvent::Activate,
        }).unwrap();
        Ok(())
    }
    fn secondary_activate(&mut self, x_: i32, y_: i32) -> Result<(), dbus::MethodErr> {
        self.conn.lock().unwrap().send(IconServerEvent {
            id: self.id.clone(),
            event: ServerEvent::SecondaryActivate,
        }).unwrap();
        Ok(())
    }
    fn scroll(&mut self, delta: i32, orientation: String) -> Result<(), dbus::MethodErr> {
        self.conn.lock().unwrap().send(IconServerEvent {
            id: self.id.clone(),
            event: ServerEvent::Scroll { delta, orientation },
        }).unwrap();
        Ok(())
    }
    fn category(&self) -> Result<String, dbus::MethodErr> {
        Ok(self.category.clone())
    }
    fn id(&self) -> Result<String, dbus::MethodErr> {
        Ok(self.id.clone())
    }
    fn title(&self) -> Result<String, dbus::MethodErr> {
        let title = self.title.lock().unwrap();
        title
            .clone()
            .ok_or_else(|| dbus::MethodErr::no_property("title"))
    }
    fn status(&self) -> Result<String, dbus::MethodErr> {
        let status = self.status.lock().unwrap();
        status
            .clone()
            .ok_or_else(|| dbus::MethodErr::no_property("status"))
    }
    fn window_id(&self) -> Result<i32, dbus::MethodErr> {
        Ok(0)
    }
    fn icon_theme_path(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property(""))
    }
    fn menu(&self) -> Result<dbus::Path<'static>, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property(""))
    }
    fn item_is_menu(&self) -> Result<bool, dbus::MethodErr> {
        Ok(false)
    }
    fn icon_name(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property("IconName"))
    }
    fn icon_pixmap(&self) -> Result<Vec<(i32, i32, Vec<u8>)>, dbus::MethodErr> {
        let icon = self.icon.lock().unwrap();
        let icon = icon
            .as_ref()
            .map(|f| f.as_slice())
            .unwrap_or_else(|| &[]);
        Ok(icon.iter().map(|f| (f.width as i32, f.height as i32, f.data.clone())).collect())
    }
    fn overlay_icon_name(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property("OverlayIconName"))
    }
    fn overlay_icon_pixmap(&self) -> Result<Vec<(i32, i32, Vec<u8>)>, dbus::MethodErr> {
        let overlay_icon = self.overlay_icon.lock().unwrap();
        let overlay_icon = overlay_icon
            .as_ref()
            .map(|f| f.as_slice())
            .unwrap_or_else(|| &[]);
        Ok(overlay_icon.iter().map(|f| (f.width as i32, f.height as i32, f.data.clone())).collect())
    }
    fn attention_icon_name(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property("AttentionIconName"))
    }
    fn attention_icon_pixmap(&self) -> Result<Vec<(i32, i32, Vec<u8>)>, dbus::MethodErr> {
        let attention_icon = self.attention_icon.lock().unwrap();
        let attention_icon = attention_icon
            .as_ref()
            .map(|f| f.as_slice())
            .unwrap_or_else(|| &[]);

        Ok(attention_icon.iter().map(|f| (f.width as i32, f.height as i32, f.data.clone())).collect())
    }
    fn attention_movie_name(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property("AttentionMovieName"))
    }

    fn tool_tip(
        &self,
    ) -> Result<(String, Vec<(i32, i32, Vec<u8>)>, String, String), dbus::MethodErr> {
        let tooltip = self.tooltip.lock().unwrap();
        let tooltip = tooltip
            .as_ref()
            .ok_or_else(|| dbus::MethodErr::no_property("ToolTip"))?;

        let icon_data = tooltip
            .icon_data.iter().map(|f| (f.width as i32, f.height as i32, f.data.clone())).collect();

        Ok((
            String::new(),
            icon_data,
            tooltip.title.clone(),
            tooltip.description.clone(),
        ))
    }
}

fn client_server(r: Receiver<IconClientEvent>, s: Sender<IconServerEvent>) {
    let mut items: HashMap<String, Arc<NotifierIcon>> = HashMap::new();
    let c = Arc::new(SyncConnection::new_session().unwrap());
    let cr = Arc::new(Mutex::new(Crossroads::new()));
    let iface_token =
        server::item::register_status_notifier_item::<Arc<NotifierIcon>>(&mut cr.lock().unwrap());
    let cr_ = cr.clone();
    c.start_receive(
        dbus::message::MatchRule::new_method_call(),
        Box::new(move |msg, conn| {
            cr_.lock().unwrap().handle_message(msg, conn).unwrap();
            true
        }),
    );

    let watcher = c.with_proxy(
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        Duration::from_millis(1000),
    );

    loop {
        c.process(Duration::from_millis(100)).unwrap();
        while let Some(item) = r.recv_timeout(Duration::from_millis(100)).ok() {
            if let ClientEvent::Create { category } = &item.event {
                if items.contains_key(&item.id) {
                    panic!("Item ID exists already");
                }

                let notifier = Arc::new(NotifierIcon {
                    conn: Mutex::new(s.clone()),
                    id: item.id.clone(),
                    category: category.clone(),

                    tooltip: Mutex::new(None),
                    title: Mutex::new(None),
                    status: Mutex::new(None),
                    icon: Mutex::new(None),
                    attention_icon: Mutex::new(None),
                    overlay_icon: Mutex::new(None),
                });

                items.insert(item.id.clone(), notifier.clone());

                cr.lock().unwrap().insert(
                    format!("/{}/StatusNotifierItem", item.id),
                    &[iface_token],
                    notifier,
                );
                watcher
                    .register_status_notifier_item(&format!("{}/{}", c.unique_name(), item.id))
                    .unwrap();
            } else {
                let watcher = c.with_proxy(
                    "org.kde.StatusNotifierWatcher",
                    "/StatusNotifierWatcher",
                    Duration::from_millis(1000),
                );

                let ni = items.get(&item.id).unwrap();
                match item.event {
                    ClientEvent::Create { .. } => unreachable!(),
                    ClientEvent::Title(title) => {
                        *ni.title.lock().unwrap() = title;
                    }
                    ClientEvent::Status(status) => {
                        *ni.status.lock().unwrap() = status;
                    }
                    ClientEvent::Icon {
                        typ,
                        data,
                    } => match typ {
                        IconType::Normal => {
                            *ni.icon.lock().unwrap() = Some(data);
                            c.channel().send((server::item::StatusNotifierItemNewIcon {}).to_emit_message(&dbus::Path::new(format!("/{}/StatusNotifierItem", item.id)).unwrap())).unwrap();
                        }
                        IconType::Attention => {
                            *ni.attention_icon.lock().unwrap() = Some(data);
                            c.channel().send((server::item::StatusNotifierItemNewAttentionIcon {}).to_emit_message(&dbus::Path::new(format!("/{}/StatusNotifierItem", item.id)).unwrap())).unwrap();
                        }
                        IconType::Overlay => {
                            *ni.overlay_icon.lock().unwrap() = Some(data);
                            c.channel().send((server::item::StatusNotifierItemNewOverlayIcon {}).to_emit_message(&dbus::Path::new(format!("/{}/StatusNotifierItem", item.id)).unwrap())).unwrap();
                        }
                    },
                    ClientEvent::RemoveIcon(typ) => match typ {
                        IconType::Normal => *ni.icon.lock().unwrap() = None,
                        IconType::Attention => *ni.attention_icon.lock().unwrap() = None,
                        IconType::Overlay => *ni.overlay_icon.lock().unwrap() = None,
                    },
                    ClientEvent::Tooltip {
                        icon_data,
                        title,
                        description,
                    } => {
                        *ni.tooltip.lock().unwrap() = Some(Tooltip {
                            title,
                            description,
                            icon_data: icon_data,
                        });
                    }
                    ClientEvent::RemoveTooltip => {
                        *ni.tooltip.lock().unwrap() = None;
                    }

                    ClientEvent::Destroy => {
                        cr.lock().unwrap().remove::<Arc<NotifierIcon>>(
                            &dbus::Path::new(format!("/{}/StatusNotifierItem", item.id)).unwrap(),
                        );
                        items.remove(&item.id);
                    }
                }
            }
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let (server_sender, server_receiver) = std::sync::mpsc::channel();
    let (client_sender, client_receiver) = std::sync::mpsc::channel();

    let client_sender = Arc::new(Mutex::new(client_sender));

    std::thread::spawn(move || client_server(client_receiver, server_sender));

    // Let's start by starting up a connection to the session bus and request a name.
    let c = SyncConnection::new_session()?;

    let watcher = c.with_proxy(
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        Duration::from_millis(1000),
    );
    let name_map = Arc::new(Mutex::new(HashMap::<String, String>::new()));
    let reverse_name_map = Arc::new(Mutex::new(HashMap::<String, String>::new()));
    let mut index = 0;

    let (name_map_, client_sender_) = (name_map.clone(), client_sender.clone());
    c.add_match(
        client::item::StatusNotifierItemNewTitle::match_rule(None, None),
        move |_: client::item::StatusNotifierItemNewIcon, c, msg| {
            let fullpath = format!("{}{}", msg.sender().unwrap(), msg.path().unwrap());
            let icon = c.with_proxy(
                msg.sender().unwrap(),
                msg.path().unwrap(),
                Duration::from_millis(1000),
            );
            let nm = name_map_.lock().unwrap();
            if let Some(nm) = nm.get(&fullpath) {
                client_sender_.lock().unwrap().send(IconClientEvent {
                    id: nm.clone(),
                    event: ClientEvent::Title(icon.title().ok()),
                }).unwrap();
            }
            true
        },
    )?;
    let (name_map_, client_sender_) = (name_map.clone(), client_sender.clone());
    c.add_match(
        client::item::StatusNotifierItemNewIcon::match_rule(None, None),
        move |_: client::item::StatusNotifierItemNewIcon, c, msg| {
            let fullpath = format!("{}{}", msg.sender().unwrap(), msg.path().unwrap());
            let icon = c.with_proxy(
                msg.sender().unwrap(),
                msg.path().unwrap(),
                Duration::from_millis(1000),
            );
            let nm = name_map_.lock().unwrap();
            if let Some(nm) = nm.get(&fullpath) {
                if let Ok(icon_pixmap) = icon.icon_pixmap() {
                        client_sender_.lock().unwrap().send(IconClientEvent {
                            id: nm.clone(),
                            event: ClientEvent::Icon {
                                typ: IconType::Normal,
                                data: icon_pixmap.into_iter().map(|(w, h, data)| IconData { width: w as u32, height: h as u32, data: data }).collect(),
                            },
                        }).unwrap();
                } else {
                    client_sender_.lock().unwrap().send(IconClientEvent {
                        id: nm.clone(),
                        event: ClientEvent::RemoveIcon(IconType::Normal),
                    }).unwrap();
                }
            }
            true
        },
    )?;
    let (name_map_, client_sender_) = (name_map.clone(), client_sender.clone());
    c.add_match(
        client::item::StatusNotifierItemNewAttentionIcon::match_rule(None, None),
        move |_: client::item::StatusNotifierItemNewAttentionIcon, c, msg| {
            let fullpath = format!("{}{}", msg.sender().unwrap(), msg.path().unwrap());
            let icon = c.with_proxy(
                msg.sender().unwrap(),
                msg.path().unwrap(),
                Duration::from_millis(1000),
            );
            let nm = name_map_.lock().unwrap();
            if let Some(nm) = nm.get(&fullpath) {
                if let Ok(icon_pixmap) = icon.attention_icon_pixmap() {
                        client_sender_.lock().unwrap().send(IconClientEvent {
                            id: nm.clone(),
                            event: ClientEvent::Icon {
                                typ: IconType::Attention,
                                data: icon_pixmap.into_iter().map(|(w, h, data)| IconData { width: w as u32, height: h as u32, data: data }).collect(),
                            },
                        }).unwrap();
                } else {
                    client_sender_.lock().unwrap().send(IconClientEvent {
                        id: nm.clone(),
                        event: ClientEvent::RemoveIcon(IconType::Attention),
                    }).unwrap();
                }
            }
            true
        },
    )?;
    let (name_map_, client_sender_) = (name_map.clone(), client_sender.clone());
    c.add_match(
        client::item::StatusNotifierItemNewOverlayIcon::match_rule(None, None),
        move |_: client::item::StatusNotifierItemNewOverlayIcon, c, msg| {
            let fullpath = format!("{}{}", msg.sender().unwrap(), msg.path().unwrap());
            let icon = c.with_proxy(
                msg.sender().unwrap(),
                msg.path().unwrap(),
                Duration::from_millis(1000),
            );
            let nm = name_map_.lock().unwrap();
            if let Some(nm) = nm.get(&fullpath) {
                if let Ok(icon_pixmap) = icon.overlay_icon_pixmap() {
                        client_sender_.lock().unwrap().send(IconClientEvent {
                            id: nm.clone(),
                            event: ClientEvent::Icon {
                                typ: IconType::Overlay,
                                data: icon_pixmap.into_iter().map(|(w, h, data)| IconData { width: w as u32, height: h as u32, data: data }).collect(),
                            },
                        }).unwrap();
                } else {
                    client_sender_.lock().unwrap().send(IconClientEvent {
                        id: nm.clone(),
                        event: ClientEvent::RemoveIcon(IconType::Overlay),
                    }).unwrap();
                }
            }
            true
        },
    )?;
    let (name_map_, client_sender_) = (name_map.clone(), client_sender.clone());
    c.add_match(
        client::item::StatusNotifierItemNewStatus::match_rule(None, None),
        move |_: client::item::StatusNotifierItemNewIcon, c, msg| {
            let fullpath = format!("{}{}", msg.sender().unwrap(), msg.path().unwrap());
            let icon = c.with_proxy(
                msg.sender().unwrap(),
                msg.path().unwrap(),
                Duration::from_millis(1000),
            );
            let nm = name_map_.lock().unwrap();

            if let Some(nm) = nm.get(&fullpath) {
                    client_sender_.lock().unwrap().send(IconClientEvent {
                        id: nm.clone(),
                        event: ClientEvent::Status(icon.status().ok()),
                    }).unwrap();
            }
            true
        },
    )?;

    for item in watcher.registered_status_notifier_items()? {
        let item_id = format!("Item{}", index);
        index += 1;
        name_map
            .lock()
            .unwrap()
            .insert(item.clone(), item_id.clone());
        reverse_name_map.lock().unwrap().insert(item_id.clone(), item.clone());
        let iindex = item.find('/').unwrap();
        let icon = c.with_proxy(
            &item[..iindex],
            &item[iindex..],
            Duration::from_millis(1000),
        );

        client_sender.lock().unwrap().send(IconClientEvent {
            id: item_id.clone(),
            event: ClientEvent::Create {
                category: icon.category()?,
            },
        })?;

        client_sender.lock().unwrap().send(IconClientEvent {
            id: item_id.clone(),
            event: ClientEvent::Status(icon.status().ok()),
        })?;

        for (ty, fun) in [
            (IconType::Normal, icon.icon_pixmap()),
            (IconType::Attention, icon.attention_icon_pixmap()),
            (IconType::Overlay, icon.overlay_icon_pixmap()),
        ] {
            if let Ok(icon_pixmap) = fun {
                        client_sender.lock().unwrap().send(IconClientEvent {
                            id: item_id.clone(),
                            event: ClientEvent::Icon {
                                typ: ty,
                                data: icon_pixmap.into_iter().map(|(w, h, data)| IconData { width: w as u32, height: h as u32, data: data }).collect(),
                            },
                        }).unwrap();
            }
        }
    }

    std::thread::scope(move |scope| {
        let reversee_name_map_ = reverse_name_map.clone();
        scope.spawn(move || {
            let c = Connection::new_session().unwrap();
            for item in server_receiver {
                if let Some(pathname) = reversee_name_map_.lock().unwrap().get(&item.id) {
                    let iindex = pathname.find('/').unwrap();
                    let icon = c.with_proxy(
                        &pathname[..iindex],
                        &pathname[iindex..],
                        Duration::from_millis(1000),
                    );

                    match item.event {
                        ServerEvent::Activate => icon.activate(0, 0).unwrap(),
                        ServerEvent::SecondaryActivate => icon.secondary_activate(0, 0).unwrap(),
                        ServerEvent::ContextMenu => icon.context_menu(0, 0).unwrap(),
                        ServerEvent::Scroll { delta, orientation } => icon.scroll(delta, &orientation).unwrap(),
                    }
                }
            }
        });

        loop {
            c.process(Duration::from_millis(1000))?;
        }
    })
}
