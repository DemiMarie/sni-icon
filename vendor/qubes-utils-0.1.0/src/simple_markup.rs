//! The [`SimpleMarkup`] type

use crate::SafelyDisplayable;
use core::fmt::{Debug, Display};
use core::ops::Deref;

/// A serializer for a simple markup language used by various [FreeDesktop.org](https://freedesktop.org)
/// standards.
///
/// Implementations of this markup language are typically not safe when used
/// with untrusted input.  Therefore, the serializer must ensure that only valid
/// markup is sent.  Furthermore, this markup will eventually be displayed to
/// a user, so the requirements of [`crate::SafelyDisplayable`] must also be
/// enforced.
///
/// TODO: support actually providing markup, rather than just escaping it.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SimpleMarkup {
    data: String,
}

impl Display for SimpleMarkup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&*self.data)
    }
}

impl Deref for SimpleMarkup {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl From<SimpleMarkup> for String {
    fn from(value: SimpleMarkup) -> Self {
        value.data
    }
}

impl SimpleMarkup {
    pub fn escape(data: SafelyDisplayable<'_>) -> Self {
        let mut v = Self::default();
        v.append_escaped(data);
        v
    }
    pub fn append_escaped(&mut self, data: SafelyDisplayable<'_>) {
        self.data.reserve(data.len());
        for i in data.chars() {
            match i {
                '>' => self.data.push_str("&gt;"),
                '<' => self.data.push_str("&lt;"),
                '"' => self.data.push_str("&quot;"),
                '\'' => self.data.push_str("&#x27;"),
                '&' => self.data.push_str("&amp;"),
                i => self.data.push(i),
            }
        }
    }
}
