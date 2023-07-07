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

use sha2::{Digest as _, Sha256};

struct NotifierIcon {
    id: u64,
    category: String,
    app_id: String,

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
        call_with_icon(|icon| Ok(icon.app_id.clone()))
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

fn parse_dest(d: &str, prefix: &str, suffix: &str) -> Option<u64> {
    let (total_len, prefix_len, suffix_len) = (d.len(), prefix.len(), suffix.len());
    if total_len <= prefix_len + suffix_len {
        return None; // too short
    }
    let suffix_start = total_len - suffix_len;
    let (first, middle, rest) = (
        &d[..prefix_len],
        &d[prefix_len..suffix_start],
        &d[suffix_start..],
    );
    eprintln!("First {:?}, middle {:?}, last {:?}", first, middle, rest);
    if first != prefix {
        eprintln!(
            "First part {:?} does not match expected prefix {:?}",
            first, prefix
        );
        return None;
    }
    if rest != suffix {
        eprintln!(
            "Last part {:?} does not match expected suffix {:?}",
            first, suffix
        );
        return None; // bad prefix or suffix
    }
    let first_byte = middle.as_bytes()[0];
    match first_byte {
        b'1'..=b'9' => match <u64 as core::str::FromStr>::from_str(middle) {
            Err(e) => {
                eprintln!("Parsing error for {:?}: {}", middle, e);
                None
            }
            Ok(0) => unreachable!("0 does not start with 1 through 9"),
            Ok(x) => Some(x),
        },
        _ => {
            eprintln!("Bad first byte {:?}", first_byte);
            None
        }
    }
}

fn bus_path(id: u64) -> dbus::Path<'static> {
    format!("/{}/StatusNotifierItem\0", id).into()
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
    let cr = Rc::new(RefCell::new(Crossroads::new()));
    let cr_ = cr.clone();
    c.start_receive(
        dbus::message::MatchRule::new_method_call(),
        Box::new(move |msg, conn| {
            use dbus::channel::Sender as _;
            let destination = msg.destination().expect(
                "Method call with no destination should not have been forwarded by bus daemon!",
            );
            let path = msg
                .path()
                .expect("Method call with no path should have been rejected by libdbus");
            let maybe_id = parse_dest(&path, &"/", &"/StatusNotifierItem");
            let dest_id = if destination.starts_with(":") {
                None
            } else {
                match parse_dest(&destination, &bus_prefix, &"") {
                    None if !msg.get_no_reply() => {
                        let err =
                            ErrorName::from_slice("org.freedesktop.DBus.Error.NameHasNoOwner\0")
                                .unwrap();
                        let human_readable = format!(
                            "Message sent to name {} we never owned (prefix {})\0",
                            destination, bus_prefix
                        );
                        conn.send(msg.error(
                            &err,
                            CStr::from_bytes_with_nul(human_readable.as_bytes()).unwrap(),
                        ))
                        .expect("dbus msg send fail");
                        return true;
                    }
                    None => return true,
                    Some(id) => Some(id),
                }
            };
            match (maybe_id, dest_id) {
                (Some(id1), Some(id2)) if id1 != id2 => {
                    if msg.get_no_reply() {
                        return true;
                    }
                    let err = ErrorName::from_slice("org.freedesktop.DBus.Error.UnknownObject\0")
                        .unwrap();
                    let human_readable = format!("Message sent to unknown object path {}\0", &path);
                    conn.send(msg.error(
                        &err,
                        CStr::from_bytes_with_nul(human_readable.as_bytes()).unwrap(),
                    ))
                    .expect("dbus msg send fail");
                }
                (Some(id), _) => {
                    ID.with(|id_| id_.set(id));
                    cr.borrow_mut().handle_message(msg, conn).unwrap();
                }
                (None, _) => cr.borrow_mut().handle_message(msg, conn).unwrap(),
            };
            true
        }),
    );

    let watcher = c.with_proxy(
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        Duration::from_millis(1000),
    );

    dbus::strings::Interface::new("bogus").expect_err("no-string-validation must be off!");
    loop {
        c.process(Duration::from_millis(100)).unwrap();
        while let Some(item) = r.recv_timeout(Duration::from_millis(100)).ok() {
            let name = format!("org.freedesktop.StatusNotifierItem-{}-{}", pid, item.id);
            if let ClientEvent::Create { category, app_id } = &item.event {
                const PREFIX: &'static str = "org.qubes-os.vm.app-id.";
                let app_id = PREFIX.to_owned() + app_id;
                if item.id <= last_index {
                    panic!("Item ID not monotonically increasing");
                }
                if category.is_empty() {
                    eprintln!("Empty category for ID {:?}!", app_id);
                    continue;
                }
                last_index = item.id;
                // FIXME: sanitize the ID
                // FIXME: this is C code (libdbus) and can be disabled (wtf???)
                let app_id = match dbus::strings::Interface::new(&app_id) {
                    Ok(_) if false => app_id,
                    _ => {
                        let mut h = Sha256::new();
                        h.update(app_id.as_bytes());
                        let result = h.finalize();
                        format!("org.qubes-os.vm.hashed-app-id.{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                        result[0],
                        result[1],
                        result[2],
                        result[3],
                        result[4],
                        result[5],
                        result[6],
                        result[7],
                        result[8],
                        result[9],
                        result[10],
                        result[11],
                        result[12],
                        result[13],
                        result[14],
                        result[15],
                        result[16],
                        result[17],
                        result[18],
                        result[19],
                        result[20],
                        result[21],
                        result[22],
                        result[23],
                        result[24],
                        result[25],
                        result[26],
                        result[27],
                        result[28],
                        result[29],
                        result[30],
                        result[31])
                    }
                };

                eprintln!("Registering new item {}, app id is {:?}", &name, app_id);
                c.request_name(name.clone(), false, true, true)
                    .expect("Cannot acquire bus name {name}?");

                let notifier = NotifierIcon {
                    id: item.id,
                    app_id,
                    category: category.clone(),

                    tooltip: None,
                    title: None,
                    status: None,
                    icon: None,
                    attention_icon: None,
                    overlay_icon: None,
                };
                items.borrow_mut().insert(item.id, notifier);
                {
                    let mut cr = cr_.borrow_mut();
                    let iface_token = server::item::register_status_notifier_item::<
                        NotifierIconWrapper,
                    >(&mut *cr);
                    cr.insert(
                        format!("/{}/StatusNotifierItem", item.id),
                        &[iface_token],
                        NotifierIconWrapper,
                    )
                }
                watcher
                    .register_status_notifier_item(&format!("{}/{}", name, item.id))
                    .unwrap();
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
                        let path = bus_path(item.id);
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
                        eprintln!("Releasing ID {}", item.id);
                        c.release_name(name.clone())
                            .expect("Cannot release bus name?");
                        eprintln!("Released bus name {name}");
                        {
                            let path = bus_path(item.id);
                            cr_.borrow_mut().remove::<()>(&path);
                        }
                        outer_ni.remove(&item.id).expect("Removed nonexistent ID?");
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
