#[path = "client/item.rs"]
mod item;

use dbus::nonblock::Proxy;

use dbus_crossroads::Crossroads;
use dbus_tokio::connection;
use item::{NotifierIcon, NotifierIconWrapper};
use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;
use tokio::io::AsyncReadExt;

use sni_icon::{names, server, ClientEvent, IconType};

use std::cell::RefCell;
use std::rc::Rc;

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

thread_local! {
    static WRAPPER: Rc<RefCell<HashMap<u64, NotifierIcon>>> = Rc::new(RefCell::new(<HashMap<u64, NotifierIcon>>::new()));
    static ID: std::cell::Cell<u64> = std::cell::Cell::new(0);
}

async fn client_server() -> Result<(), Box<dyn Error>> {
    let items = WRAPPER.with(|w| w.clone());
    let mut last_index = 0u64;
    let (resource, c) = connection::new_session_local().unwrap();
    tokio::task::spawn_local(async { panic!("D-Bus connection lost: {}", resource.await) });
    let pid = std::process::id();
    let cr_only_sni = Rc::new(RefCell::new(Crossroads::new()));
    let cr_sni_menu = Rc::new(RefCell::new(Crossroads::new()));
    {
        let iface_token_1 = server::item::register_status_notifier_item::<NotifierIconWrapper>(
            &mut *cr_only_sni.borrow_mut(),
        );
        let iface_token_2 = server::item::register_status_notifier_item::<NotifierIconWrapper>(
            &mut *cr_sni_menu.borrow_mut(),
        );
        let iface_token_3 =
            server::menu::register_dbusmenu::<NotifierIconWrapper>(&mut *cr_sni_menu.borrow_mut());
        let bus_name = names::path_status_notifier_item();
        cr_only_sni
            .borrow_mut()
            .insert(bus_name.clone(), &[iface_token_1], NotifierIconWrapper);
        cr_sni_menu.borrow_mut().insert(
            bus_name,
            &[iface_token_2, iface_token_3],
            NotifierIconWrapper,
        );
    }

    let watcher = Proxy::new(
        names::name_status_notifier_watcher(),
        names::path_status_notifier_watcher(),
        Duration::from_millis(1000),
        c.clone(),
    );

    dbus::strings::Interface::new("bogus").expect_err("no-string-validation must be off!");
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
        let (item, size) =
            bincode::decode_from_slice(&mut buffer[..], bincode::config::standard())?;
        if size != buffer.len() {
            panic!(
                "Malformed message on stdin: got {} bytes but expected {}",
                buffer.len(),
                size
            );
        }
        drop(buffer);
        match &item {
            sni_icon::IconClientEvent {
                id,
                event: ClientEvent::Icon { .. },
            } => {
                eprintln!("->client Create {}", id);
            }
            _ => {
                eprintln!("->client {:?}", item);
            }
        };
        let name = format!("org.freedesktop.StatusNotifierItem-{}-{}", pid, item.id);
        if let ClientEvent::Create {
            category,
            app_id,
            has_menu,
        } = &item.event
        {
            let has_menu = *has_menu;
            const PREFIX: &'static str = "org.qubes_os.vm.app_id.";
            let app_id = PREFIX.to_owned() + &app_id;
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
                    eprintln!("Name {:?} is invalid", app_id);
                    let mut h = Sha256::new();
                    h.update(app_id.as_bytes());
                    let result = h.finalize();
                    format!("org.qubes_os.vm.hashed_app_id.{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
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

            let cr_ = if has_menu {
                cr_sni_menu.clone()
            } else {
                cr_only_sni.clone()
            };
            let notifier = NotifierIcon::new(
                item.id,
                app_id,
                category.clone(),
                if has_menu {
                    Some(Default::default())
                } else {
                    None
                },
                cr_.clone(),
            );
            let path = notifier.bus_path();
            items.borrow_mut().insert(item.id, notifier);
            eprintln!("Registering name {:?}", name);
            watcher
                .method_call(
                    names::interface_status_notifier_watcher(),
                    names::register_status_notifier_item(),
                    (format!("{}", path),),
                )
                .await
                .expect("Could not register status notifier item")
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
                    match typ {
                        IconType::Normal => {
                            ni.set_icon(Some(data));
                        }
                        IconType::Attention => {
                            ni.set_attention_icon(Some(data));
                        }
                        IconType::Overlay => {
                            ni.set_overlay_icon(Some(data));
                        }
                        IconType::Title | IconType::Status => panic!("guest sent bad icon type"),
                    }
                }
                ClientEvent::RemoveIcon(typ) => match typ {
                    IconType::Normal => ni.set_icon(None),
                    IconType::Attention => ni.set_attention_icon(None),
                    IconType::Overlay => ni.set_overlay_icon(None),
                    IconType::Title | IconType::Status => panic!("guest sent bad icon type"),
                },
                ClientEvent::Tooltip {
                    icon_data,
                    title,
                    description,
                } => {
                    ni.set_tooltip(Some(sni_icon::Tooltip {
                        title,
                        description,
                        icon_data,
                    }));
                }
                ClientEvent::RemoveTooltip => {
                    ni.set_tooltip(None);
                }

                ClientEvent::Destroy => {
                    eprintln!("Releasing ID {}", item.id);
                    c.release_name(name.clone())
                        .await
                        .expect("Cannot release bus name?");
                    eprintln!("Released bus name {name}");
                    outer_ni.remove(&item.id).expect("Removed nonexistent ID?");
                }
                ClientEvent::EnableMenu { revision, entries } => {
                    eprintln!("D-Bus menu enabled! Revision {revision}, entries {entries:?}");
                }
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let local_set = tokio::task::LocalSet::new();

    local_set.spawn_local(client_server());
    Ok(local_set.await)
}
