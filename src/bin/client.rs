use dbus::blocking::{Connection, SyncConnection};
use dbus::channel::MatchingReceiver;
use dbus::message::{MatchRule, SignalArgs};
use dbus_crossroads::Crossroads;
use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;

use sni_icon::client::item::StatusNotifierItem;
use sni_icon::client::watcher::StatusNotifierWatcher;

use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::sync::Mutex;

use sni_icon::*;

struct NotifierIcon {
    pub id: String,
    pub category: String,

    pub tooltip: Mutex<Option<Tooltip>>,
    pub title: Mutex<Option<String>>,
    pub status: Mutex<Option<String>>,

    pub icon: Mutex<Option<Vec<IconData>>>,
    pub attention_icon: Mutex<Option<Vec<IconData>>>,
    pub overlay_icon: Mutex<Option<Vec<IconData>>>,
}

struct NotifierIconWrapper(Arc<NotifierIcon>);

impl server::item::StatusNotifierItem for NotifierIconWrapper {
    fn context_menu(&mut self, x_: i32, y_: i32) -> Result<(), dbus::MethodErr> {
        bincode::encode_into_std_write(IconServerEvent {
            id: self.0.id.clone(),
            event: ServerEvent::ContextMenu,
        }, &mut std::io::stdout().lock(), bincode::config::standard()).unwrap();
        Ok(())
    }
    fn activate(&mut self, x_: i32, y_: i32) -> Result<(), dbus::MethodErr> {
        bincode::encode_into_std_write(IconServerEvent {
            id: self.0.id.clone(),
            event: ServerEvent::Activate,
        }, &mut std::io::stdout().lock(), bincode::config::standard()).unwrap();
        Ok(())
    }
    fn secondary_activate(&mut self, x_: i32, y_: i32) -> Result<(), dbus::MethodErr> {
        bincode::encode_into_std_write(IconServerEvent {
            id: self.0.id.clone(),
            event: ServerEvent::SecondaryActivate,
        }, &mut std::io::stdout().lock(), bincode::config::standard()).unwrap();
        Ok(())
    }
    fn scroll(&mut self, delta: i32, orientation: String) -> Result<(), dbus::MethodErr> {
        bincode::encode_into_std_write(IconServerEvent {
            id: self.0.id.clone(),
            event: ServerEvent::Scroll { delta, orientation },
        }, &mut std::io::stdout().lock(), bincode::config::standard()).unwrap();
        Ok(())
    }
    fn category(&self) -> Result<String, dbus::MethodErr> {
        Ok(self.0.category.clone())
    }
    fn id(&self) -> Result<String, dbus::MethodErr> {
        Ok(self.0.id.clone())
    }
    fn title(&self) -> Result<String, dbus::MethodErr> {
        let title = self.0.title.lock().unwrap();
        title
            .clone()
            .ok_or_else(|| dbus::MethodErr::no_property("title"))
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
        let icon = icon
            .as_ref()
            .map(|f| f.as_slice())
            .unwrap_or_else(|| &[]);
        Ok(icon.iter().map(|f| (f.width as i32, f.height as i32, f.data.clone())).collect())
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
        Ok(overlay_icon.iter().map(|f| (f.width as i32, f.height as i32, f.data.clone())).collect())
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

        Ok(attention_icon.iter().map(|f| (f.width as i32, f.height as i32, f.data.clone())).collect())
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
            .icon_data.iter().map(|f| (f.width as i32, f.height as i32, f.data.clone())).collect();

        Ok((
            String::new(),
            icon_data,
            tooltip.title.clone(),
            tooltip.description.clone(),
        ))
    }
}

fn client_server(r: Receiver<IconClientEvent>) {
    let mut items: HashMap<String, Arc<NotifierIcon>> = HashMap::new();
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
            if let ClientEvent::Create { category } = &item.event {
                if items.contains_key(&item.id) {
                    panic!("Item ID exists already");
                }

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

                items.insert(item.id.clone(), notifier.clone());

                cr.lock().unwrap().insert(
                    format!("/{}/StatusNotifierItem", item.id),
                    &[iface_token],
                    NotifierIconWrapper(notifier),
                );
                watcher
                    .register_status_notifier_item(&format!("{}/{}", c.unique_name(), item.id))
                    .unwrap();
            } else {
                let watcher = c.with_proxy(
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
                    ClientEvent::Icon {
                        typ,
                        data,
                    } => match typ {
                        IconType::Normal => {
                            *ni.icon.lock().unwrap() = Some(data);
                            c.channel().send((server::item::StatusNotifierItemNewIcon {}).to_emit_message(&dbus::Path::new(format!("/{}/StatusNotifierItem", item.id)).unwrap())).unwrap();
                        }
                        IconType::Attention => {
                            *ni.attention_icon.lock().unwrap() = Some(data);
                            c.channel().send((server::item::StatusNotifierItemNewAttentionIcon {}).to_emit_message(&dbus::Path::new(format!("/{}/StatusNotifierItem", item.id)).unwrap())).unwrap();
                        }
                        IconType::Overlay => {
                            *ni.overlay_icon.lock().unwrap() = Some(data);
                            c.channel().send((server::item::StatusNotifierItemNewOverlayIcon {}).to_emit_message(&dbus::Path::new(format!("/{}/StatusNotifierItem", item.id)).unwrap())).unwrap();
                        }
                    },
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
                        cr.lock().unwrap().remove::<Arc<NotifierIcon>>(
                            &dbus::Path::new(format!("/{}/StatusNotifierItem", item.id)).unwrap(),
                        );
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
        client_sender.send(item).unwrap();
    }
}
