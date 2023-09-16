use dbus::nonblock::{MsgMatch, Proxy, SyncConnection};
use dbus_tokio::connection;

use dbus::message::SignalArgs;
use dbus::strings::{BusName, Path};
use dbus::Message;

use std::collections::HashMap;
use std::error::Error;
use std::io::Write;
use std::time::Duration;

use sni_icon::client::item::StatusNotifierItem;
use sni_icon::client::watcher::StatusNotifierWatcher;
use sni_icon::names::*;
use sni_icon::*;

use core::cell::Cell;
use std::sync::{Arc, Mutex};

use crate::client::watcher::StatusNotifierWatcherStatusNotifierItemRegistered;
use futures_util::TryFutureExt as _;
use tokio::io::AsyncReadExt;

fn send_or_panic<T: bincode::Encode>(s: T) {
    let mut out = std::io::stdout().lock();
    let v = bincode::encode_to_vec(s, bincode::config::standard()).expect("Cannot encode data");
    eprintln!("Sending {} bytes", v.len());
    out.write_all(&((v.len() as u32).to_le_bytes())[..])
        .expect("cannot write to stdout");
    out.write_all(&v[..]).expect("cannot write to stdout");
    out.flush().expect("Cannot flush stdout");
}

async fn reader(reverse_name_map: Arc<Mutex<HashMap<u64, String>>>, c: Arc<SyncConnection>) {
    let mut stdin = tokio::io::stdin();
    loop {
        let size = stdin.read_u32_le().await.expect("error reading from stdin");
        eprintln!("Got something on stdin: length {}!", size);
        if size > 0x80_000_000 {
            panic!("Excessive message size {}", size);
        }
        let mut buffer = vec![0; size as _];
        let bytes_read = stdin
            .read_exact(&mut buffer[..])
            .await
            .expect("error reading from stdin");
        assert_eq!(bytes_read, buffer.len());
        eprintln!("{} bytes read!", bytes_read);
        let (item, size): (sni_icon::IconServerEvent, usize) =
            bincode::decode_from_slice(&buffer[..], bincode::config::standard())
                .expect("malformed message");
        if size != buffer.len() {
            panic!(
                "Malformed message on stdin: got {} bytes but expected {}",
                buffer.len(),
                size
            );
        }
        drop(buffer);
        eprintln!("->server {:?}", item);
        if let Some(pathname) = reverse_name_map.lock().unwrap().get(&item.id) {
            let (bus_name, object_path) = match pathname.find('/') {
                None => (&pathname[..], "/StatusNotifierItem"),
                Some(position) => pathname.split_at(position),
            };
            // bus name and object path validated on map entry insertion,
            // no further validation required
            let icon = Proxy::new(bus_name, object_path, Duration::from_millis(1000), &*c);

            match item.event {
                ServerEvent::Activate { x, y } => {
                    icon.activate(x, y)
                        .unwrap_or_else(|e| {
                            eprintln!("->server error {:?}", e);
                        })
                        .await
                }
                ServerEvent::SecondaryActivate { x, y } => {
                    icon.secondary_activate(x, y)
                        .unwrap_or_else(|e| {
                            eprintln!("->server error {:?}", e);
                        })
                        .await
                }
                ServerEvent::ContextMenu { x, y } => {
                    icon.context_menu(x, y)
                        .unwrap_or_else(|e| {
                            eprintln!("->server error {:?}", e);
                        })
                        .await
                }
                ServerEvent::Scroll { delta, orientation } => {
                    icon.scroll(delta, &orientation)
                        .unwrap_or_else(|e| {
                            eprintln!("->server error {:?}", e);
                        })
                        .await
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

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let local_set = tokio::task::LocalSet::new();
    // Let's start by starting up a connection to the session bus and request a name.
    let (resource, c) = connection::new_session_sync().unwrap();
    local_set.spawn_local(resource);
    let _x = local_set.spawn_local(client_server(c));
    local_set.await;
    Ok(())
}
thread_local! {
    static ID: std::cell::Cell<u64> = std::cell::Cell::new(0);
}
struct IconStats {
    id: u64,
    state: Cell<u8>,
}

fn handle_cb(
    msg: Message,
    c: Arc<SyncConnection>,
    flag: IconType,
    name_map: Arc<Mutex<HashMap<String, IconStats>>>,
) {
    let fullpath = format!("{}{}", msg.sender().unwrap(), msg.path().unwrap());
    {
        let nm = name_map.lock().unwrap();
        let nm = match nm.get(&fullpath) {
            Some(state) if state.state.get() & (flag as u8) != 0 => state,
            _ => return,
        };
        nm.state.set(flag as u8 | nm.state.get());
    }
    let name_map_ = name_map.clone();
    tokio::task::spawn_local(async move {
        let icon = Proxy::new(
            msg.sender().unwrap(),
            msg.path().unwrap(),
            Duration::from_millis(1000),
            &*c,
        );
        let nm = name_map_.lock().unwrap();
        let nm = match nm.get(&fullpath) {
            Some(state) => state,
            _ => return,
        };
        nm.state.set(flag as u8 | nm.state.get());
        match flag {
            IconType::Normal | IconType::Overlay | IconType::Attention => {
                if let Ok(icon_pixmap) = icon.icon_pixmap().await {
                    nm.state.set(!(flag as u8) | nm.state.get());
                    send_or_panic(IconClientEvent {
                        id: nm.id,
                        event: ClientEvent::Icon {
                            typ: flag,
                            data: icon_pixmap
                                .into_iter()
                                .map(|(w, h, data)| IconData {
                                    width: w as u32,
                                    height: h as u32,
                                    data,
                                })
                                .collect(),
                        },
                    })
                } else if let Ok(_icon_name) = icon.icon_name().await {
                    nm.state.set(!(flag as u8) | nm.state.get());
                } else {
                    nm.state.set(!(flag as u8) | nm.state.get());
                    send_or_panic(IconClientEvent {
                        id: nm.id,
                        event: ClientEvent::RemoveIcon(flag),
                    })
                }
            }
            IconType::Title => {
                let title = icon.title().await;
                nm.state.set(!(flag as u8) | nm.state.get());
                send_or_panic(IconClientEvent {
                    id: nm.id,
                    event: ClientEvent::Title(title.ok()),
                })
            }

            IconType::Status => {
                let status = StatusNotifierItem::status(&icon).await;
                nm.state.set(!(flag as u8) | nm.state.get());
                send_or_panic(IconClientEvent {
                    id: nm.id,
                    event: ClientEvent::Status(status.ok()),
                })
            }
        }
    });
}

async fn client_server(c: Arc<SyncConnection>) -> Result<(MsgMatch, MsgMatch), Box<dyn Error>> {
    let watcher = Proxy::new(
        name_status_notifier_watcher(),
        path_status_notifier_watcher(),
        Duration::from_millis(1000),
        c.clone(),
    );
    eprintln!("Created watcher proxy!");
    let name_map = Arc::new(Mutex::new(HashMap::<String, IconStats>::new()));
    let reverse_name_map = Arc::new(Mutex::new(HashMap::<u64, String>::new()));
    let reverse_name_map_ = reverse_name_map.clone();
    tokio::task::spawn_local(reader(reverse_name_map_, c.clone()));
    eprintln!("Spawned reader future!");
    let c_ = c.clone();
    let name_map_ = name_map.clone();
    c.add_match(client::item::StatusNotifierItemNewStatus::match_rule(
        None, None,
    ))
    .await?
    .cb(move |msg, _: ()| {
        handle_cb(msg, c_.clone(), IconType::Status, name_map_.clone());
        true
    });
    eprintln!("Added status match!");
    let c_ = c.clone();
    let name_map_ = name_map.clone();
    c.add_match(client::item::StatusNotifierItemNewTitle::match_rule(
        None, None,
    ))
    .await?
    .cb(move |msg, _: ()| {
        handle_cb(msg, c_.clone(), IconType::Title, name_map_.clone());
        true
    });

    async fn go(
        item: String,
        c: Arc<SyncConnection>,
        name_map: Arc<Mutex<HashMap<String, IconStats>>>,
        reverse_name_map: Arc<Mutex<HashMap<u64, String>>>,
    ) -> Result<(), Box<dyn Error>> {
        eprintln!("Going!");
        let (bus_name, object_path) = match item.find('/') {
            None => (&item[..], "/StatusNotifierItem"),
            Some(position) => item.split_at(position),
        };
        eprintln!(
            "Bus name is {:?}, object path is {:?}",
            bus_name, object_path
        );
        let bus_name = BusName::new(bus_name).map_err(|x| {
            eprintln!("Bad bus name {:?}", x);
            x
        })?;
        let object_path = Path::new(object_path).map_err(|x| {
            eprintln!("Bad object path {:?}", x);
            x
        })?;
        eprintln!("Object path is {}", object_path);
        let icon = Proxy::new(
            bus_name.clone(),
            object_path.clone(),
            Duration::from_millis(1000),
            c.clone(),
        );
        let (app_id, category, is_menu, status) = futures_util::join!(
            icon.id(),
            icon.category(),
            icon.item_is_menu(),
            StatusNotifierItem::status(&icon)
        );
        let app_id = app_id.map_err(|x| {
            eprintln!("Oops! Cannot obtain app ID: {}", x);
            x
        })?;
        eprintln!("App ID is {:?}", app_id);

        let is_menu = is_menu.unwrap_or(false);
        eprintln!("Is menu: {}", is_menu);
        if app_id.starts_with("org.qubes_os.vm.") {
            return Result::<(), Box<dyn std::error::Error>>::Ok(());
        }
        let category = category?;
        let id = ID.with(|id| id.get()) + 1;
        ID.with(|x| x.set(id));
        eprintln!("Got new object {:?}, id {}", &item, id);
        send_or_panic(IconClientEvent {
            id,
            event: ClientEvent::Create {
                category,
                app_id,
                is_menu,
            },
        });
        name_map.lock().unwrap().insert(
            bus_name.to_string(),
            IconStats {
                id,
                state: Cell::new(0),
            },
        );
        eprintln!(
            "Create event sent, {:?} added to reverse name map",
            &bus_name.to_string()
        );
        reverse_name_map.lock().unwrap().insert(id, item);

        send_or_panic(IconClientEvent {
            id,
            event: ClientEvent::Status(status.ok()),
        });
        let (normal, attention, overlay) = futures_util::join!(
            icon.icon_pixmap(),
            icon.attention_icon_pixmap(),
            icon.overlay_icon_pixmap()
        );
        for (ty, fun) in [
            (IconType::Normal, normal),
            (IconType::Attention, attention),
            (IconType::Overlay, overlay),
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
                                data,
                            })
                            .collect(),
                    },
                })
            }
        }

        eprintln!("Returning from go()");
        Ok::<(), _>(())
    }

    for item in watcher.registered_status_notifier_items().await? {
        tokio::task::spawn_local(go(
            item,
            c.clone(),
            name_map.clone(),
            reverse_name_map.clone(),
        ));
    }

    let c_ = c.clone();
    let (name_map_, reverse_name_map_) = (name_map.clone(), reverse_name_map.clone());
    let handle_notifier = move |_msg: Message, (s,): (String,)| -> bool {
        eprintln!("Picked up registered event");
        tokio::task::spawn_local(go(
            s,
            c_.clone(),
            name_map_.clone(),
            reverse_name_map_.clone(),
        ));
        true
    };

    let matcher1 = c
        .add_match(StatusNotifierWatcherStatusNotifierItemRegistered::match_rule(None, None))
        .await?
        .cb(handle_notifier);
    let x = dbus::message::MatchRule::new_signal(interface_dbus(), name_owner_changed())
        .with_strict_sender(name_dbus())
        .with_path(path_dbus());
    let matcher2 = c.add_match(x).await?.cb(move |m, n| {
        handle_name_lost(&c, m, n, name_map.clone(), reverse_name_map.clone());
        true
    });
    Ok((matcher1, matcher2))
}

fn handle_name_lost(
    _c: &Arc<SyncConnection>,
    _msg: Message,
    NameOwnerChanged {
        name,
        old_owner,
        new_owner,
    }: NameOwnerChanged,
    name_map: Arc<Mutex<HashMap<String, IconStats>>>,
    reverse_name_map: Arc<Mutex<HashMap<u64, String>>>,
) {
    eprintln!("Name {:?} lost", &name);
    if old_owner.is_empty() || !new_owner.is_empty() {
        return;
    }
    let id = match name_map.lock().unwrap().remove(&name) {
        Some(stats) => stats.id,
        None => return,
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
    })
}
