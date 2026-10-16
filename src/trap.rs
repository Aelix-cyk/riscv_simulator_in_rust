//! Architectural faults.

/// A fault raised while executing or fetching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trap {
    /// The encoding is not a legal instruction.
    IllegalInstruction,
    /// An access fell outside any implemented region.
    AccessFault,
    /// A jump or branch target was not four-byte aligned.
    InstructionAddressMisaligned,
}
