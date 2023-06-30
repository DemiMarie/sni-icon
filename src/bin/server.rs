use dbus::blocking::{Connection, SyncConnection};

use dbus::message::SignalArgs;
use dbus::Message;

use std::collections::HashMap;
use std::error::Error;
use std::io::Write;
use std::time::Duration;

use sni_icon::client::item::StatusNotifierItem;
use sni_icon::client::watcher::StatusNotifierWatcher;
use sni_icon::*;

use std::sync::Arc;
use std::sync::Mutex;

fn send_or_panic<T: bincode::Encode>(s: T) {
    let mut out = std::io::stdout().lock();
    bincode::encode_into_std_write(s, &mut out, bincode::config::standard())
        .expect("Cannot write to stdout");
    out.flush().expect("Cannot flush stdout");
}

fn reader(name_map: Arc<Mutex<HashMap<u64, String>>>) {
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
                ServerEvent::Activate => icon.activate(0, 0).unwrap_or_else(|e| {
                    eprintln!("->server error {:?}", e);
                }),
                ServerEvent::SecondaryActivate => {
                    icon.secondary_activate(0, 0).unwrap_or_else(|e| {
                        eprintln!("->server error {:?}", e);
                    })
                }
                ServerEvent::ContextMenu => icon.context_menu(0, 0).unwrap_or_else(|e| {
                    eprintln!("->server error {:?}", e);
                }),
                ServerEvent::Scroll { delta, orientation } => {
                    icon.scroll(delta, &orientation).unwrap_or_else(|e| {
                        eprintln!("->server error {:?}", e);
                    })
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct NameOwnerChanged {
    pub name: String,
    pub old_owner: String,
    pub new_owner: String,
}

impl dbus::arg::ReadAll for NameOwnerChanged {
    fn read(i: &mut dbus::arg::Iter) -> Result<Self, dbus::arg::TypeMismatchError> {
        Ok(Self {
            name: i.read()?,
            old_owner: i.read()?,
            new_owner: i.read()?,
        })
    }
}

impl dbus::message::SignalArgs for NameOwnerChanged {
    const NAME: &'static str = "NameOwnerChanged";
    const INTERFACE: &'static str = "org.freedesktop.DBus";
}

fn main() -> Result<(), Box<dyn Error>> {
    // Let's start by starting up a connection to the session bus and request a name.
    let c = SyncConnection::new_session()?;

    let bus_watcher = c.with_proxy(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        Duration::from_millis(1000),
    );

    let watcher = c.with_proxy(
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        Duration::from_millis(1000),
    );

    let name_map = Arc::new(Mutex::new(HashMap::<String, u64>::new()));
    let reverse_name_map = Arc::new(Mutex::new(HashMap::<u64, String>::new()));
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
            if let Some(&id) = nm.get(&fullpath) {
                send_or_panic(IconClientEvent {
                    id,
                    event: ClientEvent::Title(icon.title().ok()),
                })
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

    let name_map_ = name_map.clone();
    let reverse_name_map_ = reverse_name_map.clone();
    let mut go =
        move |item: String, c: &SyncConnection| -> Result<(), Box<dyn std::error::Error>> {
            eprintln!("Going!");
            let iindex = match item.find('/') {
                None => return Ok(()), // invalid name
                Some(position) => position,
            };
            let bus_name = &item[..iindex];
            let object_path = &item[iindex..];
            index += 1;
            let id = index;
            eprintln!("Got new object {:?}, id {}", &item, id);
            let icon = c.with_proxy(bus_name, object_path, Duration::from_millis(1000));

            bincode::encode_into_std_write(
                IconClientEvent {
                    id,
                    event: ClientEvent::Create {
                        category: icon.category()?,
                    },
                },
                &mut std::io::stdout().lock(),
                bincode::config::standard(),
            )
            .expect("error writing to stdout");
            name_map_
                .lock()
                .expect("poisoned?")
                .insert(bus_name.to_owned(), id);
            reverse_name_map_
                .lock()
                .unwrap()
                .insert(id, bus_name.to_owned());
            eprintln!("Create event sent");

            bincode::encode_into_std_write(
                IconClientEvent {
                    id,
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
                            id,
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
                    .expect("cannot write to stdout");
                }
            }

            std::io::stdout()
                .lock()
                .flush()
                .expect("cannot write to stdout");
            eprintln!("Returning from go()");
            Ok::<(), _>(())
        };

    for item in watcher.registered_status_notifier_items()? {
        go(item, &c)?
    }

    let handle_notifier =
        move |arg: client::watcher::StatusNotifierWatcherStatusNotifierItemRegistered,
              c: &SyncConnection,
              _msg: &Message|
              -> bool {
            eprintln!("Picked up registered event");
            let _ = go(arg.arg0, c);
            true
        };

    let handle_name_lost = move |NameOwnerChanged {
                                     name,
                                     old_owner,
                                     new_owner,
                                 },
                                 c: &SyncConnection,
                                 _msg: &Message|
          -> bool {
        if old_owner.is_empty() || !new_owner.is_empty() {
            return true;
        }
        let id = match name_map.lock().expect("poisoned?").remove(&name) {
            Some(i) => i,
            None => return true,
        };
        eprintln!("Name {} lost, destroying icon {}", &name, id);
        let r = reverse_name_map
            .lock()
            .unwrap()
            .remove(&id)
            .expect("reverse and forward maps inconsistent");
        assert_eq!(name, r, "reverse and forward maps inconsistent");
        bincode::encode_into_std_write(
            IconClientEvent {
                id,
                event: ClientEvent::Destroy,
            },
            &mut std::io::stdout().lock(),
            bincode::config::standard(),
        )
        .expect("cannot write to stdout");
        std::io::stdout()
            .lock()
            .flush()
            .expect("cannot write to stdout");
        true
    };

    watcher.match_signal(handle_notifier)?;

    bus_watcher.match_signal(handle_name_lost)?;

    loop {
        c.process(Duration::from_millis(1000))?;
    }
}
