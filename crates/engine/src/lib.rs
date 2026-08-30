//! The engine: TCP on one side, [`nanofix_session`] on the other, and a thread
//! that never sleeps in the kernel.
//!
//! `DESIGN.md` D8 is the shape of this crate: the loop spins on non-blocking
//! sockets, and a blocking call on the engine thread is a bug rather than a
//! style choice (`CLAUDE.md` §2 non-negotiable 4).

pub mod frame;
pub mod transport;
pub mod wait;
