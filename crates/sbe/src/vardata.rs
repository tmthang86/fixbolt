//! `varData`: a length prefix, then that many bytes (SBE 1.0 RC4, "Variable-length string encoding").

use crate::error::SbeError;
use crate::group::{Cursor, read_length};
use crate::schema::VarDataLayout;
use crate::wire;

impl<'a> Cursor<'a> {
    /// Reads `varData` `layout` at the cursor and returns its bytes with the
    /// cursor after it.
    ///
    /// `None` — and the cursor unmoved — when the message's version predates
    /// the element (`sinceVersion` > header `version`): it is not on the wire.
    /// A present element of length 0 is `Some(&[])`; SBE makes no distinction
    /// between empty and null data (the same section).
    ///
    /// # Errors
    /// `Truncated` when the length prefix or the bytes it announces are cut
    /// off.
    #[inline]
    pub fn var_data(
        self,
        layout: &'static VarDataLayout,
    ) -> Result<(Option<&'a [u8]>, Cursor<'a>), SbeError> {
        if layout.since_version > self.version {
            return Ok((None, self));
        }
        let len = read_length(self.buf, self.pos, layout.length, self.order)?;
        let start = self
            .pos
            .checked_add(layout.length.size())
            .ok_or(SbeError::Truncated)?;
        let data = wire::slice(self.buf, start, len)?;
        Ok((
            Some(data),
            Cursor {
                pos: start + len,
                ..self
            },
        ))
    }
}
