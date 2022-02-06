use std::error::Error;
use std::net::SocketAddr;
use std::panic;
use std::sync::Arc;
use std::time::Duration;

use btleplug::api::{ScanFilter};
use btleplug::api::{Central, Manager as _, Peripheral};
use btleplug::platform::{Adapter, Manager};
use bytes::{Buf, Bytes};
use log::{debug, info, warn};
use prometheus_hyper::{RegistryFn, Server};
use tokio::sync::Notify;
use prometheus::{Opts, Registry, GaugeVec, IntGaugeVec};
use prometheus::core::Collector;
use tokio::time;
use uuid::Uuid;

extern crate pretty_env_logger;

const SENSORVALUES_CHARACTERISTIC_UUID: Uuid = Uuid::from_u128(0xb42e2a68_ade7_11e4_89d3_123b93f75cba);
const SENSORVALUES_SERVICE_UUID: Uuid = Uuid::from_u128(0xb42e1c08_ade7_11e4_89d3_123b93f75cba);

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    pretty_env_logger::init();

    let metrics = create_metrics();
    let adapter_list = start_scanning().await
        .expect("could not set adapters up to start scanning");

    loop {
        time::sleep(Duration::from_secs(5)).await;
        query_peripherals(&metrics, &adapter_list).await;
    }
}

async fn query_peripherals(metrics: &CustomMetrics, adapter_list: &Vec<Adapter>) {
    for adapter in adapter_list.iter() {
        let peripherals = adapter.peripherals().await;

        if let Err(err) = peripherals {
            warn!("Could not get peripherals: {:?}", err);
            continue;
        }

        let peripherals = peripherals.unwrap();
        if peripherals.is_empty() {
            debug!("No peripheral devices found, skipping");
            continue;
        }

        // All peripheral devices in range.
        debug!("discovered {} peripherals", peripherals.len());
        for peripheral in peripherals.iter() {
            let properties = peripheral.properties().await;
            if let Err(err) = properties {
                debug!("Failed to read properties from peripheral, skipping: {:?}", err);
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
                    debug!("Error connecting to peripheral, skipping: {:?}", err);
                    continue;
                }

                query_peripheral(metrics, peripheral, serial).await;

                if let Err(err) = peripheral.disconnect().await {
                    debug!("failed to disconnect from peripheral {:X}: {:?}", properties.address, err);
                }
            }
        }
    }
}

async fn query_peripheral(metrics: &CustomMetrics, peripheral: &impl Peripheral, serial: u32) {
    // discover services and characteristics
    if let Err(err) = peripheral.discover_services().await {
        debug!("Failed to discover services, skipping: {:?}", err);
        return;
    }

    // find the characteristic we want
    let chars = peripheral.characteristics();
    let char = chars
        .iter()
        .find(|c| c.uuid == SENSORVALUES_CHARACTERISTIC_UUID);

    if let None = char {
        debug!("Failed to find correct characteristic, skipping");
        return;
    }
    let char = char.unwrap();

    let data = peripheral.read(char).await;
    if let Err(err) = data {
        debug!("Failed to read data from characteristic, skipping: {:?}", err);
        return;
    }

    let values = SensorValues::from_vec(data.unwrap());
    info!("serial {}, {:?}", serial, values);

    let label_values = &[&*serial.to_string()];
    metrics.gauge_humidity.with_label_values(label_values).set(values.humidity as f64);
    metrics.gauge_temp.with_label_values(label_values).set(values.temp as f64);
    metrics.gauge_atm.with_label_values(label_values).set(values.atm as f64);
    metrics.gauge_radon_short.with_label_values(label_values).set(values.radon_short as i64);
    metrics.gauge_radon_long.with_label_values(label_values).set(values.radon_long as i64);
    metrics.gauge_co2.with_label_values(label_values).set(values.co2 as i64);
    metrics.gauge_voc.with_label_values(label_values).set(values.voc as i64);
}

async fn start_scanning() -> Result<Vec<Adapter>, Box<dyn Error>> {
    let manager = Manager::new().await?;
    let adapter_list = manager.adapters().await?;
    if adapter_list.is_empty() {
        panic!("No Bluetooth adapters found");
    }

    for adapter in adapter_list.iter() {
        info!("Starting scan...");
        adapter
            .start_scan(ScanFilter { services: vec!(SENSORVALUES_SERVICE_UUID) })
            .await
            .expect("Can't scan BLE adapter for connected devices...");
    }

    Ok(adapter_list)
}

fn create_metrics() -> CustomMetrics {
    let registry = Arc::new(Registry::new());
    let shutdown = Arc::new(Notify::new());
    let shutdown_clone = Arc::clone(&shutdown);
    let (metrics, f) = CustomMetrics::new().expect("failed prometheus");
    f(&registry).expect("problem registering");

    // Startup Server
    let jh = tokio::spawn(async move {
        Server::run(
            Arc::clone(&registry),
            SocketAddr::from(([0; 4], 8080)),
            shutdown_clone.notified(),
        ).await
    });
    metrics
}

#[derive(Debug)]
pub struct SensorValues {
    pub humidity: f32,
    pub temp: f32,
    pub atm: f32,
    pub radon_short: u16,
    pub radon_long: u16,
    pub co2: u16,
    pub voc: u16,
}

pub struct CustomMetrics {
    pub gauge_humidity: GaugeVec,
    pub gauge_temp: GaugeVec,
    pub gauge_atm: GaugeVec,
    pub gauge_radon_short: IntGaugeVec,
    pub gauge_radon_long: IntGaugeVec,
    pub gauge_co2: IntGaugeVec,
    pub gauge_voc: IntGaugeVec,
}

impl CustomMetrics {
    pub fn new() -> Result<(Self, RegistryFn), Box<dyn Error>> {
        let label_names = ["serial"];

        let metrics = Self {
            gauge_humidity: GaugeVec::new(Opts::new("humidity", "in rel%"), &label_names)?,
            gauge_temp: GaugeVec::new(Opts::new("temperature", "air temperature, in C"), &label_names)?,
            gauge_atm: GaugeVec::new(Opts::new("atm_pressure", "atmospheric pressure, in mbar"), &label_names)?,
            gauge_radon_short: IntGaugeVec::new(Opts::new("radon_short", "in Bq/m3"), &label_names)?,
            gauge_radon_long: IntGaugeVec::new(Opts::new("radon_long", "in Bq/m3"), &label_names)?,
            gauge_voc: IntGaugeVec::new(Opts::new("voc", "in ppb"), &label_names)?,
            gauge_co2: IntGaugeVec::new(Opts::new("co2", "in ppm"), &label_names)?,
        };

        let to_register: Vec<Box<dyn Collector>> = vec!(
            Box::new(metrics.gauge_humidity.clone()),
            Box::new(metrics.gauge_temp.clone()),
            Box::new(metrics.gauge_atm.clone()),
            Box::new(metrics.gauge_radon_short.clone()),
            Box::new(metrics.gauge_radon_long.clone()),
            Box::new(metrics.gauge_voc.clone()),
            Box::new(metrics.gauge_co2.clone()),
        );

        let f = |r: &Registry| {
            for m in to_register {
                r.register(m);
            }
            Ok(())
        };

        Ok((metrics, Box::new(f)))
    }
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
