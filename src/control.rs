use std::rc::Rc;
use std::time::{Duration, Instant};
use log::{info, warn};
use crate::sensor::SensorValues;
use crate::metrics::CustomMetrics;

pub trait PeripheralControl<T: Eq> {
    fn should_query(&self, now: Instant) -> bool;
    fn update(&mut self, now: Instant, value: &T);
    fn remove_metric_if_stale(&self, now: Instant);
}

pub fn new_peripheral_control(
    update_interval: Duration,
    metrics: Rc<CustomMetrics>,
    label_values: &Vec<String>,
) -> Box<dyn PeripheralControl<SensorValues>> {
    return Box::new(
        PeripheralQueryControl{
            metrics, update_interval,
            label_values: label_values.clone(),
            query_control: new_query_control(update_interval),
            last_values: None,
            last_values_time: Instant::now(),
        }
    );
}
struct PeripheralQueryControl {
    metrics: Rc<CustomMetrics>,
    label_values: Vec<String>,
    query_control: Box<dyn QueryControl>,
    update_interval: Duration,

    last_values: Option<SensorValues>,
    last_values_time: Instant,
}

impl PeripheralControl<SensorValues> for PeripheralQueryControl {
    fn should_query(&self, now: Instant) -> bool {
        self.query_control.should_query(now)
    }

    fn update(&mut self, now: Instant, values: &SensorValues) {
        let changed = match &self.last_values {
            None => true,
            Some(last_values) => last_values != values,
        };

        self.last_values = Some((*values).clone());
        self.last_values_time = now;
        self.query_control.update(now, changed);

        let label_values: Vec<&str> = as_slice(&self.label_values);
        info!("device {:?}, {:?}", label_values, values);
        self.metrics.gauge_humidity.with_label_values(&label_values).set(values.humidity as f64);
        self.metrics.gauge_temp.with_label_values(&label_values).set(values.temp as f64);
        self.metrics.gauge_atm.with_label_values(&label_values).set(values.atm as f64);
        self.metrics.gauge_radon_short.with_label_values(&label_values).set(values.radon_short as i64);
        self.metrics.gauge_radon_long.with_label_values(&label_values).set(values.radon_long as i64);
        self.metrics.gauge_co2.with_label_values(&label_values).set(values.co2 as i64);
        self.metrics.gauge_voc.with_label_values(&label_values).set(values.voc as i64);
    }

    fn remove_metric_if_stale(&self, now: Instant) {
        if now.duration_since(self.last_values_time) > self.update_interval*2 {
            let label_values: Vec<&str> = as_slice(&self.label_values);
            warn!("peripheral {:?} has stale values, removing from metrics", label_values);
            self.metrics.gauge_humidity.remove_label_values(&label_values);
            self.metrics.gauge_temp.remove_label_values(&label_values);
            self.metrics.gauge_atm.remove_label_values(&label_values);
            self.metrics.gauge_radon_short.remove_label_values(&label_values);
            self.metrics.gauge_radon_long.remove_label_values(&label_values);
            self.metrics.gauge_co2.remove_label_values(&label_values);
            self.metrics.gauge_voc.remove_label_values(&label_values);
        }
    }
}

fn as_slice(vec: &Vec<String>) -> Vec<&str> {
    vec.iter().map(|x| &**x).collect()
}

pub trait QueryControl {
    fn should_query(&self, now: Instant) -> bool;
    fn update(&mut self, now: Instant, changed: bool);
}

pub fn new_query_control(update_interval: Duration) -> Box<dyn QueryControl> {
    return Box::new(BinarySearchQueryControl {
        sensor_update_interval: update_interval,
        expected_interval: None,
    })
}

struct BinarySearchQueryControl {
    sensor_update_interval: Duration,
    expected_interval: Option<(Instant, Instant)>,
}

impl QueryControl for BinarySearchQueryControl {
    fn should_query(&self, now: Instant) -> bool {
        return match self.expected_interval {
            None => true,
            Some(expected_interval) => {
                return now > Self::next_query_interval(expected_interval)
            }
        }
    }

    fn update(&mut self, now: Instant, changed: bool) {
        match self.expected_interval {
            None => self.expected_interval = Some((now, now+self.sensor_update_interval)),
            Some(expected_interval) => {
                if expected_interval.0 <= now && now <= expected_interval.1 {
                    // update falls inside the expected interval, let's slice
                    if changed {
                        // value already changed, selecting the first slice and advancing
                        self.expected_interval = Some((
                            expected_interval.0 + self.sensor_update_interval,
                            now + self.sensor_update_interval
                        ))
                    } else {
                        // value yet to be changed, selecting the second slice, no need to advance
                        self.expected_interval = Some((
                            now,
                            expected_interval.1
                        ))
                    }
                } else if now > expected_interval.1 {
                    // update falls outside of the expected interval in the future
                    if changed {
                        // we expect the value to change when past the expected interval
                        // in this case we can not slice the interval, so simply advance
                        let mut new_expected_interval = expected_interval.clone();
                        while new_expected_interval.1 < now {
                            new_expected_interval.0 = new_expected_interval.0 + self.sensor_update_interval;
                            new_expected_interval.1 = new_expected_interval.1 + self.sensor_update_interval;
                        }

                        self.expected_interval = Some(new_expected_interval);
                    } else {
                        // the value changed, which is unexpected, means our expected_interval is incorrect
                        // it is probably somewhere close, so let's keep polling frequently
                        self.expected_interval = Some((now, now + Duration::from_secs(10)));
                    }
                } else {
                    // remaining is the case when we're updating before the expected interval
                    if changed {
                        // we don't expect the value to change in this case
                        // if it did, let's assume the worst case and start over
                        self.expected_interval = Some((now, now + self.sensor_update_interval));
                    }
                }
            },
        }

        // if we arrived at the interval, that is too small, extend it back
        if let Some(expected_interval) = self.expected_interval {
            if expected_interval.1-expected_interval.0 < Duration::from_secs(10) {
                let center = expected_interval.0 + (expected_interval.1-expected_interval.0)/2;
                self.expected_interval = Some((
                    center - Duration::from_secs(5),
                    center + Duration::from_secs(5)
                ));
            }
        }
    }
}

impl BinarySearchQueryControl {
    fn next_query_interval(expected_interval: (Instant, Instant)) -> Instant {
        if expected_interval.1 - expected_interval.0 <= Duration::from_secs(10) {
            expected_interval.1
        } else {
            expected_interval.0 + (expected_interval.1 - expected_interval.0) / 2
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};
    use rand;
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn query_control_learns_sensor_update_times() {
        let mut rng = ChaCha8Rng::from_seed(Default::default());
        let mut now = Instant::now();

        for test in 0..1000 {
            let mut times = super::new_query_control(Duration::from_secs(5 * 60));
            let mut update_time = now + Duration::from_secs(rng.gen_range(0..300));
            let mut hits_streak = 0;

            for i in 0..60 * 60 {
                if times.should_query(now) {
                    if now > update_time && (now.duration_since(update_time)) <= Duration::from_secs(15) {
                        hits_streak += 1;
                    } else {
                        hits_streak = 0;
                    }

                    times.update(now, now > update_time);
                    while now > update_time {
                        update_time = update_time + Duration::from_secs(300);
                    }
                }

                now = now + Duration::from_secs(1);
            }

            assert_eq!(hits_streak >= 6, true, "last 6 queries should be spot on");
        }
    }

}
