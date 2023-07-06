use core::ffi::CStr;
use dbus::blocking::LocalConnection;
use dbus::channel::MatchingReceiver;
use dbus::message::SignalArgs;
use dbus::strings::ErrorName;
use dbus_crossroads::Crossroads;
use std::collections::HashMap;
use std::error::Error;
use std::io::Write;
use std::time::Duration;

use sni_icon::client::watcher::StatusNotifierWatcher;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::Receiver;

use sni_icon::*;

struct NotifierIcon {
    id: u64,
    category: String,

    tooltip: Option<Tooltip>,
    title: Option<String>,
    status: Option<String>,

    icon: Option<Vec<IconData>>,
    attention_icon: Option<Vec<IconData>>,
    overlay_icon: Option<Vec<IconData>>,
}

struct NotifierIconWrapper;

fn send_or_panic<T: bincode::Encode>(s: T) {
    let mut out = std::io::stdout().lock();
    bincode::encode_into_std_write(s, &mut out, bincode::config::standard())
        .expect("Cannot write to stdout");
    out.flush().expect("Cannot flush stdout");
}

fn call_with_icon<T, U: FnOnce(&mut NotifierIcon) -> Result<T, dbus::MethodErr>>(
    cb: U,
) -> Result<T, dbus::MethodErr> {
    WRAPPER.with(|items| {
        let mut items = items.borrow_mut();
        match ID.with(|id| items.get_mut(&id.get())) {
            None => {
                let err = unsafe {
                    // SAFETY: the preconditions are held
                    ErrorName::from_slice_unchecked("org.freedesktop.DBus.Error.ServiceUnknown\0")
                };
                Err((err, "Icon does not exist").into())
            }
            Some(icon) => cb(icon),
        }
    })
}

impl server::item::StatusNotifierItem for NotifierIconWrapper {
    fn context_menu(&mut self, x: i32, y: i32) -> Result<(), dbus::MethodErr> {
        call_with_icon(|icon| {
            send_or_panic(IconServerEvent {
                id: icon.id,
                event: ServerEvent::ContextMenu { x, y },
            });
            Ok(())
        })
    }
    fn activate(&mut self, x: i32, y: i32) -> Result<(), dbus::MethodErr> {
        call_with_icon(|icon| {
            send_or_panic(IconServerEvent {
                id: icon.id,
                event: ServerEvent::Activate { x, y },
            });
            Ok(())
        })
    }
    fn secondary_activate(&mut self, x: i32, y: i32) -> Result<(), dbus::MethodErr> {
        call_with_icon(|icon| {
            send_or_panic(IconServerEvent {
                id: icon.id,
                event: ServerEvent::SecondaryActivate { x, y },
            });
            Ok(())
        })
    }
    fn scroll(&mut self, delta: i32, orientation: String) -> Result<(), dbus::MethodErr> {
        call_with_icon(|icon| {
            send_or_panic(IconServerEvent {
                id: icon.id,
                event: ServerEvent::Scroll { delta, orientation },
            });
            Ok(())
        })
    }
    fn category(&self) -> Result<String, dbus::MethodErr> {
        call_with_icon(|icon| Ok(icon.category.clone()))
    }
    fn id(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property("Id"))
    }
    fn title(&self) -> Result<String, dbus::MethodErr> {
        call_with_icon(|icon| {
            icon.title
                .clone()
                .ok_or_else(|| dbus::MethodErr::no_property("Title"))
        })
    }
    fn status(&self) -> Result<String, dbus::MethodErr> {
        call_with_icon(|icon| {
            icon.status
                .clone()
                .ok_or_else(|| dbus::MethodErr::no_property("status"))
        })
    }
    fn window_id(&self) -> Result<i32, dbus::MethodErr> {
        Ok(0)
    }
    fn icon_theme_path(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property("icon_theme_path"))
    }
    fn menu(&self) -> Result<dbus::Path<'static>, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property("menu"))
    }
    fn item_is_menu(&self) -> Result<bool, dbus::MethodErr> {
        Ok(false)
    }
    fn icon_name(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property("IconName"))
    }
    fn icon_pixmap(&self) -> Result<Vec<(i32, i32, Vec<u8>)>, dbus::MethodErr> {
        call_with_icon(|icon| {
            Ok(icon
                .icon
                .as_ref()
                .map(|f| f.as_slice())
                .unwrap_or_else(|| &[])
                .iter()
                .map(|f| (f.width as i32, f.height as i32, f.data.clone()))
                .collect())
        })
    }
    fn overlay_icon_name(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property("OverlayIconName"))
    }
    fn overlay_icon_pixmap(&self) -> Result<Vec<(i32, i32, Vec<u8>)>, dbus::MethodErr> {
        call_with_icon(|overlay_icon| {
            Ok(overlay_icon
                .overlay_icon
                .as_ref()
                .map(|f| f.as_slice())
                .unwrap_or_else(|| &[])
                .iter()
                .map(|f| (f.width as i32, f.height as i32, f.data.clone()))
                .collect())
        })
    }
    fn attention_icon_name(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property("AttentionIconName"))
    }
    fn attention_icon_pixmap(&self) -> Result<Vec<(i32, i32, Vec<u8>)>, dbus::MethodErr> {
        call_with_icon(|attention_icon| {
            Ok(attention_icon
                .attention_icon
                .as_ref()
                .map(|f| f.as_slice())
                .unwrap_or_else(|| &[])
                .iter()
                .map(|f| (f.width as i32, f.height as i32, f.data.clone()))
                .collect())
        })
    }
    fn attention_movie_name(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property("AttentionMovieName"))
    }

    fn tool_tip(
        &self,
    ) -> Result<(String, Vec<(i32, i32, Vec<u8>)>, String, String), dbus::MethodErr> {
        call_with_icon(|tooltip| {
            let tooltip = tooltip
                .tooltip
                .as_ref()
                .ok_or_else(|| dbus::MethodErr::no_property("ToolTip"))?;
            let icon_data = tooltip
                .icon_data
                .iter()
                .map(|f| (f.width as i32, f.height as i32, f.data.clone()))
                .collect();
            Ok((
                String::new(),
                icon_data,
                tooltip.title.clone(),
                tooltip.description.clone(),
            ))
        })
    }
}

fn parse_dest(d: &dbus::strings::BusName, s: &str) -> Option<u64> {
    if d.len() <= s.len() {
        return None; // too short
    }
    let (first, rest) = d.split_at(s.len());
    if first != s {
        return None; // bad prefix
    }
    match rest.as_bytes()[0] {
        b'1'..=b'9' => <u64 as core::str::FromStr>::from_str(rest).ok(),
        _ => None,
    }
}

thread_local! {
    static WRAPPER: Rc<RefCell<HashMap<u64, NotifierIcon>>> = Rc::new(RefCell::new(<HashMap<u64, NotifierIcon>>::new()));
    static ID: std::cell::Cell<u64> = std::cell::Cell::new(0);
}

fn client_server(r: Receiver<IconClientEvent>) {
    let items = WRAPPER.with(|w| w.clone());
    let mut last_index = 0u64;
    let c = Rc::new(LocalConnection::new_session().unwrap());
    let pid = std::process::id();
    let bus_prefix = format!("org.freedesktop.StatusNotifierItem-{}-", pid);
    let mut cr = Crossroads::new();
    let iface_token = server::item::register_status_notifier_item::<NotifierIconWrapper>(&mut cr);
    cr.insert("/StatusNotifierItem", &[iface_token], NotifierIconWrapper);
    c.start_receive(
        dbus::message::MatchRule::new_method_call(),
        Box::new(move |msg, conn| {
            let destination = msg
                .destination()
                .expect("Method call with no destination should have been rejected by bus daemon!");
            if destination.starts_with(":") {
                if !msg.get_no_reply() {
                    use dbus::channel::Sender as _;
                    let err =
                        ErrorName::from_slice("org.freedesktop.DBus.Error.NotSupported\0").unwrap();
                    let human_readable = CStr::from_bytes_with_nul(
                        &b"Messages sent to a unique ID not supported\0"[..],
                    )
                    .unwrap();
                    conn.send(msg.error(&err, human_readable)).unwrap();
                }
            } else {
                let id = parse_dest(&destination, &bus_prefix)
                    .expect("bus daemon sent a message to name we never owned");
                ID.with(|c| c.set(id));
                cr.handle_message(msg, conn).unwrap();
            }
            true
        }),
    );

    let watcher = c.with_proxy(
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        Duration::from_millis(1000),
    );

    let path = unsafe {
        // SAFETY: the path is correct
        dbus::Path::from_slice_unchecked("/StatusNotifierItem\0")
    };
    dbus::strings::Interface::new("bogus").expect_err("no-string-validation must be off!");
    loop {
        c.process(Duration::from_millis(100)).unwrap();
        while let Some(item) = r.recv_timeout(Duration::from_millis(100)).ok() {
            let name = format!("org.freedesktop.StatusNotifierItem-{}-{}", pid, item.id);
            if let ClientEvent::Create { category } = &item.event {
                if item.id <= last_index {
                    panic!("Item ID not monotonically increasing");
                }
                last_index = item.id;
                eprintln!("Registering new item {}", &name);
                c.request_name(name.clone(), false, true, true)
                    .expect("Cannot acquire name bus name?");

                let notifier = NotifierIcon {
                    id: item.id.clone(),
                    category: category.clone(),

                    tooltip: None,
                    title: None,
                    status: None,
                    icon: None,
                    attention_icon: None,
                    overlay_icon: None,
                };
                watcher
                    .register_status_notifier_item(&format!("{}", name))
                    .unwrap();
                items.borrow_mut().insert(item.id, notifier);
            } else {
                let mut outer_ni = items.borrow_mut();
                let ni = outer_ni.get_mut(&item.id).unwrap();
                match item.event {
                    ClientEvent::Create { .. } => unreachable!(),
                    ClientEvent::Title(title) => {
                        ni.title = title;
                    }
                    ClientEvent::Status(status) => {
                        ni.status = status;
                    }
                    ClientEvent::Icon { typ, mut data } => {
                        for item in &mut data {
                            let mut set_pixel = |x: u32, y: u32| {
                                let base = ((y * item.width + x) * 4) as usize;
                                item.data[base] = 255;
                                item.data[base + 1] = 255;
                                item.data[base + 2] = 0;
                                item.data[base + 3] = 0;
                            };

                            for x in 0..2 {
                                for y in 0..item.height {
                                    set_pixel(x, y);
                                    set_pixel(item.width - 1 - x, y);
                                }
                            }

                            for y in 0..2 {
                                for x in 0..item.width {
                                    set_pixel(x, y);
                                    set_pixel(x, item.height - 1 - y);
                                }
                            }
                        }

                        match typ {
                            IconType::Normal => {
                                ni.icon = Some(data);
                                c.channel()
                                    .send(
                                        (server::item::StatusNotifierItemNewIcon {})
                                            .to_emit_message(&path),
                                    )
                                    .unwrap();
                            }
                            IconType::Attention => {
                                ni.attention_icon = Some(data);
                                c.channel()
                                    .send(
                                        (server::item::StatusNotifierItemNewAttentionIcon {})
                                            .to_emit_message(&path),
                                    )
                                    .unwrap();
                            }
                            IconType::Overlay => {
                                ni.overlay_icon = Some(data);
                                c.channel()
                                    .send(
                                        (server::item::StatusNotifierItemNewOverlayIcon {})
                                            .to_emit_message(&path),
                                    )
                                    .unwrap();
                            }
                        }
                    }
                    ClientEvent::RemoveIcon(typ) => match typ {
                        IconType::Normal => ni.icon = None,
                        IconType::Attention => ni.attention_icon = None,
                        IconType::Overlay => ni.overlay_icon = None,
                    },
                    ClientEvent::Tooltip {
                        icon_data,
                        title,
                        description,
                    } => {
                        ni.tooltip = Some(Tooltip {
                            title,
                            description,
                            icon_data: icon_data,
                        });
                    }
                    ClientEvent::RemoveTooltip => {
                        ni.tooltip = None;
                    }

                    ClientEvent::Destroy => {
                        c.release_name(name.clone())
                            .expect("Cannot release bus name?");
                        eprintln!("Released bus name {name}");
                        outer_ni.remove(&item.id);
                    }
                }
            }
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let (client_sender, client_receiver) = std::sync::mpsc::channel();

    std::thread::spawn(move || client_server(client_receiver));

    let mut stdin = std::io::stdin().lock();

    loop {
        let item = bincode::decode_from_std_read(&mut stdin, bincode::config::standard())?;
        match &item {
            IconClientEvent {
                id: _,
                event: ClientEvent::Icon { .. },
            } => {}
            _ => eprintln!("->client {:?}", item),
        }
        client_sender.send(item).unwrap();
    }
}
