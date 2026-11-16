//! The PiP window. TEMPORARY stub until Task 4 of the Win32 rewrite.

use super::app::{App, Op};

/// A PiP window action that runs after the state borrow ends.
pub enum PipOp {}

pub fn show(_app: &mut App) -> Vec<Op> {
    Vec::new()
}

pub fn run_op(op: PipOp) {
    match op {}
}
