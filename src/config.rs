use serde::{Deserialize, Serialize};
use std::fs::{File};
use std::io::{BufWriter, Read, Write};
use std::path::Path;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)] // PartialEq追加
pub enum OutputFormat {
    Text,
    CSV,
    JSON,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub segment_size: u64,
    pub chunk_size: usize,
    pub writer_buffer_size: usize,
    pub prime_min: String,
    pub prime_max: String,
    pub output_format: OutputFormat,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            segment_size: 10_000_000,
            chunk_size: 16_384,
            writer_buffer_size: 8 * 1024 * 1024,
            prime_min: "1".to_string(),
            prime_max: "1000000".to_string(),
            output_format: OutputFormat::Text, // Default is Text format
        }
    }
}

const SETTINGS_FILE: &str = "settings.txt";

pub fn load_or_create_config() -> Result<Config, Box<dyn std::error::Error>> {
    if Path::new(SETTINGS_FILE).exists() {
        let mut file = File::open(SETTINGS_FILE)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let config = toml::from_str(&contents)
            .map_err(|e| format!("Failed to parse the settings file: {}", e))?;
        Ok(config)
    } else {
        let config = Config::default();
        save_config(&config)?;
        Ok(config)
    }
}

pub fn save_config(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let toml_str = toml::to_string(config)?;
    let file = File::create(SETTINGS_FILE)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(toml_str.as_bytes())?;
    Ok(())
}
