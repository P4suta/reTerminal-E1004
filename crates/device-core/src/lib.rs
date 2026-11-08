#![no_std]
#![deny(clippy::panic, clippy::todo, clippy::unimplemented)]

pub mod app;
pub mod calendar;
pub mod commands;
pub mod daily;
pub mod geometry;
pub mod image;
pub mod media;
pub mod presentation;
pub mod settings;
pub mod telemetry;

#[cfg(kani)]
mod proofs;
