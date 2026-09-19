//! The one error type of the crate. Fieldless, so it is one byte, `Copy`, and
//! returning it on a hot path costs nothing (CLAUDE.md §6: errors on a hot path
//! are fieldless).

/// Why an SBE read or write did not complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SbeError {
    /// The buffer ends before something the wire says is there: the 8-byte
    /// header, the root block `blockLength` announces, a group dimension, a
    /// group entry, a `varData` length prefix or its bytes. A truncated message
    /// is always this error, never a read past the end.
    Truncated,
    /// A field the schema places inside a block reaches past the block length
    /// the wire announced, although its `sinceVersion` says the encoder knew
    /// it. The message and the schema disagree.
    FieldOutsideBlock,
    /// The header's `schemaId` is not the schema the caller decodes with.
    WrongSchema,
    /// The header's `templateId` is not in the schema. The root block is
    /// still delimited by `blockLength`; nothing after it can be walked.
    UnknownTemplate,
    /// An element index past the end of a field's element table.
    NoSuchElement,
    /// The output buffer is too small for what is being written.
    BufferTooSmall,
    /// Writing: a field, group or `varData` id that the block being written
    /// does not have.
    UnknownField,
    /// Writing: raw field bytes whose length is not the field's wire length.
    WrongLength,
    /// Writing: a value that the element cannot hold — the wrong [`Value`]
    /// kind for its primitive, out of range for its width, an array of the
    /// wrong length, `None` (null) for a required element, or a value other
    /// than the schema's for a constant.
    ///
    /// [`Value`]: crate::Value
    BadValue,
    /// Writing: a group count or `varData` length that does not fit the width
    /// the schema gives its `numInGroup` or `length`.
    LengthOverflow,
    /// Writing: a group or `varData` requested behind one already written.
    /// SBE puts them on the wire in schema order, groups before `varData`, and
    /// a forward-only writer cannot go back.
    OutOfOrder,
}
