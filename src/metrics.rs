use prometheus::{GaugeVec, IntGaugeVec, Opts, Registry};
use std::sync::Arc;
use tokio::sync::Notify;
use prometheus_hyper::{RegistryFn, Server};
use std::net::SocketAddr;
use prometheus::core::Collector;

pub fn create_metrics(label_names: &Vec<String>) -> CustomMetrics {
    let registry = Arc::new(Registry::new());
    let shutdown = Arc::new(Notify::new());
    let shutdown_clone = Arc::clone(&shutdown);
    let (metrics, f) = CustomMetrics::new(label_names)
        .expect("failed creating metrics");
    f(&registry).expect("failed registering metrics");

    // Startup Server
    let _jh = tokio::spawn(async move {
        Server::run(
            Arc::clone(&registry),
            SocketAddr::from(([0; 4], 8080)),
            shutdown_clone.notified(),
        ).await
    });
    metrics
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
    pub fn new(label_names: &Vec<String>) -> anyhow::Result<(Self, RegistryFn)> {
        let mut slice: Vec<&str> = Vec::new();
        for s in label_names {
            slice.push(&s);
        }
        let slice = slice.as_slice();

        let metrics = Self {
            gauge_humidity: GaugeVec::new(Opts::new("humidity", "in rel%"), slice)?,
            gauge_temp: GaugeVec::new(Opts::new("temperature", "air temperature, in C"), slice)?,
            gauge_atm: GaugeVec::new(Opts::new("atm_pressure", "atmospheric pressure, in mbar"), slice)?,
            gauge_radon_short: IntGaugeVec::new(Opts::new("radon_short", "in Bq/m3"), slice)?,
            gauge_radon_long: IntGaugeVec::new(Opts::new("radon_long", "in Bq/m3"), slice)?,
            gauge_voc: IntGaugeVec::new(Opts::new("voc", "in ppb"), slice)?,
            gauge_co2: IntGaugeVec::new(Opts::new("co2", "in ppm"), slice)?,
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
                r.register(m)?;
            }
            Ok(())
        };

        Ok((metrics, Box::new(f)))
    }
}
