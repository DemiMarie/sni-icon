use dbus::blocking::SyncConnection;
use dbus::channel::MatchingReceiver;
use dbus::message::SignalArgs;
use dbus_crossroads::Crossroads;
use std::collections::HashMap;
use std::error::Error;
use std::io::Write;
use std::time::Duration;

use sni_icon::client::item::StatusNotifierItem;
use sni_icon::client::watcher::StatusNotifierWatcher;

use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::sync::Mutex;

use sni_icon::*;

struct NotifierIcon {
    pub id: u64,
    pub category: String,

    pub tooltip: Mutex<Option<Tooltip>>,
    pub title: Mutex<Option<String>>,
    pub status: Mutex<Option<String>>,

    pub icon: Mutex<Option<Vec<IconData>>>,
    pub attention_icon: Mutex<Option<Vec<IconData>>>,
    pub overlay_icon: Mutex<Option<Vec<IconData>>>,
}

struct NotifierIconWrapper(Arc<NotifierIcon>);

fn send_or_panic<T: bincode::Encode>(s: T) {
    let mut out = std::io::stdout().lock();
    bincode::encode_into_std_write(s, &mut out, bincode::config::standard())
        .expect("Cannot write to stdout");
    out.flush().expect("Cannot flush stdout");
}

impl server::item::StatusNotifierItem for NotifierIconWrapper {
    fn context_menu(&mut self, _x_: i32, _y_: i32) -> Result<(), dbus::MethodErr> {
        send_or_panic(IconServerEvent {
            id: self.0.id,
            event: ServerEvent::ContextMenu,
        });
        Ok(())
    }
    fn activate(&mut self, _x_: i32, _y_: i32) -> Result<(), dbus::MethodErr> {
        send_or_panic(IconServerEvent {
            id: self.0.id,
            event: ServerEvent::Activate,
        });
        Ok(())
    }
    fn secondary_activate(&mut self, _x_: i32, _y_: i32) -> Result<(), dbus::MethodErr> {
        send_or_panic(IconServerEvent {
            id: self.0.id,
            event: ServerEvent::SecondaryActivate,
        });
        Ok(())
    }
    fn scroll(&mut self, delta: i32, orientation: String) -> Result<(), dbus::MethodErr> {
        send_or_panic(IconServerEvent {
            id: self.0.id,
            event: ServerEvent::Scroll { delta, orientation },
        });
        Ok(())
    }
    fn category(&self) -> Result<String, dbus::MethodErr> {
        Ok(self.0.category.clone())
    }
    fn id(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property("Id"))
    }
    fn title(&self) -> Result<String, dbus::MethodErr> {
        let title = self.0.title.lock().unwrap();
        title
            .clone()
            .ok_or_else(|| dbus::MethodErr::no_property("Title"))
    }
    fn status(&self) -> Result<String, dbus::MethodErr> {
        let status = self.0.status.lock().unwrap();
        status
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
        let icon = self.0.icon.lock().unwrap();
        let icon = icon.as_ref().map(|f| f.as_slice()).unwrap_or_else(|| &[]);
        Ok(icon
            .iter()
            .map(|f| (f.width as i32, f.height as i32, f.data.clone()))
            .collect())
    }
    fn overlay_icon_name(&self) -> Result<String, dbus::MethodErr> {
        Err(dbus::MethodErr::no_property("OverlayIconName"))
    }
    fn overlay_icon_pixmap(&self) -> Result<Vec<(i32, i32, Vec<u8>)>, dbus::MethodErr> {
        let overlay_icon = self.0.overlay_icon.lock().unwrap();
        let overlay_icon = overlay_icon
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
        let attention_icon = self.0.attention_icon.lock().unwrap();
        let attention_icon = attention_icon
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
        let tooltip = self.0.tooltip.lock().unwrap();
        let tooltip = tooltip
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

fn client_server(r: Receiver<IconClientEvent>) {
    let mut items: HashMap<u64, Arc<NotifierIcon>> = HashMap::new();
    let mut last_index = 0u64;
    let c = Arc::new(SyncConnection::new_session().unwrap());
    let cr = Arc::new(Mutex::new(Crossroads::new()));
    let iface_token =
        server::item::register_status_notifier_item::<NotifierIconWrapper>(&mut cr.lock().unwrap());
    let cr_ = cr.clone();
    c.start_receive(
        dbus::message::MatchRule::new_method_call(),
        Box::new(move |msg, conn| {
            cr_.lock().unwrap().handle_message(msg, conn).unwrap();
            true
        }),
    );

    let watcher = c.with_proxy(
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        Duration::from_millis(1000),
    );

    loop {
        c.process(Duration::from_millis(100)).unwrap();
        while let Some(item) = r.recv_timeout(Duration::from_millis(100)).ok() {
            let name = format!("/QubesIcon/{}/StatusNotifierItem", item.id);
            if let ClientEvent::Create { category } = &item.event {
                if item.id <= last_index {
                    panic!("Item ID not monotonically increasing");
                }
                last_index = item.id;
                eprintln!("Registering new item {}", &name);

                let notifier = Arc::new(NotifierIcon {
                    id: item.id.clone(),
                    category: category.clone(),

                    tooltip: Mutex::new(None),
                    title: Mutex::new(None),
                    status: Mutex::new(None),
                    icon: Mutex::new(None),
                    attention_icon: Mutex::new(None),
                    overlay_icon: Mutex::new(None),
                });

                items.insert(item.id, notifier.clone());
                cr.lock().unwrap().insert(
                    name.clone(),
                    &[iface_token],
                    NotifierIconWrapper(notifier),
                );
                watcher.register_status_notifier_item(&name).unwrap();
            } else {
                let path = dbus::Path::new(name).expect("validated above");
                let _watcher = c.with_proxy(
                    "org.kde.StatusNotifierWatcher",
                    "/StatusNotifierWatcher",
                    Duration::from_millis(1000),
                );

                let ni = items.get(&item.id).unwrap();
                match item.event {
                    ClientEvent::Create { .. } => unreachable!(),
                    ClientEvent::Title(title) => {
                        *ni.title.lock().unwrap() = title;
                    }
                    ClientEvent::Status(status) => {
                        *ni.status.lock().unwrap() = status;
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
                                *ni.icon.lock().unwrap() = Some(data);
                                c.channel()
                                    .send(
                                        (server::item::StatusNotifierItemNewIcon {})
                                            .to_emit_message(&path),
                                    )
                                    .unwrap();
                            }
                            IconType::Attention => {
                                *ni.attention_icon.lock().unwrap() = Some(data);
                                c.channel()
                                    .send(
                                        (server::item::StatusNotifierItemNewAttentionIcon {})
                                            .to_emit_message(&path),
                                    )
                                    .unwrap();
                            }
                            IconType::Overlay => {
                                *ni.overlay_icon.lock().unwrap() = Some(data);
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
                        IconType::Normal => *ni.icon.lock().unwrap() = None,
                        IconType::Attention => *ni.attention_icon.lock().unwrap() = None,
                        IconType::Overlay => *ni.overlay_icon.lock().unwrap() = None,
                    },
                    ClientEvent::Tooltip {
                        icon_data,
                        title,
                        description,
                    } => {
                        *ni.tooltip.lock().unwrap() = Some(Tooltip {
                            title,
                            description,
                            icon_data: icon_data,
                        });
                    }
                    ClientEvent::RemoveTooltip => {
                        *ni.tooltip.lock().unwrap() = None;
                    }

                    ClientEvent::Destroy => {
                        cr.lock().unwrap().remove::<Arc<NotifierIcon>>(&path);
                        items.remove(&item.id);
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
        eprintln!("->client {:?}", item);
        client_sender.send(item).unwrap();
    }
}
