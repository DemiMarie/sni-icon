//! Utility libraries for use in Qubes OS.
//!
//! This crate is Qubes OS-specific and relies on Qubes OS C libraries.

mod safely_displayable;
mod simple_markup;
pub use safely_displayable::{NotSafelyDisplayable, SafelyDisplayable};
pub use simple_markup::SimpleMarkup;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smiley_unsafe() {
        match SafelyDisplayable::try_from("\u{1f642}") {
            Ok(_) => panic!("Emojies are not safe for display"),
            Err(NotSafelyDisplayable::UnsafeCodePoint { code_point, offset }) => {
                assert_eq!(code_point, '\u{1f642}'.into());
                assert_eq!(offset, 0);
            }
        }
    }
}
