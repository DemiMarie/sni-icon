use dbus::blocking::{Connection, SyncConnection};

use dbus::message::SignalArgs;
use dbus::strings::{BusName, Path};
use dbus::Message;

use std::collections::HashMap;
use std::error::Error;
use std::io::Write;
use std::time::Duration;

use sni_icon::client::item::StatusNotifierItem;
use sni_icon::client::menu::Dbusmenu;
use sni_icon::client::watcher::StatusNotifierWatcher;
use sni_icon::*;

use std::sync::Arc;
use std::sync::Mutex;

fn send_or_panic<T: bincode::Encode>(s: T) {
    let mut out = std::io::stdout().lock();
    let v = bincode::encode_to_vec(s, bincode::config::standard()).expect("Cannot encode data");
    eprintln!("Sending {} bytes", v.len());
    out.write_all(&((v.len() as u32).to_le_bytes())[..])
        .expect("cannot write to stdout");
    out.write_all(&v[..]).expect("cannot write to stdout");
    out.flush().expect("Cannot flush stdout");
}

fn reader(reverse_name_map: Arc<Mutex<HashMap<u64, (String, Option<dbus::Path>)>>>) {
    let mut stdin = std::io::stdin().lock();
    let c = Connection::new_session().unwrap();
    loop {
        let item: sni_icon::IconServerEvent =
            bincode::decode_from_std_read(&mut stdin, bincode::config::standard()).unwrap();
        eprintln!("->server {:?}", item);
        if let Some((pathname, _)) = reverse_name_map.lock().unwrap().get(&item.id) {
            let (bus_name, object_path) = match pathname.find('/') {
                None => (&pathname[..], "/StatusNotifierItem"),
                Some(position) => pathname.split_at(position),
            };
            // bus name and object path validated on map entry insertion,
            // no further validation required
            let icon = c.with_proxy(bus_name, object_path, Duration::from_millis(1000));

            match item.event {
                ServerEvent::Activate { x, y } => icon.activate(x, y).unwrap_or_else(|e| {
                    eprintln!("->server error {:?}", e);
                }),
                ServerEvent::SecondaryActivate { x, y } => {
                    icon.secondary_activate(x, y).unwrap_or_else(|e| {
                        eprintln!("->server error {:?}", e);
                    })
                }
                ServerEvent::ContextMenu { x, y } => icon.context_menu(x, y).unwrap_or_else(|e| {
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

    let name_map = Arc::new(Mutex::new(HashMap::<String, (u64, Option<Path>)>::new()));
    let reverse_name_map = Arc::new(Mutex::new(HashMap::<u64, (String, Option<Path>)>::new()));
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
            if let Some(&(id, _)) = nm.get(&fullpath) {
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
            if let Some((nm, _)) = nm.get(&fullpath) {
                if let Ok(icon_pixmap) = icon.icon_pixmap() {
                    send_or_panic(IconClientEvent {
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
                    })
                } else if let Ok(icon_name) = icon.icon_name() {
                } else {
                    send_or_panic(IconClientEvent {
                        id: nm.clone(),
                        event: ClientEvent::RemoveIcon(IconType::Normal),
                    })
                }
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
            if let Some((nm, _)) = nm.get(&fullpath) {
                if let Ok(icon_pixmap) = icon.attention_icon_pixmap() {
                    send_or_panic(IconClientEvent {
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
                    })
                } else {
                    send_or_panic(IconClientEvent {
                        id: nm.clone(),
                        event: ClientEvent::RemoveIcon(IconType::Attention),
                    })
                }
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
            if let Some((nm, _)) = nm.get(&fullpath) {
                let id = *nm;
                if let Ok(icon_pixmap) = icon.overlay_icon_pixmap() {
                    send_or_panic(IconClientEvent {
                        id,
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
                    })
                } else {
                    send_or_panic(IconClientEvent {
                        id,
                        event: ClientEvent::RemoveIcon(IconType::Overlay),
                    })
                }
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

            if let Some((id, _)) = nm.get(&fullpath) {
                send_or_panic(IconClientEvent {
                    id: *id,
                    event: ClientEvent::Status(StatusNotifierItem::status(&icon).ok()),
                })
            }
            true
        },
    )?;

    let name_map_ = name_map.clone();
    let reverse_name_map_ = reverse_name_map.clone();
    let mut go =
        move |item: String, c: &SyncConnection| -> Result<(), Box<dyn std::error::Error>> {
            eprintln!("Going!");
            let (bus_name, object_path) = match item.find('/') {
                None => (&item[..], "/StatusNotifierItem"),
                Some(position) => item.split_at(position),
            };
            let bus_name = BusName::new(bus_name)?;
            let object_path = Path::new(object_path)?;
            let icon = c.with_proxy(bus_name.clone(), object_path, Duration::from_millis(1000));
            let app_id = icon.id()?;
            if app_id.starts_with("org.qubes-os.vm.") {
                return Ok(());
            }
            let category = icon.category()?;
            let menu = match icon.menu() {
                Ok(p) => Some(p),
                Err(e) => match e.name() {
                    Some("org.freedesktop.DBus.Error.NoSuchProperty") => None,
                    _ => return Err(e.into()),
                },
            };
            match &menu {
                Some(m) => {
                    let menu = c.with_proxy(bus_name.clone(), m, Duration::from_millis(1000));
                    let layout = menu.get_layout(0, -1, vec![])?;
                    eprintln!("Layout: {:?}", layout);
                }
                None => {}
            }
            index += 1;
            let id = index;
            eprintln!("Got new object {:?}, id {}", &item, id);
            send_or_panic(IconClientEvent {
                id,
                event: ClientEvent::Create {
                    category,
                    app_id,
                    has_menu: menu.is_some(),
                },
            });
            name_map_
                .lock()
                .expect("poisoned?")
                .insert(bus_name.to_string(), (id, menu.clone()));
            eprintln!(
                "Create event sent, {:?} added to reverse name map",
                &bus_name.to_string()
            );
            reverse_name_map_.lock().unwrap().insert(id, (item, menu));

            send_or_panic(IconClientEvent {
                id,
                event: ClientEvent::Status(StatusNotifierItem::status(&icon).ok()),
            });

            for (ty, fun) in [
                (IconType::Normal, icon.icon_pixmap()),
                (IconType::Attention, icon.attention_icon_pixmap()),
                (IconType::Overlay, icon.overlay_icon_pixmap()),
            ] {
                if let Ok(icon_pixmap) = fun {
                    send_or_panic(IconClientEvent {
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
                    })
                }
            }

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
                                 _c: &SyncConnection,
                                 _msg: &Message|
          -> bool {
        eprintln!("Name {:?} lost", &name);
        if old_owner.is_empty() || !new_owner.is_empty() {
            return true;
        }
        let id = match name_map.lock().expect("poisoned?").remove(&name) {
            Some(i) => i.0,
            None => return true,
        };
        eprintln!("Name {} lost, destroying icon {}", &name, id);
        reverse_name_map
            .lock()
            .unwrap()
            .remove(&id)
            .expect("reverse and forward maps inconsistent");
        send_or_panic(IconClientEvent {
            id,
            event: ClientEvent::Destroy,
        });
        true
    };

    watcher.match_signal(handle_notifier)?;

    bus_watcher.match_signal(handle_name_lost)?;

    loop {
        c.process(Duration::from_millis(1000))?;
    }
}
