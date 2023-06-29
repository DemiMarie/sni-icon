use dbus::blocking::{Connection, SyncConnection};
use dbus::channel::MatchingReceiver;
use dbus::message::{MatchRule, SignalArgs};
use dbus_crossroads::Crossroads;
use std::collections::HashMap;
use std::error::Error;
use std::io::Write;
use std::time::Duration;

use sni_icon::client::item::StatusNotifierItem;
use sni_icon::client::watcher::StatusNotifierWatcher;
use sni_icon::*;

use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::sync::Mutex;

fn reader(name_map: Arc<Mutex<HashMap<String, String>>>) {
    let mut stdin = std::io::stdin().lock();
    let c = Connection::new_session().unwrap();
    loop {
        let item: sni_icon::IconServerEvent =
            bincode::decode_from_std_read(&mut stdin, bincode::config::standard()).unwrap();
        eprintln!("->server {:?}", item);
        if let Some(pathname) = name_map.lock().unwrap().get(&item.id) {
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
                ServerEvent::Scroll { delta, orientation } => {
                    icon.scroll(delta, &orientation).unwrap()
                }
            }
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    // Let's start by starting up a connection to the session bus and request a name.
    let c = SyncConnection::new_session()?;

    let watcher = c.with_proxy(
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        Duration::from_millis(1000),
    );
    let name_map = Arc::new(Mutex::new(HashMap::<String, String>::new()));
    let reverse_name_map = Arc::new(Mutex::new(HashMap::<String, String>::new()));
    let reverse_name_map_ = reverse_name_map.clone();
    std::thread::spawn(move || reader(reverse_name_map_));

    let mut index = 0;
    let name_map_ = name_map.clone();
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
                bincode::encode_into_std_write(
                    IconClientEvent {
                        id: nm.clone(),
                        event: ClientEvent::Title(icon.title().ok()),
                    },
                    &mut std::io::stdout().lock(),
                    bincode::config::standard(),
                )
                .unwrap();

                std::io::stdout().lock().flush().unwrap();
            }
            true
        },
    )?;
    let name_map_ = name_map.clone();
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
                    bincode::encode_into_std_write(
                        IconClientEvent {
                            id: nm.clone(),
                            event: ClientEvent::Icon {
                                typ: IconType::Normal,
                                data: icon_pixmap
                                    .into_iter()
                                    .map(|(w, h, data)| IconData {
                                        width: w as u32,
                                        height: h as u32,
                                        data: data,
                                    })
                                    .collect(),
                            },
                        },
                        &mut std::io::stdout().lock(),
                        bincode::config::standard(),
                    )
                    .unwrap();
                } else {
                    bincode::encode_into_std_write(
                        IconClientEvent {
                            id: nm.clone(),
                            event: ClientEvent::RemoveIcon(IconType::Normal),
                        },
                        &mut std::io::stdout().lock(),
                        bincode::config::standard(),
                    )
                    .unwrap();
                }

                std::io::stdout().lock().flush().unwrap();
            }
            true
        },
    )?;
    let name_map_ = name_map.clone();
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
                    bincode::encode_into_std_write(
                        IconClientEvent {
                            id: nm.clone(),
                            event: ClientEvent::Icon {
                                typ: IconType::Attention,
                                data: icon_pixmap
                                    .into_iter()
                                    .map(|(w, h, data)| IconData {
                                        width: w as u32,
                                        height: h as u32,
                                        data: data,
                                    })
                                    .collect(),
                            },
                        },
                        &mut std::io::stdout().lock(),
                        bincode::config::standard(),
                    )
                    .unwrap();
                } else {
                    bincode::encode_into_std_write(
                        IconClientEvent {
                            id: nm.clone(),
                            event: ClientEvent::RemoveIcon(IconType::Attention),
                        },
                        &mut std::io::stdout().lock(),
                        bincode::config::standard(),
                    )
                    .unwrap();
                }

                std::io::stdout().lock().flush().unwrap();
            }
            true
        },
    )?;
    let name_map_ = name_map.clone();
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
                    bincode::encode_into_std_write(
                        IconClientEvent {
                            id: nm.clone(),
                            event: ClientEvent::Icon {
                                typ: IconType::Overlay,
                                data: icon_pixmap
                                    .into_iter()
                                    .map(|(w, h, data)| IconData {
                                        width: w as u32,
                                        height: h as u32,
                                        data: data,
                                    })
                                    .collect(),
                            },
                        },
                        &mut std::io::stdout().lock(),
                        bincode::config::standard(),
                    )
                    .unwrap();
                } else {
                    bincode::encode_into_std_write(
                        IconClientEvent {
                            id: nm.clone(),
                            event: ClientEvent::RemoveIcon(IconType::Overlay),
                        },
                        &mut std::io::stdout().lock(),
                        bincode::config::standard(),
                    )
                    .unwrap();
                }

                std::io::stdout().lock().flush().unwrap();
            }
            true
        },
    )?;
    let name_map_ = name_map.clone();
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
                bincode::encode_into_std_write(
                    IconClientEvent {
                        id: nm.clone(),
                        event: ClientEvent::Status(icon.status().ok()),
                    },
                    &mut std::io::stdout().lock(),
                    bincode::config::standard(),
                )
                .unwrap();

                std::io::stdout().lock().flush().unwrap();
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
        reverse_name_map
            .lock()
            .unwrap()
            .insert(item_id.clone(), item.clone());
        let iindex = item.find('/').unwrap();
        let icon = c.with_proxy(
            &item[..iindex],
            &item[iindex..],
            Duration::from_millis(1000),
        );

        bincode::encode_into_std_write(
            IconClientEvent {
                id: item_id.clone(),
                event: ClientEvent::Create {
                    category: icon.category()?,
                },
            },
            &mut std::io::stdout().lock(),
            bincode::config::standard(),
        )?;

        bincode::encode_into_std_write(
            IconClientEvent {
                id: item_id.clone(),
                event: ClientEvent::Status(icon.status().ok()),
            },
            &mut std::io::stdout().lock(),
            bincode::config::standard(),
        )?;

        for (ty, fun) in [
            (IconType::Normal, icon.icon_pixmap()),
            (IconType::Attention, icon.attention_icon_pixmap()),
            (IconType::Overlay, icon.overlay_icon_pixmap()),
        ] {
            if let Ok(icon_pixmap) = fun {
                bincode::encode_into_std_write(
                    IconClientEvent {
                        id: item_id.clone(),
                        event: ClientEvent::Icon {
                            typ: ty,
                            data: icon_pixmap
                                .into_iter()
                                .map(|(w, h, data)| IconData {
                                    width: w as u32,
                                    height: h as u32,
                                    data: data,
                                })
                                .collect(),
                        },
                    },
                    &mut std::io::stdout().lock(),
                    bincode::config::standard(),
                )
                .unwrap();
            }
        }

        std::io::stdout().lock().flush()?;
    }

    loop {
        c.process(Duration::from_millis(1000))?;
    }
}
