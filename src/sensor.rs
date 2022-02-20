use std::collections::HashMap;
use bytes::{Buf, Bytes};

pub fn parse_serial(manufacturer_data: HashMap<u16, Vec<u8>>) -> Option<u32> {
    manufacturer_data.get(&820).map(|md| {
        ((md[3] as u32) << 24) +
        ((md[2] as u32) << 16) +
        ((md[1] as u32) << 8) +
        (md[0] as u32)
    })
}

#[derive(Debug, Clone)]
pub struct SensorValues {
    pub humidity: f32,
    pub temp: f32,
    pub atm: f32,
    pub radon_short: u16,
    pub radon_long: u16,
    pub co2: u16,
    pub voc: u16,
}

impl PartialEq for SensorValues {
    fn eq(&self, other: &Self) -> bool {
        if self.humidity != other.humidity && (!self.humidity.is_nan() || other.humidity.is_nan()) {
            return false;
        }
        if self.temp != other.temp && (!self.temp.is_nan() || other.temp.is_nan()) {
            return false;
        }
        if self.atm != other.atm && (!self.atm.is_nan() || other.atm.is_nan()) {
            return false;
        }

        if self.radon_short != other.radon_short {
            return false;
        }
        if self.radon_long != other.radon_long {
            return false;
        }
        if self.co2 != other.co2 {
            return false;
        }
        if self.voc != other.voc {
            return false;
        }

        return true;
    }
}

impl Eq for SensorValues {}

impl SensorValues {
    pub fn from_vec(data: Vec<u8>) -> SensorValues {
        let mut bytes = Bytes::from(data);

        bytes.advance(1);
        let humidity = (bytes.get_u8() as f32) / 2.0;
        bytes.advance(2);
        let radon_short = bytes.get_u16_le();
        let radon_long = bytes.get_u16_le();
        let temp = bytes.get_u16_le() as f32 / 100.0;
        let atm = bytes.get_u16_le() as f32 / 50.0;
        let co2 = bytes.get_u16_le();
        let voc = bytes.get_u16_le();

        return SensorValues{
            humidity, radon_short, radon_long, temp, atm, co2, voc,
        }
    }
}
