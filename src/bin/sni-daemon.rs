#[path = "sni-daemon/item.rs"]
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
use std::sync::{Arc, Mutex};

use bincode::Options as _;
use sha2::{Digest as _, Sha256};

thread_local! {
    static WRAPPER: Arc<Mutex<HashMap<u64, NotifierIcon>>> = Arc::new(Mutex::new(<HashMap<u64, NotifierIcon>>::new()));
    static ID: std::cell::Cell<u64> = std::cell::Cell::new(0);
}

async fn client_server() -> Result<(), Box<dyn Error>> {
    let items = WRAPPER.with(|w| w.clone());
    let mut last_index = 0u64;
    let (resource, c) = connection::new_session_sync().unwrap();
    tokio::task::spawn_local(async { panic!("D-Bus connection lost: {}", resource.await) });
    let cr_only_sni = Arc::new(Mutex::new(Crossroads::new()));
    {
        let iface_token_1 = server::item::register_status_notifier_item::<NotifierIconWrapper>(
            &mut cr_only_sni.lock().unwrap(),
        );
        let bus_name = names::path_status_notifier_item();
        cr_only_sni
            .lock()
            .unwrap()
            .insert(bus_name.clone(), &[iface_token_1], NotifierIconWrapper);
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
        let options = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .with_native_endian()
            .reject_trailing_bytes();
        let item = options.deserialize(&buffer[..])?;
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
        if let ClientEvent::Create {
            category,
            app_id,
            is_menu,
        } = &item.event
        {
            const PREFIX: &str = "org.qubes_os.vm.app_id.";
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

            eprintln!(
                "Registering new item {}, app id is {:?}, is_menu {}",
                &c.unique_name(),
                app_id,
                is_menu
            );
            let cr_ = cr_only_sni.clone();
            let notifier =
                NotifierIcon::new(item.id, app_id, category.clone(), cr_.clone(), *is_menu);
            let path = notifier.bus_path();

            items.lock().unwrap().insert(item.id, notifier);
            watcher
                .method_call(
                    names::interface_status_notifier_watcher(),
                    names::register_status_notifier_item(),
                    (path.to_string(),),
                )
                .await
                .expect("Could not register status notifier item")
        } else {
            let mut outer_ni = items.lock().unwrap();
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
                    outer_ni.remove(&item.id).expect("Removed nonexistent ID?");
                }
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let local_set = tokio::task::LocalSet::new();

    local_set.spawn_local(client_server());
    local_set.await;
    Ok(())
}
