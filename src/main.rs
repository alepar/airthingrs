use std::{fs, panic};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use btleplug::api::{BDAddr, ScanFilter};
use btleplug::api::{Central, Manager as _, Peripheral};
use btleplug::platform::{Adapter, Manager};
use bytes::{Buf, Bytes};
use log::{debug, info, trace, warn};
use prometheus::{GaugeVec, IntGaugeVec, Opts, Registry};
use prometheus::core::Collector;
use prometheus_hyper::{RegistryFn, Server};
use tokio::sync::Notify;
use tokio::time;
use toml::Value;
use toml::Value::Table;
use uuid::Uuid;

use metrics::CustomMetrics;
use sensor::SensorValues;

use crate::control::PeripheralControl;

mod control;
mod config;
mod logging;
mod metrics;
mod sensor;

const SENSORVALUES_CHARACTERISTIC_UUID: Uuid = Uuid::from_u128(0xb42e2a68_ade7_11e4_89d3_123b93f75cba);
const SENSORVALUES_SERVICE_UUID: Uuid = Uuid::from_u128(0xb42e1c08_ade7_11e4_89d3_123b93f75cba);

#[tokio::main]
async fn main() -> Result<()> {
    logging::init_logger()?;

    let (devices, label_names) = config::load_device_labels();

    let metrics = metrics::create_metrics(&label_names);
    let adapter_list = start_scanning().await
        .expect("could not set adapters up to start scanning");

    let mut peripheral_controls: HashMap<u32, Box<dyn PeripheralControl<SensorValues>>> = HashMap::new();
    let metrics = Rc::new(metrics);

    loop {
        time::sleep(Duration::from_secs(5)).await;
        query_peripherals(&metrics, &adapter_list, &devices, &mut peripheral_controls).await;

        for control in peripheral_controls.values() {
            control.remove_metric_if_stale(Instant::now());
        }
    }
}

async fn query_peripherals(
    metrics: &Rc<CustomMetrics>,
    adapter_list: &Vec<Adapter>,
    devices_labels: &HashMap<String, Vec<String>>,
    controls: &mut HashMap<u32, Box<dyn PeripheralControl<SensorValues>>>
) {
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
        trace!("discovered {} peripherals", peripherals.len());
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

            if let Some(serial) = sensor::parse_serial(manufacturer_data) {
                let peripheral_control = controls.entry(serial).or_insert_with(||
                    control::new_peripheral_control(
                        Duration::from_secs(5*60),
                        Rc::clone(metrics),
                        devices_labels.get(&*serial.to_string()).unwrap(),
                    )
                );

                if !peripheral_control.should_query(Instant::now()) {
                    trace!("peripheral {} queried recently, skipping", serial);
                    continue;
                }

                trace!("querying peripheral {}", serial);
                let result = query_peripheral(peripheral, peripheral_control).await;
                if let Err(err) = result {
                    debug!("Failed to query peripheral {}, skipped: {:?}", serial, err);
                }

                // don't ever disconnect, it's a noop atm anyway
            }
        }
    }
}

async fn query_peripheral(peripheral: &impl Peripheral, peripheral_control: &mut Box<dyn PeripheralControl<SensorValues>>) -> Result<()> {
    // Connect if we aren't already connected.
    let is_connected = peripheral.is_connected().await.context("Failed to check if device is connected")?;
    if !is_connected {
        peripheral.connect().await.context("Failed to connect to a peripheral")?
    }

    // discover services and characteristics
    peripheral.discover_services().await.context("Failed to discover services")?;

    // find the characteristic we want
    let chars = peripheral.characteristics();
    let char = chars
        .iter()
        .find(|c| c.uuid == SENSORVALUES_CHARACTERISTIC_UUID);

    if let None = char {
        return Err(anyhow!("Failed to find correct characteristic"));
    }
    let char = char.unwrap();

    let data = peripheral.read(char).await.context("Failed to read data from characteristic")?;
    peripheral_control.update(Instant::now(), &SensorValues::from_vec(data));
    Ok(())
}

async fn start_scanning() -> Result<Vec<Adapter>> {
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
