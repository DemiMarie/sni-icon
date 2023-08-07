use core::cell::RefCell;
use dbus::arg::RefArg;
use dbus::channel::Sender as _;
use dbus::message::SignalArgs;
use dbus::nonblock::LocalConnection as Connection;
use dbus::strings::{ErrorName, Path};
use dbus::MethodErr;
use sni_icon::{server, IconServerEvent};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::io::Write as _;
use std::rc::{Rc, Weak};

use sni_icon::{IconData, ServerEvent};

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
    path: Path<'static>,
    category: String,
    app_id: String,

    tooltip: Option<sni_icon::Tooltip>,
    title: Option<String>,
    status: Option<String>,

    icon: Option<Vec<IconData>>,
    attention_icon: Option<Vec<IconData>>,
    overlay_icon: Option<Vec<IconData>>,

    has_menu: Option<Vec<sni_icon::DBusMenuEntry>>,
}

struct MenuEntry {
    data: Vec<Rc<MenuEntry>>,
    ids: Vec<(i32, Weak<MenuEntry>)>,
    revision: u32,
}
struct Menu {
    entries: HashMap<i32, Rc<RefCell<MenuEntry>>>,
}

pub(super) fn bus_path(id: u64) -> dbus::Path<'static> {
    format!("/{}/StatusNotifierItem\0", id).into()
}

impl NotifierIcon {
    pub fn new(
        id: u64,
        app_id: String,
        category: String,
        has_menu: Option<Vec<sni_icon::DBusMenuEntry>>,
    ) -> Self {
        Self {
            id,
            app_id,
            category,

            tooltip: None,
            title: None,
            status: None,
            icon: None,
            attention_icon: None,
            overlay_icon: None,
            has_menu,
            path: bus_path(id),
        }
    }
    pub fn set_title(&mut self, title: Option<String>, connection: &Connection) {
        self.title = title;
        connection
            .send((server::item::StatusNotifierItemNewTitle {}).to_emit_message(&self.path))
            .unwrap();
    }
    pub fn set_tooltip(&mut self, tooltip: Option<sni_icon::Tooltip>, connection: &Connection) {
        self.tooltip = tooltip;
        connection
            .send((server::item::StatusNotifierItemNewToolTip {}).to_emit_message(&self.path))
            .unwrap();
    }
    pub fn set_status(&mut self, status: Option<String>, connection: &Connection) {
        self.status = status.clone();
        connection
            .send(
                (server::item::StatusNotifierItemNewStatus {
                    status: status.unwrap_or_else(|| "normal".to_owned()),
                })
                .to_emit_message(&self.path),
            )
            .unwrap();
    }
    pub fn set_icon(&mut self, icon: Option<Vec<IconData>>, connection: &Connection) {
        self.icon = icon;
        connection
            .send((server::item::StatusNotifierItemNewIcon {}).to_emit_message(&self.path))
            .unwrap();
    }
    pub fn set_attention_icon(
        &mut self,
        attention_icon: Option<Vec<IconData>>,
        connection: &Connection,
    ) {
        self.attention_icon = attention_icon;
        connection
            .send((server::item::StatusNotifierItemNewAttentionIcon {}).to_emit_message(&self.path))
            .unwrap();
    }
    pub fn set_overlay_icon(
        &mut self,
        overlay_icon: Option<Vec<IconData>>,
        connection: &Connection,
    ) {
        self.overlay_icon = overlay_icon;
        connection
            .send((server::item::StatusNotifierItemNewOverlayIcon {}).to_emit_message(&self.path))
            .unwrap();
    }

    fn about_to_show(&self, id: i32) -> Result<bool, dbus::MethodErr> {
        Err(dbus::MethodErr::failed("not yet implemented"))
    }

    fn contains_id(&self, _: i32) -> bool {
        return false;
    }

    fn get_property(
        &self,
        _id: i32,
        _name: String,
    ) -> Result<dbus::arg::Variant<Box<(dyn RefArg + 'static)>>, dbus::MethodErr> {
        Err(dbus::MethodErr::failed("not yet implemented"))
    }
    fn event(
        &mut self,
        _: i32,
        _: std::string::String,
        _: dbus::arg::Variant<Box<(dyn RefArg + 'static)>>,
        _: u32,
    ) -> Result<(), MethodErr> {
        Err(dbus::MethodErr::failed("not yet implemented"))
    }

    fn get_layout(
        &mut self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: Vec<std::string::String>,
    ) -> Result<
        (
            u32,
            (
                i32,
                HashMap<std::string::String, dbus::arg::Variant<Box<(dyn RefArg + 'static)>>>,
                Vec<dbus::arg::Variant<Box<(dyn RefArg + 'static)>>>,
            ),
        ),
        MethodErr,
    > {
        Err(dbus::MethodErr::failed("not yet implemented"))
    }
}

pub(super) struct NotifierIconWrapper;

fn call_with_icon<T, U: FnOnce(&mut NotifierIcon) -> Result<T, dbus::MethodErr>>(
    cb: U,
) -> Result<T, dbus::MethodErr> {
    crate::WRAPPER.with(|items| {
        let mut items = items.borrow_mut();
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
        eprintln!("menu() called!");
        call_with_icon(|icon| {
            if icon.has_menu.is_some() {
                Ok(icon.path.clone())
            } else {
                Err(dbus::MethodErr::no_property("menu"))
            }
        })
    }
    fn item_is_menu(&self) -> Result<bool, dbus::MethodErr> {
        call_with_icon(|icon| Ok(icon.has_menu.is_some()))
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

impl server::menu::Dbusmenu for NotifierIconWrapper {
    fn get_layout(
        &mut self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: Vec<std::string::String>,
    ) -> Result<
        (
            u32,
            (
                i32,
                HashMap<std::string::String, dbus::arg::Variant<Box<(dyn RefArg + 'static)>>>,
                Vec<dbus::arg::Variant<Box<(dyn RefArg + 'static)>>>,
            ),
        ),
        MethodErr,
    > {
        call_with_icon(|icon| icon.get_layout(parent_id, recursion_depth, property_names))
    }
    fn get_group_properties(
        &mut self,
        ids: Vec<i32>,
        property_names: Vec<std::string::String>,
    ) -> Result<
        Vec<(
            i32,
            HashMap<std::string::String, dbus::arg::Variant<Box<(dyn RefArg + 'static)>>>,
        )>,
        MethodErr,
    > {
        call_with_icon(|icon| {
            let mut out_vec = Vec::new();
            for id in &*ids {
                let id = *id;
                let mut out = HashMap::new();
                for property_name in &*property_names {
                    out.insert(
                        property_name.clone(),
                        icon.get_property(id, property_name.clone())?,
                    );
                }
                out_vec.push((id, out));
            }
            if out_vec.is_empty() {
                return Err(dbus::MethodErr::failed("No matching IDs"));
            } else {
                return Ok(out_vec);
            }
        })
    }
    fn get_property(
        &mut self,
        id: i32,
        name: std::string::String,
    ) -> Result<dbus::arg::Variant<Box<(dyn RefArg + 'static)>>, MethodErr> {
        call_with_icon(|icon| icon.get_property(id, name))
    }
    fn event(
        &mut self,
        _: i32,
        _: std::string::String,
        _: dbus::arg::Variant<Box<(dyn RefArg + 'static)>>,
        _: u32,
    ) -> Result<(), MethodErr> {
        call_with_icon(|icon| {
            eprintln!("Got an event!");
            Ok(())
        })
    }
    fn event_group(
        &mut self,
        events: Vec<(
            i32,
            std::string::String,
            dbus::arg::Variant<Box<(dyn RefArg + 'static)>>,
            u32,
        )>,
    ) -> Result<Vec<i32>, MethodErr> {
        call_with_icon(|icon| {
            let mut not_found = vec![];
            let mut found_something = false;
            for (id, event_id, data, timestamp) in events.into_iter() {
                if icon.contains_id(id) {
                    icon.event(id, event_id, data, timestamp)?;
                    found_something = true;
                } else {
                    not_found.push(id)
                }
            }
            if !found_something {
                return Err(dbus::MethodErr::failed("No matching IDs"));
            } else {
                return Ok(not_found);
            }
        })
    }
    fn about_to_show(&mut self, id: i32) -> Result<bool, MethodErr> {
        call_with_icon(|icon| icon.about_to_show(id))
    }
    fn about_to_show_group(&mut self, ids: Vec<i32>) -> Result<(Vec<i32>, Vec<i32>), MethodErr> {
        call_with_icon(|icon| {
            let mut not_found = vec![];
            let mut invalidated = vec![];
            let mut found_something = false;
            for &id in &*ids {
                if icon.contains_id(id) {
                    if icon.about_to_show(id)? {
                        invalidated.push(id)
                    }
                    found_something = true;
                } else {
                    not_found.push(id)
                }
            }
            if !found_something {
                return Err(dbus::MethodErr::failed(
                    "No entry found with any of the ID numbers",
                ));
            } else {
                return Ok((invalidated, not_found));
            }
        })
    }
    fn version(&self) -> Result<u32, MethodErr> {
        Ok(1)
    }
    fn text_direction(&self) -> Result<std::string::String, MethodErr> {
        Ok("ltr".to_owned())
    }
    fn status(&self) -> Result<std::string::String, MethodErr> {
        Ok("normal".to_owned())
    }
    fn icon_theme_path(&self) -> Result<Vec<std::string::String>, MethodErr> {
        Err(dbus::MethodErr::no_property("IconThemePath"))
    }
}
