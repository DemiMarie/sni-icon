//! The [`SafelyDisplayable`] type

use core::convert::TryFrom;
use core::fmt::{Debug, Display};
use core::ops::Deref;
use std::error::Error;

/// A string that can safely be displayed to a user, and which
/// will not be able to exploit vulnerabilities in C or C++
/// text rendering libraries.
///
/// Such strings can be safely displayed even in dom0, and even
/// if they come from an untrusted source.
///
/// This is convertable to [`str`].
pub struct SafelyDisplayable<'a>(&'a str);

/// Error that indicates a string is not safely displayable
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum NotSafelyDisplayable {
    /// Indicates that an unsafe code point was found at the given byte offset.
    UnsafeCodePoint { code_point: u32, offset: usize },
}

impl Display for NotSafelyDisplayable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsafeCodePoint { code_point, offset } => f.write_fmt(format_args!(
                "code point {} at byte offset {} is not safe to display",
                code_point, offset
            )),
        }
    }
}

impl Error for NotSafelyDisplayable {}

impl<'a> TryFrom<&'a str> for SafelyDisplayable<'a> {
    type Error = NotSafelyDisplayable;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        // This could be implemented as an FFI call, but it is _much_
        // nicer to use the functionality in the Rust standard library.
        for (offset, code_point) in value.char_indices() {
            // SAFETY: this function is not really "unsafe"
            if !unsafe {
                qubes_utils_sys::qubes_pure_code_point_safe_for_display(code_point as u32)
            } {
                return Err(NotSafelyDisplayable::UnsafeCodePoint {
                    code_point: code_point as u32,
                    offset,
                });
            }
        }

        Ok(Self(value))
    }
}

// TODO: Some methods can return a SafelyDisplayable<'a> instead of just
// an &'a str.  These will be added as-needed, instead of trying to implement
// them all right now.
impl<'a> Deref for SafelyDisplayable<'a> {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<'a> From<SafelyDisplayable<'a>> for &'a str {
    fn from(value: SafelyDisplayable<'a>) -> Self {
        value.0
    }
}
