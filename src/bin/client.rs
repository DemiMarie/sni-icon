#[path = "client/item.rs"]
mod item;

use dbus::blocking::LocalConnection;
use dbus::channel::MatchingReceiver;
use dbus::message::SignalArgs;

use dbus_crossroads::Crossroads;
use item::{NotifierIcon, NotifierIconWrapper};
use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;

use sni_icon::client::watcher::StatusNotifierWatcher;
use sni_icon::{server, ClientEvent, IconType};

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::Receiver;

use sha2::{Digest as _, Sha256};

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
    // eprintln!("First {:?}, middle {:?}, last {:?}", first, middle, rest);
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

fn client_server(r: Receiver<sni_icon::IconClientEvent>) {
    let items = WRAPPER.with(|w| w.clone());
    let mut last_index = 0u64;
    let c = Rc::new(LocalConnection::new_session().unwrap());
    let pid = std::process::id();
    let cr = Rc::new(RefCell::new(Crossroads::new()));
    let cr_ = cr.clone();
    c.start_receive(
        dbus::message::MatchRule::new_method_call(),
        Box::new(move |msg, conn| {
            let path = msg
                .path()
                .expect("Method call with no path should have been rejected by libdbus");
            if let Some(id) = parse_dest(&path, &"/", &"/StatusNotifierItem") {
                ID.with(|id_| id_.set(id))
            }
            cr.borrow_mut().handle_message(msg, conn).unwrap();
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
            if let ClientEvent::Create {
                category,
                app_id,
                has_menu,
            } = &item.event
            {
                let has_menu = *has_menu;
                const PREFIX: &'static str = "org.qubes-os.vm.app-id.";
                let app_id = PREFIX.to_owned() + app_id;
                if item.id <= last_index {
                    panic!("Item ID not monotonically increasing");
                }
                if category.is_empty() {
                    eprintln!("Empty category for ID {:?}!", app_id);
                    continue;
                }
                if has_menu {
                    eprintln!("NYI: displaying menu")
                }
                last_index = item.id;
                // FIXME: sanitize the ID
                // FIXME: this is C code (libdbus) and can be disabled (wtf???)
                let app_id = match dbus::strings::Interface::new(&app_id) {
                    Ok(_) => app_id,
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

                let notifier = NotifierIcon::new(item.id, app_id, category.clone(), has_menu);
                items.borrow_mut().insert(item.id, notifier);
                {
                    let mut cr = cr_.borrow_mut();
                    let iface_token = server::item::register_status_notifier_item::<
                        NotifierIconWrapper,
                    >(&mut *cr);
                    let bus_name = format!("/{}/StatusNotifierItem", item.id);
                    if has_menu {
                        let iface_token_2 =
                            server::menu::register_dbusmenu::<NotifierIconWrapper>(&mut *cr);
                        cr.insert(bus_name, &[iface_token, iface_token_2], NotifierIconWrapper);
                    } else {
                        cr.insert(bus_name, &[iface_token], NotifierIconWrapper);
                    }
                }
                eprintln!("Registering name {:?}", name);
                watcher
                    .register_status_notifier_item(&format!("{}/{}", name, item.id))
                    .unwrap();
            } else {
                let mut outer_ni = items.borrow_mut();
                let ni = outer_ni.get_mut(&item.id).unwrap();
                match item.event {
                    ClientEvent::Create { .. } => unreachable!(),
                    ClientEvent::Title(title) => {
                        ni.set_title(title);
                    }
                    ClientEvent::Status(status) => {
                        ni.set_status(status);
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
                                ni.set_icon(Some(data));
                                c.channel()
                                    .send(
                                        (server::item::StatusNotifierItemNewIcon {})
                                            .to_emit_message(&path),
                                    )
                                    .unwrap();
                            }
                            IconType::Attention => {
                                ni.set_attention_icon(Some(data));
                                c.channel()
                                    .send(
                                        (server::item::StatusNotifierItemNewAttentionIcon {})
                                            .to_emit_message(&path),
                                    )
                                    .unwrap();
                            }
                            IconType::Overlay => {
                                ni.set_overlay_icon(Some(data));
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
                        IconType::Normal => ni.set_icon(None),
                        IconType::Attention => ni.set_attention_icon(None),
                        IconType::Overlay => ni.set_overlay_icon(None),
                    },
                    ClientEvent::Tooltip {
                        icon_data,
                        title,
                        description,
                    } => {
                        ni.set_tooltip(Some(sni_icon::Tooltip {
                            title,
                            description,
                            icon_data: icon_data,
                        }));
                    }
                    ClientEvent::RemoveTooltip => {
                        ni.set_tooltip(None);
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
            sni_icon::IconClientEvent {
                id: _,
                event: ClientEvent::Icon { .. },
            } => {}
            _ => eprintln!("->client {:?}", item),
        }
        client_sender.send(item).unwrap();
    }
}
