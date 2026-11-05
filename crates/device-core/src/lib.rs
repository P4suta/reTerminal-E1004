#![no_std]
#![deny(clippy::panic, clippy::todo, clippy::unimplemented)]

pub mod app;
pub mod geometry;
pub mod presentation;
pub mod settings;
pub mod telemetry;

#[cfg(kani)]
mod proofs;
