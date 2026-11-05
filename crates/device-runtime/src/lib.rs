#![no_std]

#[path = "../../../firmware/src/effects.rs"]
mod effect_dispatch;

#[cfg(feature = "ble")]
pub mod ble;
pub mod buttons;
pub mod console;
pub mod measurements;
#[cfg(any(feature = "ble", kani))]
pub mod packet_pool;
pub mod policy;
#[cfg(kani)]
mod proofs;
pub mod state;
pub mod tasks;
pub mod wifi;

#[cfg(test)]
extern crate std;
