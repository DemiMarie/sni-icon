use dbus::nonblock::{LocalConnection, LocalMsgMatch, Proxy};
use dbus_tokio::connection;

use dbus::arg::{ArgType, Iter};
use dbus::message::SignalArgs;
use dbus::strings::{BusName, Path, Signature};
use dbus::Message;

use std::collections::HashMap;
use std::error::Error;
use std::io::Write;
use std::time::Duration;

use sni_icon::client::item::StatusNotifierItem;
use sni_icon::client::menu::Dbusmenu;
use sni_icon::client::watcher::StatusNotifierWatcher;
use sni_icon::names::*;
use sni_icon::*;

use core::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

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

async fn reader(
    reverse_name_map: Rc<RefCell<HashMap<u64, (String, Option<dbus::Path<'_>>)>>>,
    c: Arc<LocalConnection>,
) {
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
            bincode::decode_from_slice(&mut buffer[..], bincode::config::standard())
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
        if let Some((pathname, _)) = reverse_name_map.borrow().get(&item.id) {
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
#[derive(Debug)]
struct Menu {
    revision: u32,
    layout: MenuEntries,
}
#[derive(Debug)]
struct MenuEntries(DBusMenuEntry);
impl dbus::arg::Arg for MenuEntries {
    const ARG_TYPE: ArgType = ArgType::Struct;
    fn signature() -> Signature<'static> {
        // SAFETY: The string is a valid D-Bus signature and is NUL-terminated.
        unsafe { Signature::from_slice_unchecked("(ia{sv}av)\0") }
    }
}

fn get(i: &mut Iter, parent: i32, depth: u32) -> Option<MenuEntries> {
    eprintln!("Calling MenuEntries::get");
    let mut x = i.recurse(ArgType::Struct)?;
    eprintln!("Recursed into first struct");

    let id: i32 = x.get()?;
    assert!(x.next());
    eprintln!("Got ID {}", id);
    let mut properties = x.recurse(ArgType::Array)?;
    eprintln!("Entered properties dict");
    let mut entry_counter = 0;
    let mut is_separator = None;
    let mut label: Option<String> = None;
    let mut enabled: Option<bool> = None;
    let mut visible: Option<bool> = None;
    let mut children_display: Option<bool> = None;
    let mut has_next = true;
    let mut disposition: Option<Disposition> = None;

    while has_next {
        let mut dict_entry = properties.recurse(ArgType::DictEntry)?;
        has_next = properties.next();
        eprintln!("Found a dict entry");
        entry_counter += 1;
        let prop_name: String = dict_entry.get()?;
        eprintln!("Property name is {prop_name:?}");
        assert!(dict_entry.next());
        let mut variant_value = dict_entry.recurse(ArgType::Variant)?;
        eprintln!("Property value is {variant_value:?}");
        match &*prop_name {
            "type" => {
                // string: "standard" or "separator"
                if is_separator
                    .replace(match &*variant_value.get::<String>()? {
                        "standard" => false,
                        "separator" => true,
                        _ => {
                            eprintln!("Invalid entry type");
                            return None;
                        }
                    })
                    .is_some()
                {
                    eprintln!("Multiple type values not allowed");
                    return None;
                }
            }
            "label" => {
                // string, with special handling of underscores
                if label.replace(variant_value.get()?).is_some() {
                    eprintln!("Multiple labels not allowed");
                    return None;
                }
            }
            "enabled" => {
                // boolean - true if item can be activated
                if enabled.replace(variant_value.get()?).is_some() {
                    eprintln!("Multiple enabled values not allowed");
                    return None;
                }
            }
            "visible" => {
                // boolean - true if item is visible
                if visible.replace(variant_value.get()?).is_some() {
                    eprintln!("Multiple visible values not allowed");
                    return None;
                }
            }
            "icon-name" => {} // string - icon name
            "icon-data" => { // bytes - PNG icon data
                 // FIXME: does this need to be decompressed in the guest or can
                 // the PNG crate on the host be trusted with malicious PNG?
            }
            "shortcut" => {
                // array of length-2 arrays of strings - shortcut keys
                let mut outer_array = variant_value.recurse(ArgType::Array)?;
                let mut has_next = true;
                while has_next {
                    let mut inner_array = outer_array.recurse(ArgType::Array)?;
                    has_next = outer_array.next();
                    let _s1: String = inner_array.get()?;
                    if !inner_array.next() {
                        return None;
                    }
                    let _s2: String = inner_array.get()?;
                    if inner_array.next() {
                        return None;
                    }
                }
            }
            "toggle-type" => {
                // string, either "checkmark", "radio", or "" (not togglable)
                match &*variant_value.get::<String>()? {
                    "checkmark" | "radio" | "" => {}
                    _ => {
                        eprintln!("Invalid toggle type");
                        return None;
                    }
                }
            }
            "toggle-state" => {
                // integer (i32), either 0 (not toggled), 1 (toggled), or
                // something else (indeterminate), default -1
            }
            "children-display" => {
                eprintln!("children-display set");
                // "submenu" if there are children, otherwise ""
                if children_display
                    .replace(match &*variant_value.get::<String>()? {
                        "submenu" => true,
                        "" => false,
                        _ => {
                            eprintln!("Invalid submenu type");
                            return None;
                        }
                    })
                    .is_some()
                {
                    eprintln!("children-display occurs twice");
                    return None;
                }
            }
            "disposition" => {
                if disposition
                    .replace(match &*variant_value.get::<String>()? {
                        "normal" => Disposition::Normal,
                        "informative" => Disposition::Informative,
                        "warning" => Disposition::Warning,
                        "alert" => Disposition::Alert,
                        _ => {
                            eprintln!("Invalid disposition");
                            return None;
                        }
                    })
                    .is_some()
                {
                    eprintln!("Cannot specify disposition more than once");
                    return None;
                }
            } // "normal", "informative", "warning", or "alert"
            x if x.starts_with("x-") => {} // ignored, but valid
            x => eprintln!("Invalid property name {:?}", x),
        }
    }

    eprintln!("All properties read");
    if !x.next() {
        eprintln!("No subentry array");
        return None;
    }
    let mut subentries = x.recurse(ArgType::Array)?;
    let mut children = vec![];
    if children_display.unwrap_or(false) {
        eprintln!(
            "Processing submenus, content of type {:?}",
            subentries.arg_type()
        );
        has_next = true;
        while has_next {
            let mut submenu = subentries.recurse(ArgType::Variant)?;
            has_next = subentries.next();
            eprintln!("Entered variant, content of type {:?}", submenu.arg_type());
            children.push(get(&mut submenu, id, depth + 1)?.0);
        }
    } else if subentries.next() {
        eprintln!("Submenu entries but submenu-display not set");
        return None;
    }

    if i.next() {
        return None;
    }

    if is_separator.unwrap_or(false) {
        if entry_counter != if visible.is_some() { 2 } else { 1 } {
            eprintln!("Separators must not have properties other than \u{201c}visible\u{201d}");
            return None;
        }

        return Some(MenuEntries(DBusMenuEntry::Separator {
            visible: visible.unwrap_or(true),
        }));
    } else {
        return Some(MenuEntries(DBusMenuEntry::Standard {
            label: label.unwrap_or_else(String::new),
            access_key: None,
            visible: visible.unwrap_or(false),
            children,
            disposition: disposition.unwrap_or(Disposition::Normal),
            id: unsafe { std::mem::transmute::<i32, Option<core::num::NonZeroI32>>(id) },
            depth: 0,
            parent: None,
        }));
    }
}

impl<'a> dbus::arg::Get<'a> for MenuEntries {
    fn get(i: &mut Iter<'a>) -> Option<Self> {
        get(i, 0, 0)
    }
}

impl dbus::arg::ReadAll for Menu {
    fn read(i: &mut dbus::arg::Iter<'_>) -> Result<Self, dbus::arg::TypeMismatchError> {
        Ok(Self {
            revision: i.read()?,
            layout: i.read()?,
        })
    }
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
    let (resource, c) = connection::new_session_local().unwrap();
    local_set.spawn_local(resource);
    let _x = local_set.spawn_local(client_server(c));
    Ok(local_set.await)
}
thread_local! {
    static ID: std::cell::Cell<u64> = std::cell::Cell::new(0);
}
struct IconStats {
    id: u64,
    state: Cell<u8>,
    path: Path<'static>,
    menu: Option<Path<'static>>,
}

fn handle_cb(
    msg: Message,
    c: Arc<LocalConnection>,
    flag: IconType,
    name_map: Rc<RefCell<HashMap<String, IconStats>>>,
) -> () {
    let fullpath = format!("{}{}", msg.sender().unwrap(), msg.path().unwrap());
    {
        let nm = name_map.borrow();
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
        let nm = name_map_.borrow();
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

async fn client_server(
    c: Arc<LocalConnection>,
) -> Result<(LocalMsgMatch, LocalMsgMatch), Box<dyn Error>> {
    let watcher = Proxy::new(
        name_status_notifier_watcher(),
        path_status_notifier_watcher(),
        Duration::from_millis(1000),
        c.clone(),
    );
    eprintln!("Created watcher proxy!");
    let name_map = Rc::new(RefCell::new(HashMap::<String, IconStats>::new()));
    let reverse_name_map = Rc::new(RefCell::new(HashMap::<u64, (String, Option<Path>)>::new()));
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
        c: Arc<LocalConnection>,
        name_map: Rc<RefCell<HashMap<String, IconStats>>>,
        reverse_name_map: Rc<RefCell<HashMap<u64, (String, Option<Path<'static>>)>>>,
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
        let bus_name = BusName::new(bus_name)?;
        let object_path = Path::new(object_path)?;
        let icon = Proxy::new(
            bus_name.clone(),
            object_path.clone(),
            Duration::from_millis(1000),
            c.clone(),
        );
        let (app_id, category, menu, status) = futures_util::join!(
            icon.id(),
            icon.category(),
            icon.menu(),
            StatusNotifierItem::status(&icon)
        );
        let app_id = app_id?;
        if app_id.starts_with("org.qubes_os.vm.") {
            return Result::<(), Box<dyn std::error::Error>>::Ok(());
        }
        let category = category?;
        let menu = match menu {
            Ok(p) => Some(p),
            Err(e) => match e.name() {
                Some("org.freedesktop.DBus.Error.NoSuchProperty") => None,
                _ => return Err(e.into()),
            },
        };
        let menu_object = match &menu {
            Some(m) => {
                let menu = Proxy::new(bus_name.clone(), m, Duration::from_millis(1000), c);

                eprintln!("Issuing method call!");
                let iter: Result<Menu, _> = menu
                    .method_call(
                        interface_com_canonical_dbusmenu(),
                        get_layout(),
                        (0i32, -1i32, Vec::<&str>::new()),
                    )
                    .await;

                eprintln!("Got menu!");
                Some(iter?)
            }
            None => None,
        };
        let x = layout_updated(bus_name.clone(), object_path.clone());
        let id = ID.with(|id| id.get()) + 1;
        ID.with(|x| x.set(id));
        eprintln!("Got new object {:?}, id {}", &item, id);
        send_or_panic(IconClientEvent {
            id,
            event: ClientEvent::Create {
                category,
                app_id,
                has_menu: menu.is_some(),
            },
        });
        name_map.borrow_mut().insert(
            bus_name.to_string(),
            IconStats {
                id,
                state: Cell::new(0),
                path: object_path,
                menu: menu.clone(),
            },
        );
        eprintln!(
            "Create event sent, {:?} added to reverse name map",
            &bus_name.to_string()
        );
        reverse_name_map.borrow_mut().insert(id, (item, menu));

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
    let matcher2 = c
        .add_match(x)
        .await?
        .cb(move |m, n| handle_name_lost(m, n, name_map.clone(), reverse_name_map.clone()));
    Ok((matcher1, matcher2))
}

fn handle_name_lost(
    _msg: Message,
    NameOwnerChanged {
        name,
        old_owner,
        new_owner,
    }: NameOwnerChanged,
    name_map: Rc<RefCell<HashMap<String, IconStats>>>,
    reverse_name_map: Rc<RefCell<HashMap<u64, (String, Option<Path<'static>>)>>>,
) -> bool {
    eprintln!("Name {:?} lost", &name);
    if old_owner.is_empty() || !new_owner.is_empty() {
        return true;
    }
    let id = match name_map.borrow_mut().remove(&name) {
        Some(i) => i.id,
        None => return true,
    };
    eprintln!("Name {} lost, destroying icon {}", &name, id);
    reverse_name_map
        .borrow_mut()
        .remove(&id)
        .expect("reverse and forward maps inconsistent");
    send_or_panic(IconClientEvent {
        id,
        event: ClientEvent::Destroy,
    });
    true
}
