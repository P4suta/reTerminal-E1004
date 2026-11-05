use device_core::telemetry::Fault;
use e1004_bsp::sensors::{sensor_crc, sht40_decode};

#[test]
fn the_publisher_checksum_vector_and_every_single_wire_bit_error_are_checked() {
    assert_eq!(sensor_crc([0xbe, 0xef]), 0x92);
    for word in [0u16, 0xbeef, u16::MAX] {
        let [high, low] = word.to_be_bytes();
        let checksum = sensor_crc([high, low]);
        let valid = [high, low, checksum, high, low, checksum];
        assert!(sht40_decode(valid).is_ok());
        for bit in 0..48 {
            let mut damaged = valid;
            damaged[bit / 8] ^= 1 << (bit % 8);
            assert_eq!(
                sht40_decode(damaged),
                Err(Fault::SensorCrc),
                "word={word} bit={bit}"
            );
        }
    }
}

#[test]
fn every_sensor_word_agrees_with_the_independent_signed_rational_specification() {
    for word in 0u16..=u16::MAX {
        let [high, low] = word.to_be_bytes();
        let checksum = sensor_crc([high, low]);
        let (temperature, humidity) =
            sht40_decode([high, low, checksum, high, low, checksum]).unwrap();
        let ticks = i64::from(word);
        let expected_temperature = (17500 * ticks - 4500 * 65535).div_euclid(65535);
        let expected_humidity = (12500 * ticks - 600 * 65535)
            .div_euclid(65535)
            .clamp(0, 10000);
        assert_eq!(i64::from(temperature.get()), expected_temperature);
        assert_eq!(i64::from(humidity.get()), expected_humidity);
    }
}
