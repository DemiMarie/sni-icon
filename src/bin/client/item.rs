use dbus::channel::{MatchingReceiver as _, Sender as _};
use dbus::message::SignalArgs as _;
use dbus::nonblock::SyncConnection as Connection;
use dbus::strings::{ErrorName, Path};
use dbus_crossroads::Crossroads;
use futures_util::future::{AbortHandle, Abortable};
use sni_icon::{server, IconServerEvent};
use std::io::Write as _;
use std::sync::{Arc, Mutex};

use sni_icon::{names::path_status_notifier_item as path, IconData, ServerEvent};

fn send_or_panic<T: bincode::Encode>(s: T) {
    let mut out = std::io::stdout().lock();
    let v = bincode::encode_to_vec(s, bincode::config::standard()).expect("Cannot encode data");
    eprintln!("Sending {} bytes", v.len());
    out.write_all(&((v.len() as u32).to_le_bytes())[..])
        .expect("cannot write to stdout");
    out.write_all(&v[..]).expect("cannot write to stdout");
    out.flush().expect("Cannot flush stdout");
}

pub(super) struct NotifierIcon {
    id: u64,
    connection: Arc<Connection>,
    category: String,
    app_id: String,

    tooltip: Option<sni_icon::Tooltip>,
    title: Option<String>,
    status: Option<String>,

    icon: Option<Vec<IconData>>,
    attention_icon: Option<Vec<IconData>>,
    overlay_icon: Option<Vec<IconData>>,
    is_menu: bool,

    abort_handle: AbortHandle,
}

impl Drop for NotifierIcon {
    fn drop(&mut self) {
        self.abort_handle.abort()
    }
}

impl NotifierIcon {
    pub fn new(
        id: u64,
        app_id: String,
        category: String,
        cr: Arc<Mutex<Crossroads>>,
        is_menu: bool,
    ) -> Self {
        eprintln!("Creating new notifier icon");
        let (resource, connection) =
            dbus_tokio::connection::new_session_sync().expect("Cannot connect to session bus");
        let (abort_handle, abort_registration) = AbortHandle::new_pair();
        tokio::task::spawn_local(Abortable::new(resource, abort_registration));
        connection.start_receive(
            dbus::message::MatchRule::new_method_call(),
            Box::new(move |msg, conn| {
                super::ID.with(|id_| id_.set(id));
                cr.lock().unwrap().handle_message(msg, conn).unwrap();
                true
            }),
        );
        Self {
            id,
            app_id,
            category,

            connection,
            tooltip: None,
            title: None,
            status: None,
            icon: None,
            attention_icon: None,
            overlay_icon: None,
            is_menu,
            abort_handle,
        }
    }
    pub fn set_title(&mut self, title: Option<String>) {
        self.title = title;
        self.connection
            .send((server::item::StatusNotifierItemNewTitle {}).to_emit_message(&path()))
            .unwrap();
    }
    pub fn bus_path(&self) -> String {
        self.connection.unique_name().to_string()
    }
    pub fn set_tooltip(&mut self, tooltip: Option<sni_icon::Tooltip>) {
        self.tooltip = tooltip;
        self.connection
            .send((server::item::StatusNotifierItemNewToolTip {}).to_emit_message(&path()))
            .unwrap();
    }
    pub fn set_status(&mut self, status: Option<String>) {
        self.status = status.clone();
        self.connection
            .send(
                (server::item::StatusNotifierItemNewStatus {
                    status: status.unwrap_or_else(|| "normal".to_owned()),
                })
                .to_emit_message(&path()),
            )
            .unwrap();
    }
    pub fn set_icon(&mut self, icon: Option<Vec<IconData>>) {
        self.icon = icon;
        self.connection
            .send((server::item::StatusNotifierItemNewIcon {}).to_emit_message(&path()))
            .unwrap();
    }
    pub fn set_attention_icon(&mut self, attention_icon: Option<Vec<IconData>>) {
        self.attention_icon = attention_icon;
        self.connection
            .send((server::item::StatusNotifierItemNewAttentionIcon {}).to_emit_message(&path()))
            .unwrap();
    }
    pub fn set_overlay_icon(&mut self, overlay_icon: Option<Vec<IconData>>) {
        self.overlay_icon = overlay_icon;
        self.connection
            .send((server::item::StatusNotifierItemNewOverlayIcon {}).to_emit_message(&path()))
            .unwrap();
    }
}

pub(super) struct NotifierIconWrapper;

fn call_with_icon<T, U: FnOnce(&mut NotifierIcon) -> Result<T, dbus::MethodErr>>(
    cb: U,
) -> Result<T, dbus::MethodErr> {
    crate::WRAPPER.with(|items| {
        let mut items = items.lock().unwrap();
        match crate::ID.with(|id| items.get_mut(&id.get())) {
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
        eprintln!("Got context menu event: {x}x{y}");
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
    fn menu(&self) -> Result<Path<'static>, dbus::MethodErr> {
        eprintln!("menu() called!");
        call_with_icon(|_| Err(dbus::MethodErr::no_property("menu")))
    }
    fn item_is_menu(&self) -> Result<bool, dbus::MethodErr> {
        call_with_icon(|icon| Ok(icon.is_menu))
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
