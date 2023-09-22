#![forbid(clippy::correctness)]
#![forbid(clippy::cargo)]
#![forbid(clippy::suspicious)]
#![forbid(clippy::undocumented_unsafe_blocks)]
use dbus::channel::{MatchingReceiver as _, Sender as _};
use dbus::nonblock::{MsgMatch, Proxy, SyncConnection};
use dbus_crossroads::Crossroads;
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

struct Watcher {
    items: Arc<Mutex<std::collections::HashSet<String>>>,
    hosts: Arc<Mutex<std::collections::HashSet<String>>>,
    connection: Arc<SyncConnection>,
    _msg_match: MsgMatch,
}

impl Watcher {
    async fn new(connection: Arc<SyncConnection>) -> Result<Watcher, dbus::MethodErr> {
        let items = Arc::new(Mutex::new(std::collections::HashSet::default()));
        let hosts = Arc::new(Mutex::new(std::collections::HashSet::default()));
        let items2 = items.clone();
        let hosts2 = hosts.clone();
        let connection_ = connection.clone();
        let name_owner_changed_cb = move |connection_: &Arc<SyncConnection>,
                                          _msg: Message,
                                          NameOwnerChanged {
                                              name,
                                              old_owner: _,
                                              new_owner,
                                          }| {
            hosts2.lock().unwrap().remove(&name);
            if new_owner.is_empty() && items2.lock().unwrap().remove(&name) {
                match connection_.send(
                    (server::watcher::StatusNotifierWatcherStatusNotifierItemUnregistered {
                        arg0: name.clone(),
                    })
                    .to_emit_message(&"/StatusNotifierWatcher".into()),
                ) {
                    Ok(_) => eprintln!("Removed name {:?}", name),
                    Err(()) => eprintln!("Message send failed"),
                };
                match connection_.send(
                    dbus::nonblock::stdintf::org_freedesktop_dbus::PropertiesPropertiesChanged {
                        interface_name: "org.kde.StatusNotifierWatcher".to_owned(),
                        changed_properties: Default::default(),
                        invalidated_properties: vec!["RegisteredStatusNotifierItems".to_owned()],
                    }
                    .to_emit_message(&"/StatusNotifierWatcher".into()),
                ) {
                    Ok(_) => eprintln!("Properties invalidated to indicate disconnection"),
                    Err(()) => eprintln!("Message send failed"),
                }
            }

            true
        };
        eprintln!(
            "Requesting bus name {}",
            names::name_status_notifier_watcher()
        );
        connection
            .request_name(names::name_status_notifier_watcher(), false, true, false)
            .await
            .expect("Cannot connect to bus");
        eprintln!(
            "Received bus name {}",
            names::name_status_notifier_watcher()
        );
        let x = dbus::message::MatchRule::new_signal(interface_dbus(), name_owner_changed())
            .with_strict_sender(name_dbus())
            .with_path(path_dbus());
        eprintln!("Match rule created");
        let _msg_match = connection
            .add_match(x)
            .await?
            .cb(move |m, n| name_owner_changed_cb(&connection_, m, n));
        eprintln!("Match rule added");

        Ok(Self {
            items,
            hosts,
            connection,
            _msg_match,
        })
    }
}

impl server::watcher::StatusNotifierWatcher for Watcher {
    fn register_status_notifier_item(&mut self, service: String) -> Result<(), dbus::MethodErr> {
        // FIXME: validate
        self.items.lock().unwrap().insert(service.clone());
        match self.connection.send(
            (server::watcher::StatusNotifierWatcherStatusNotifierItemRegistered { arg0: service })
                .to_emit_message(&"/StatusNotifierWatcher".into()),
        ) {
            Ok(_) => eprintln!("Item registered"),
            Err(()) => eprintln!("Message send failed"),
        };
        match self.connection.send(
            dbus::nonblock::stdintf::org_freedesktop_dbus::PropertiesPropertiesChanged {
                interface_name: "org.kde.StatusNotifierWatcher".to_owned(),
                changed_properties: Default::default(),
                invalidated_properties: vec!["RegisteredStatusNotifierItems".to_owned()],
            }
            .to_emit_message(&"/StatusNotifierWatcher".into()),
        ) {
            Ok(_) => eprintln!("Properties invalidated"),
            Err(()) => eprintln!("Message send failed"),
        }
        Ok(())
    }
    fn register_status_notifier_host(&mut self, service: String) -> Result<(), dbus::MethodErr> {
        self.hosts.lock().unwrap().insert(service);
        match self.connection.send(
            (server::watcher::StatusNotifierWatcherStatusNotifierHostRegistered {})
                .to_emit_message(&"/StatusNotifierWatcher".into()),
        ) {
            Ok(_) => {}
            Err(()) => eprintln!("Message send failed"),
        };
        Ok(())
    }
    fn registered_status_notifier_items(&self) -> Result<Vec<String>, dbus::MethodErr> {
        Ok(self.items.lock().unwrap().iter().cloned().collect())
    }
    fn is_status_notifier_host_registered(&self) -> Result<bool, dbus::MethodErr> {
        Ok(!self.hosts.lock().unwrap().is_empty())
    }
    fn protocol_version(&self) -> Result<i32, dbus::MethodErr> {
        Ok(1) // used by Swaybar
    }
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
        let lock = reverse_name_map
            .lock()
            .unwrap()
            .get(&item.id)
            .map(|x| x.to_owned());
        if let Some(pathname) = lock {
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
    let (resource, c) = connection::new_session_sync()?;
    local_set.spawn_local(resource);
    let (resource, c2) = connection::new_session_sync()?;
    local_set.spawn_local(resource);
    let _x = local_set.spawn_local(client_server(c, c2));
    local_set.await;
    eprintln!("Returning from main()");
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
    let sender = msg
        .sender()
        .expect("D-Bus will not send a message with no sender");
    let path = msg
        .path()
        .expect("D-Bus will not send a message with no path");
    let fullpath = format!("{}{}", sender, path);
    {
        let nm = name_map.lock().unwrap();
        let nm = match nm.get(&fullpath) {
            Some(state) if state.state.get() & (flag as u8) == 0 => state,
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
        {
            let nm = name_map_.lock().expect("mutex should not be poisoned");
            let nm = match nm.get(&fullpath) {
                Some(state) => state,
                _ => return, // Icon does not exist
            };
            nm.state.set(flag as u8 | nm.state.get());
        }
        match flag {
            IconType::Normal | IconType::Overlay | IconType::Attention => {
                if let Ok(icon_pixmap) = icon.icon_pixmap().await {
                    let nm = name_map_.lock().expect("mutex should not be poisoned");
                    let nm = match nm.get(&fullpath) {
                        Some(state) => state,
                        _ => return, // Icon does not exist
                    };
                    nm.state.set(!(flag as u8) & nm.state.get());
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
                    let nm = name_map_.lock().expect("mutex should not be poisoned");
                    let nm = match nm.get(&fullpath) {
                        Some(state) => state,
                        _ => return, // Icon does not exist
                    };
                    nm.state.set(!(flag as u8) & nm.state.get());
                } else {
                    let nm = name_map_.lock().expect("mutex should not be poisoned");
                    let nm = match nm.get(&fullpath) {
                        Some(state) => state,
                        _ => return, // Icon does not exist
                    };
                    nm.state.set(!(flag as u8) & nm.state.get());
                    send_or_panic(IconClientEvent {
                        id: nm.id,
                        event: ClientEvent::RemoveIcon(flag),
                    })
                }
            }
            IconType::Title => {
                let title = icon.title().await;
                let nm = name_map_.lock().expect("mutex should not be poisoned");
                let nm = match nm.get(&fullpath) {
                    Some(state) => state,
                    _ => return, // Icon does not exist
                };
                nm.state.set(!(flag as u8) | nm.state.get());
                send_or_panic(IconClientEvent {
                    id: nm.id,
                    event: ClientEvent::Title(title.ok()),
                })
            }

            IconType::Status => {
                let status = StatusNotifierItem::status(&icon).await;
                let nm = name_map_.lock().expect("mutex should not be poisoned");
                let nm = match nm.get(&fullpath) {
                    Some(state) => state,
                    _ => return, // Icon does not exist
                };
                nm.state.set(!(flag as u8) | nm.state.get());
                send_or_panic(IconClientEvent {
                    id: nm.id,
                    event: ClientEvent::Status(status.ok()),
                })
            }
        }
    });
}

async fn client_server(
    c: Arc<SyncConnection>,
    c2: Arc<SyncConnection>,
) -> Result<(MsgMatch, MsgMatch), Box<dyn Error>> {
    {
        let cr = Arc::new(Mutex::new(Crossroads::new()));

        let iface_token_1 = server::watcher::register_status_notifier_watcher::<Watcher>(
            &mut cr.lock().expect("mutex should not be poisoned"),
        );
        let watcher = Watcher::new(c2.clone())
            .await
            .expect("watcher should be successfully created");
        cr.lock().expect("mutex should not be poisoned").insert(
            names::path_status_notifier_watcher(),
            &[iface_token_1],
            watcher,
        );
        c2.start_receive(
            dbus::message::MatchRule::new_method_call(),
            Box::new(move |msg, conn| {
                let mut x = cr.lock().expect("lock should not be poisoned");
                (*x).handle_message(msg, conn).is_ok()
            }),
        );
    }

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
        name_map
            .lock()
            .expect("mutex should not be poisoned")
            .insert(
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
        reverse_name_map
            .lock()
            .expect("mutex should not be poisoned")
            .insert(id, item);

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
    eprintln!(
        "Got NameOwnerChanged: name {:?}, old owner {:?}, new owner {:?}",
        name, old_owner, new_owner
    );
    if old_owner.is_empty() || !new_owner.is_empty() {
        return;
    }
    eprintln!("Name {:?} lost", &name);
    let id = match name_map
        .lock()
        .expect("mutex should not be poisoned")
        .remove(&name)
    {
        Some(stats) => stats.id,
        None => return,
    };
    eprintln!("Name {} lost, destroying icon {}", &name, id);
    reverse_name_map
        .lock()
        .expect("mutex should not be poisoned")
        .remove(&id)
        .expect("reverse and forward maps inconsistent");
    send_or_panic(IconClientEvent {
        id,
        event: ClientEvent::Destroy,
    })
}
