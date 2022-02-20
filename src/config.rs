use std::collections::{HashMap, HashSet};
use std::fs;
use toml::Value;
use toml::Value::Table;

pub fn load_device_labels() -> (HashMap<String, Vec<String>>, Vec<String>) {
    let config_str = fs::read_to_string("devices.toml").unwrap();
    let value = config_str.parse::<Value>().unwrap();

    let mut devices: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut label_names: HashSet<String> = HashSet::new();
    if let Table(root_table) = value {
        for (serial, labels_value) in root_table {
            let mut labels_map: HashMap<String, String> = HashMap::new();
            labels_map.insert(String::from("serial"), serial.clone());

            if let Table(device_table) = labels_value {
                for (name, value) in device_table {
                    if let Value::String(str_value) = value {
                        labels_map.insert(name.clone(), str_value);
                        label_names.insert(name);
                    }
                }
            }

            devices.insert(serial, labels_map);
        }
    }

    let mut label_names_vec = vec![String::from("serial")];
    for name in label_names {
        label_names_vec.push(name);
    }

    let default_value = String::from("");
    let mut devices_labels: HashMap<String, Vec<String>> = HashMap::new();
    for (serial, device_labels) in devices {
        let mut label_values: Vec<String> = Vec::new();

        for name in &label_names_vec {
            let value = device_labels.get(name).unwrap_or(&default_value);
            label_values.push(value.clone());
        }

        devices_labels.insert(serial, label_values);
    }

    return (devices_labels, label_names_vec);
}
