pub(crate) const PACKET_CAPACITY: u8 = 16;
pub(crate) const PACKET_BYTES: usize = 251;

pub(crate) fn reserve(used: u8) -> Option<u8> {
    if used < PACKET_CAPACITY {
        Some(used + 1)
    } else {
        None
    }
}

pub(crate) fn release(used: u8) -> Option<u8> {
    if used > 0 && used <= PACKET_CAPACITY {
        Some(used - 1)
    } else {
        None
    }
}

#[cfg(feature = "ble")]
mod owned {
    use super::*;
    use core::cell::Cell;
    use embassy_sync::blocking_mutex::{raw::CriticalSectionRawMutex, Mutex};
    use trouble_host::{Packet, PacketPool};

    static USED: Mutex<CriticalSectionRawMutex, Cell<u8>> = Mutex::new(Cell::new(0));

    pub struct OwnedPacketPool;
    pub struct OwnedPacket {
        bytes: [u8; PACKET_BYTES],
    }
    impl PacketPool for OwnedPacketPool {
        type Packet = OwnedPacket;
        const MTU: usize = PACKET_BYTES;
        fn capacity() -> usize {
            usize::from(PACKET_CAPACITY)
        }
        fn allocate() -> Option<OwnedPacket> {
            USED.lock(|used| {
                let next = reserve(used.get())?;
                used.set(next);
                Some(())
            })?;
            Some(OwnedPacket {
                bytes: [0; PACKET_BYTES],
            })
        }
    }
    impl Packet for OwnedPacket {}
    impl AsRef<[u8]> for OwnedPacket {
        fn as_ref(&self) -> &[u8] {
            &self.bytes
        }
    }
    impl AsMut<[u8]> for OwnedPacket {
        fn as_mut(&mut self) -> &mut [u8] {
            &mut self.bytes
        }
    }
    impl Drop for OwnedPacket {
        fn drop(&mut self) {
            USED.lock(|used| {
                used.set(release(used.get()).expect("packet released without a reservation"));
            });
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn packets_own_disjoint_buffers_and_release_the_complete_bounded_capacity() {
            for count in 0u8..=u8::MAX {
                let wide = u16::from(count);
                assert_eq!(
                    reserve(count).map(u16::from),
                    (wide < 16).then_some(wide + 1)
                );
                assert_eq!(
                    release(count).map(u16::from),
                    match wide {
                        1..=16 => Some(wide - 1),
                        _ => None,
                    }
                );
            }
            assert_eq!(OwnedPacketPool::capacity(), 16);
            assert_eq!(OwnedPacketPool::MTU, 251);
            let mut packets = (0..16)
                .map(|value| {
                    let mut packet = OwnedPacketPool::allocate().unwrap();
                    assert!(packet.as_ref().iter().all(|byte| *byte == 0));
                    packet.as_mut().fill(value);
                    packet
                })
                .collect::<std::vec::Vec<_>>();
            assert!(OwnedPacketPool::allocate().is_none());
            for (value, packet) in packets.iter_mut().enumerate() {
                assert!(packet.as_ref().iter().all(|byte| *byte == value as u8));
                packet.as_mut()[250] = 255;
            }
            let moved = packets.remove(3);
            assert_eq!(moved.as_ref()[0], 3);
            drop(moved);
            let reused = OwnedPacketPool::allocate().unwrap();
            assert!(reused.as_ref().iter().all(|byte| *byte == 0));
            assert!(OwnedPacketPool::allocate().is_none());
            drop(reused);
            drop(packets);
            assert_eq!(USED.lock(Cell::get), 0);
        }
    }
}

#[cfg(feature = "ble")]
pub use owned::{OwnedPacket, OwnedPacketPool};
