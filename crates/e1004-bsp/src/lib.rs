#![no_std]

pub mod console;
pub mod fat;
pub mod input;
pub mod pins;

pub mod media;
#[cfg(kani)]
mod proofs;
pub mod sensors;
pub mod storage;
