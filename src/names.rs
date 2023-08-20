//! Functions to obtain various D-Bus names

use dbus::strings::{BusName, ErrorName, Signature};
use dbus::strings::{Interface, Member, Path};
pub fn interface_com_canonical_dbusmenu() -> Interface<'static> {
    // SAFETY: this is a valid NUL-terminated interface name
    unsafe { Interface::from_slice_unchecked("com.canonical.dbusmenu\0") }
}

pub fn name_owner_changed() -> Member<'static> {
    // SAFETY: this is a valid NUL-terminated member name
    unsafe { Member::from_slice_unchecked("NameOwnerChanged\0") }
}

pub fn get_layout() -> Member<'static> {
    // SAFETY: this is a valid NUL-terminated member name
    unsafe { Member::from_slice_unchecked("GetLayout\0") }
}

pub fn interface_dbus() -> Interface<'static> {
    // SAFETY: this is a valid NUL-terminated interface name
    unsafe { Interface::from_slice_unchecked("org.freedesktop.DBus\0") }
}

pub fn path_dbus() -> Path<'static> {
    // SAFETY: this is a valid NUL-terminated path name
    unsafe { Path::from_slice_unchecked("/org/freedesktop/DBus\0") }
}

pub fn name_dbus() -> BusName<'static> {
    // SAFETY: this is a valid NUL-terminated bus name
    unsafe { BusName::from_slice_unchecked("org.freedesktop.DBus\0") }
}

pub fn name_status_notifier_watcher() -> BusName<'static> {
    // SAFETY: this is a valid NUL-terminated bus name
    unsafe { BusName::from_slice_unchecked("org.kde.StatusNotifierWatcher\0") }
}

pub fn interface_status_notifier_watcher() -> Interface<'static> {
    // SAFETY: this is a valid NUL-terminated interface name
    unsafe { Interface::from_slice_unchecked("org.kde.StatusNotifierWatcher\0") }
}

pub fn layout_updated<'a, 'b: 'a, 'c: 'a>(
    b: BusName<'b>,
    p: Path<'c>,
) -> dbus::message::MatchRule<'a> {
    // SAFETY: this is a valid NUL-terminated member name
    let member = unsafe { Member::from_slice_unchecked("LayoutUpdated\0") };
    dbus::message::MatchRule::new_signal(interface_com_canonical_dbusmenu(), member)
        .with_strict_sender(b)
        .with_path(p)
}

pub fn path_status_notifier_watcher() -> Path<'static> {
    // SAFETY: this is a valid NUL-terminated path name
    unsafe { Path::from_slice_unchecked("/StatusNotifierWatcher\0") }
}

pub fn register_status_notifier_item() -> Member<'static> {
    // SAFETY: this is a valid NUL-terminated member name
    unsafe { Member::from_slice_unchecked("RegisterStatusNotifierItem\0") }
}

pub fn path_status_notifier_item() -> Path<'static> {
    // SAFETY: this is a valid NUL-terminated path name
    unsafe { Path::from_slice_unchecked("/StatusNotifierItem\0") }
}
