### Bugs
- don't continually remove stale values, do it once only
- discard invalid values at sensor startup:
  SensorValues { humidity: 127.5, temp: 382.2, atm: 1310.7, radon_short: 0, radon_long: 0, co2: 65535, voc: 65535 }
- `devices_labels.get(&*serial.to_string()).unwrap()` -> should support unknown devices

### Features
- try subscribing instead of polling
