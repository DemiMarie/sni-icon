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

use sni_icon::client::item::StatusNotifierItem;
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

struct NotifierIconWrapper(Rc<RefCell<NotifierIcon>>);
// This is a total lie, but we get away with it because the code is essentially single-threaded.
// It must be fixed before shipping the code in production.
unsafe impl Send for NotifierIconWrapper {}

fn send_or_panic<T: bincode::Encode>(s: T) {
    let mut out = std::io::stdout().lock();
    bincode::encode_into_std_write(s, &mut out, bincode::config::standard())
        .expect("Cannot write to stdout");
    out.flush().expect("Cannot flush stdout");
}

impl server::item::StatusNotifierItem for NotifierIconWrapper {
    fn context_menu(&mut self, _x_: i32, _y_: i32) -> Result<(), dbus::MethodErr> {
        send_or_panic(IconServerEvent {
            id: self.0.borrow().id,
            event: ServerEvent::ContextMenu,
        });
        Ok(())
    }
    fn activate(&mut self, _x_: i32, _y_: i32) -> Result<(), dbus::MethodErr> {
        send_or_panic(IconServerEvent {
            id: self.0.borrow().id,
            event: ServerEvent::Activate,
        });
        Ok(())
    }
    fn secondary_activate(&mut self, _x_: i32, _y_: i32) -> Result<(), dbus::MethodErr> {
        send_or_panic(IconServerEvent {
            id: self.0.borrow().id,
            event: ServerEvent::SecondaryActivate,
        });
        Ok(())
    }
    fn scroll(&mut self, delta: i32, orientation: String) -> Result<(), dbus::MethodErr> {
        send_or_panic(IconServerEvent {
            id: self.0.borrow().id,
            event: ServerEvent::Scroll { delta, orientation },
        });
        Ok(())
    }
    fn category(&self) -> Result<String, dbus::MethodErr> {
        Ok(self.0.borrow().category.clone())
    }
    fn id(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property("Id"))
    }
    fn title(&self) -> Result<String, dbus::MethodErr> {
        self.0
            .borrow()
            .title
            .clone()
            .ok_or_else(|| dbus::MethodErr::no_property("Title"))
    }
    fn status(&self) -> Result<String, dbus::MethodErr> {
        self.0
            .borrow()
            .status
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
        let icon = self.0.borrow();
        let icon = icon
            .icon
            .as_ref()
            .map(|f| f.as_slice())
            .unwrap_or_else(|| &[]);
        Ok(icon
            .iter()
            .map(|f| (f.width as i32, f.height as i32, f.data.clone()))
            .collect())
    }
    fn overlay_icon_name(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property("OverlayIconName"))
    }
    fn overlay_icon_pixmap(&self) -> Result<Vec<(i32, i32, Vec<u8>)>, dbus::MethodErr> {
        let overlay_icon = self.0.borrow();
        let overlay_icon = overlay_icon
            .overlay_icon
            .as_ref()
            .map(|f| f.as_slice())
            .unwrap_or_else(|| &[]);
        Ok(overlay_icon
            .iter()
            .map(|f| (f.width as i32, f.height as i32, f.data.clone()))
            .collect())
    }
    fn attention_icon_name(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property("AttentionIconName"))
    }
    fn attention_icon_pixmap(&self) -> Result<Vec<(i32, i32, Vec<u8>)>, dbus::MethodErr> {
        let attention_icon = self.0.borrow();
        let attention_icon = attention_icon
            .attention_icon
            .as_ref()
            .map(|f| f.as_slice())
            .unwrap_or_else(|| &[]);

        Ok(attention_icon
            .iter()
            .map(|f| (f.width as i32, f.height as i32, f.data.clone()))
            .collect())
    }
    fn attention_movie_name(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property("AttentionMovieName"))
    }

    fn tool_tip(
        &self,
    ) -> Result<(String, Vec<(i32, i32, Vec<u8>)>, String, String), dbus::MethodErr> {
        let tooltip = self.0.borrow();
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
    }
}

fn parse_dest(d: &dbus::strings::BusName, s: &str) -> Option<u64> {
    if d.len() < s.len() + 1 {
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

fn client_server(r: Receiver<IconClientEvent>) {
    let mut items = Rc::new(RefCell::new(<HashMap<
        u64,
        (NotifierIconWrapper, Crossroads),
    >>::new()));
    let mut last_index = 0u64;
    let c = Rc::new(LocalConnection::new_session().unwrap());
    let pid = std::process::id();
    let bus_prefix = format!("org.freedesktop.StatusNotifierItem-{}-", pid);
    let items_ = items.clone();
    c.start_receive(
        dbus::message::MatchRule::new_method_call(),
        Box::new(move |msg, conn| {
            let destination = msg
                .destination()
                .expect("Method call with no destination should have been rejected by bus daemon!");
            let path = msg
                .path()
                .expect("Method call with no path should have been rejected by bus daemon!");
            if destination.starts_with(":") {
                if !msg.get_no_reply() {
                    use dbus::channel::Sender as _;
                    let err = unsafe {
                        // SAFETY: the preconditions are held
                        ErrorName::from_slice_unchecked("org.freedesktop.DBus.Error.NotSupported\0")
                    };
                    let human_readable = unsafe {
                        // SAFETY: the preconditions are held
                        CStr::from_bytes_with_nul_unchecked(
                            &b"Messages sent to a unique ID not supported\0"[..],
                        )
                    };
                    conn.send(msg.error(&err, human_readable));
                }
            } else {
                let id = parse_dest(&destination, &bus_prefix)
                    .expect("bus daemon sent a message to name we never owned");
                if let Some(cr) = items_.borrow_mut().get_mut(&id) {
                    cr.1.handle_message(msg, conn).unwrap();
                }
            }
            true
        }),
    );

    let watcher = c.with_proxy(
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        Duration::from_millis(1000),
    );

    let path = dbus::Path::new("/StatusNotifierItem").expect("validated above");
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

                let mut cr = Crossroads::new();
                let notifier = Rc::new(RefCell::new(NotifierIcon {
                    id: item.id.clone(),
                    category: category.clone(),

                    tooltip: None,
                    title: None,
                    status: None,
                    icon: None,
                    attention_icon: None,
                    overlay_icon: None,
                }));
                let iface_token =
                    server::item::register_status_notifier_item::<NotifierIconWrapper>(&mut cr);
                cr.insert(
                    "/StatusNotifierItem",
                    &[iface_token],
                    NotifierIconWrapper(notifier.clone()),
                );
                items
                    .borrow_mut()
                    .insert(item.id, (NotifierIconWrapper(notifier), cr));
                watcher
                    .register_status_notifier_item(&format!("{}", name))
                    .unwrap();
            } else {
                let _watcher = c.with_proxy(
                    "org.kde.StatusNotifierWatcher",
                    "/StatusNotifierWatcher",
                    Duration::from_millis(1000),
                );
                let mut outer_ni = items.borrow_mut();
                let ni = &outer_ni.get(&item.id).unwrap().0 .0;
                match item.event {
                    ClientEvent::Create { .. } => unreachable!(),
                    ClientEvent::Title(title) => {
                        ni.borrow_mut().title = title;
                    }
                    ClientEvent::Status(status) => {
                        ni.borrow_mut().status = status;
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
                                ni.borrow_mut().icon = Some(data);
                                c.channel()
                                    .send(
                                        (server::item::StatusNotifierItemNewIcon {})
                                            .to_emit_message(&path),
                                    )
                                    .unwrap();
                            }
                            IconType::Attention => {
                                ni.borrow_mut().attention_icon = Some(data);
                                c.channel()
                                    .send(
                                        (server::item::StatusNotifierItemNewAttentionIcon {})
                                            .to_emit_message(&path),
                                    )
                                    .unwrap();
                            }
                            IconType::Overlay => {
                                ni.borrow_mut().overlay_icon = Some(data);
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
                        IconType::Normal => ni.borrow_mut().icon = None,
                        IconType::Attention => ni.borrow_mut().attention_icon = None,
                        IconType::Overlay => ni.borrow_mut().overlay_icon = None,
                    },
                    ClientEvent::Tooltip {
                        icon_data,
                        title,
                        description,
                    } => {
                        ni.borrow_mut().tooltip = Some(Tooltip {
                            title,
                            description,
                            icon_data: icon_data,
                        });
                    }
                    ClientEvent::RemoveTooltip => {
                        ni.borrow_mut().tooltip = None;
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
