use std::error::Error;
use std::panic;
use std::time::Duration;

use btleplug::api::{ScanFilter};
use btleplug::api::{Central, Manager as _, Peripheral};
use btleplug::platform::Manager;
use bytes::{Buf, Bytes};
use log::{debug, info, warn};
use tokio::time;
use uuid::Uuid;

extern crate pretty_env_logger;
#[macro_use] extern crate log;

const SENSORVALUES_CHARACTERISTIC_UUID: Uuid = Uuid::from_u128(0xb42e2a68_ade7_11e4_89d3_123b93f75cba);
const SENSORVALUES_SERVICE_UUID: Uuid = Uuid::from_u128(0xb42e1c08_ade7_11e4_89d3_123b93f75cba);

#[derive(Debug)]
pub struct SensorValues {
    humidity: f32,
    radon_short: u16,
    radon_long: u16,
    temp: f32,
    atm: f32,
    co2: u16,
    voc: u16,
}

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    pretty_env_logger::init();

    let manager = Manager::new().await?;
    let adapter_list = manager.adapters().await?;
    if adapter_list.is_empty() {
        panic!("No Bluetooth adapters found");
    }

    for adapter in adapter_list.iter() {
        info!("Starting scan...");
        adapter
            .start_scan(ScanFilter{services: vec!(SENSORVALUES_SERVICE_UUID)})
            .await
            .expect("Can't scan BLE adapter for connected devices...");
    }

    loop {
        time::sleep(Duration::from_secs(5)).await;

        for adapter in adapter_list.iter() {
            let peripherals = adapter.peripherals().await;

            if let Err(err) = peripherals {
                warn!("Could not get peripherals: {:?}", err);
                continue;
            }

            let peripherals = peripherals.unwrap();

            if peripherals.is_empty() {
                warn!("No peripheral devices found, skipping");
            } else {
                // All peripheral devices in range.
                debug!("discovered {} peripherals", peripherals.len());
                for peripheral in peripherals.iter() {
                    let properties = peripheral.properties().await;
                    if let Err(err) = properties {
                        info!("Failed to read properties from peripheral, skipping: {:?}", err);
                        continue;
                    }

                    let properties = properties.unwrap();
                    if let None = properties {
                        continue;
                    }

                    let properties = properties.unwrap();
                    let manufacturer_data = properties.manufacturer_data;

                    if manufacturer_data.contains_key(&820) {
                        let md = manufacturer_data.get(&820).unwrap();
                        let serial =
                            ((md[3] as u32) << 24) +
                            ((md[2] as u32) << 16) +
                            ((md[1] as u32) << 8) +
                            (md[0] as u32);

                        // Connect if we aren't already connected.
                        if let Err(err) = peripheral.connect().await {
                            info!("Error connecting to peripheral, skipping: {:?}", err);
                            continue;
                        }

                        // discover services and characteristics
                        if let Err(err) = peripheral.discover_services().await {
                            info!("Failed to discover services, skipping: {:?}", err);
                            continue;
                        }

                        // find the characteristic we want
                        let chars = peripheral.characteristics();
                        let char = chars
                            .iter()
                            .find(|c| c.uuid == SENSORVALUES_CHARACTERISTIC_UUID);

                        if let None = char {
                            info!("Failed to find correct characteristic, skipping");
                            continue;
                        }
                        let char = char.unwrap();

                        let data = peripheral.read(char).await;
                        if let Err(err) = data {
                            info!("Failed to read data from characteristic, skipping: {:?}", err);
                            continue;
                        }

                        let values = SensorValues::from_vec(data.unwrap());
                        debug!("serial {}, {:?}", serial, values);

                        if let Err(err) = peripheral.disconnect().await {
                            warn!("failed to disconnect from peripheral {:X}: {:?}", properties.address, err);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
