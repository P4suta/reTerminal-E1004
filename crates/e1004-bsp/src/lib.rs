#![no_std]

pub mod console;
pub mod fat;
pub mod input;
pub mod pins;

#[cfg(kani)]
mod proofs;
pub mod sensors;
pub mod storage;
