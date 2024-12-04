use eframe::{egui, App};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use bitvec::prelude::*;
use rayon::prelude::*;

const SETTINGS_FILE: &str = "settings.txt";

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Config {
    prime_cache_size: usize,
    segment_size: u64,
    chunk_size: usize,
    writer_buffer_size: usize,
    prime_min: u64,
    prime_max: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            prime_cache_size: 100_000,
            segment_size: 10_000_000,
            chunk_size: 16_384,
            writer_buffer_size: 8 * 1024 * 1024,
            prime_min: 1,
            prime_max: 10_000_000_000,
        }
    }
}

fn main() {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Prime Number Generation Program",
        options,
        Box::new(|cc| Box::new(MyApp::new(cc))),
    );
}

struct MyApp {
    config: Config,
    is_running: bool,
    log: String,
    receiver: Option<mpsc::Receiver<String>>,
    prime_min_input: String,
    prime_max_input: String,
}

impl MyApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let config = load_or_create_config().unwrap_or_default();
        MyApp {
            prime_min_input: config.prime_min.to_string(),
            prime_max_input: config.prime_max.to_string(),
            config,
            is_running: false,
            log: String::new(),
            receiver: None,
        }
    }
}

impl App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Receive messages from the thread
        if let Some(ref receiver) = self.receiver {
            let mut remove_receiver = false;
            while let Ok(message) = receiver.try_recv() {
                if message == "done" {
                    self.is_running = false;
                    remove_receiver = true;
                } else {
                    self.log.push_str(&message);
                }
            }

            if remove_receiver {
                self.receiver = None;
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Settings");

            ui.add(
                egui::Slider::new(&mut self.config.prime_cache_size, 1..=1_000_000)
                    .text("prime_cache_size"),
            );
            ui.add(
                egui::Slider::new(&mut self.config.segment_size, 1_000_000..=100_000_000)
                    .text("segment_size"),
            );
            ui.add(
                egui::Slider::new(&mut self.config.chunk_size, 1_024..=65_536)
                    .text("chunk_size"),
            );
            ui.add(
                egui::Slider::new(
                    &mut self.config.writer_buffer_size,
                    1024..=16 * 1024 * 1024,
                )
                .text("writer_buffer_size"),
            );

            ui.horizontal(|ui| {
                ui.label("prime_min:");
                ui.text_edit_singleline(&mut self.prime_min_input);
            });

            ui.horizontal(|ui| {
                ui.label("prime_max:");
                ui.text_edit_singleline(&mut self.prime_max_input);
            });

            if ui.button("Save Settings").clicked() {
                let mut errors = Vec::new();

                // Parse the value of prime_min
                match self.prime_min_input.trim().parse::<u64>() {
                    Ok(value) => self.config.prime_min = value,
                    Err(_) => errors.push("Invalid value for prime_min. Please enter a positive integer."),
                }

                // Parse the value of prime_max
                match self.prime_max_input.trim().parse::<u64>() {
                    Ok(value) => self.config.prime_max = value,
                    Err(_) => errors.push("Invalid value for prime_max. Please enter a positive integer."),
                }

                // Check the relationship between prime_min and prime_max
                if self.config.prime_min >= self.config.prime_max {
                    errors.push("prime_min must be less than prime_max.");
                }

                if errors.is_empty() {
                    if let Err(e) = save_config(&self.config) {
                        self.log
                            .push_str(&format!("Failed to save settings: {}\n", e));
                    } else {
                        self.log.push_str("Settings saved.\n");
                    }
                } else {
                    for error in errors {
                        self.log.push_str(&format!("{}\n", error));
                    }
                }
            }

            if ui.button("Run").clicked() && !self.is_running {
                // Validate input before running
                let mut errors = Vec::new();

                // Parse the value of prime_min
                match self.prime_min_input.trim().parse::<u64>() {
                    Ok(value) => self.config.prime_min = value,
                    Err(_) => errors.push("Invalid value for prime_min. Please enter a positive integer."),
                }

                // Parse the value of prime_max
                match self.prime_max_input.trim().parse::<u64>() {
                    Ok(value) => self.config.prime_max = value,
                    Err(_) => errors.push("Invalid value for prime_max. Please enter a positive integer."),
                }

                // Check the relationship between prime_min and prime_max
                if self.config.prime_min >= self.config.prime_max {
                    errors.push("prime_min must be less than prime_max.");
                }

                if errors.is_empty() {
                    self.is_running = true;
                    let config = self.config.clone();
                    let (sender, receiver) = mpsc::channel();
                    self.receiver = Some(receiver);

                    // Execute in a separate thread
                    thread::spawn(move || {
                        if let Err(e) = run_program(config, sender.clone()) {
                            let _ = sender.send(format!("An error occurred: {}\n", e));
                        }
                        // Notify completion
                        let _ = sender.send("done".to_string());
                    });
                } else {
                    for error in errors {
                        self.log.push_str(&format!("{}\n", error));
                    }
                }
            }

            if self.is_running {
                ui.label("Running...");
            } else {
                ui.label("Idle");
            }

            ui.separator();
            ui.heading("Log");
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(&self.log);
            });
        });

        // Request repaint
        ctx.request_repaint();
    }
}

// In the run_program function, send log messages progressively
fn run_program(
    config: Config,
    sender: mpsc::Sender<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    sender.send("Starting the program.\n".to_string()).ok();

    // Record the start time of the entire program
    let total_start_time = Instant::now();

    // Open the file to save primes (create new/overwrite mode) with a large buffer size
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open("primes.txt")
        .map_err(|e| format!("Failed to open primes.txt: {}", e))?;

    let mut writer = BufWriter::with_capacity(config.writer_buffer_size, file);

    // Generate the first prime_cache_size primes
    let small_primes = generate_small_primes(config.prime_cache_size)?;
    sender
        .send(format!(
            "Generated the first {} primes.\n",
            small_primes.len()
        ))
        .ok();

    // Count the total number of primes found
    let mut total_primes_found = 0;

    // Write the initial small primes to the file in bulk
    for &prime in &small_primes {
        if prime >= config.prime_min {
            writeln!(writer, "{}", prime)
                .map_err(|e| format!("Failed to write to file: {}", e))?;
            total_primes_found += 1;
        }
    }

    let mut low = std::cmp::max(
        config.prime_min,
        small_primes.last().cloned().unwrap_or(2) + 1,
    );

    while low <= config.prime_max {
        let high = low
            .saturating_add(config.segment_size - 1)
            .min(config.prime_max);

        sender
            .send(format!("Processing segment: {} - {}\n", low, high))
            .ok();

        let segment_start_time = Instant::now();

        // Perform parallel processing within the segment
        let segment_primes =
            segmented_sieve_parallel(&small_primes, low, high, config.chunk_size)?;

        if !segment_primes.is_empty() {
            // Write to the file in the main thread in bulk
            for &prime in &segment_primes {
                if prime >= config.prime_min && prime <= config.prime_max {
                    writeln!(writer, "{}", prime)
                        .map_err(|e| format!("Failed to write to file: {}", e))?;
                    total_primes_found += 1;
                }
            }

            sender
                .send(format!(
                    "Found {} primes in this segment.\n",
                    segment_primes.len()
                ))
                .ok();
        }

        let segment_duration = segment_start_time.elapsed();
        sender
            .send(format!(
                "Segment completed: {} - {} (Duration: {:.2?})\n",
                low, high, segment_duration
            ))
            .ok();

        // Move to the next segment
        low = high + 1;
    }

    // Flush the buffer
    writer
        .flush()
        .map_err(|e| format!("Failed to flush buffer: {}", e))?;

    // Record the end time of the entire program
    let total_duration = total_start_time.elapsed();
    sender
        .send(format!("Total computation time: {:.2?}\n", total_duration))
        .ok();

    // Display the total number of primes found
    sender
        .send(format!("Total number of primes found: {}\n", total_primes_found))
        .ok();

    Ok(())
}

// Other functions (load_or_create_config, save_config, generate_small_primes, estimate_sieve_size, segmented_sieve_parallel) remain unchanged.

fn load_or_create_config() -> Result<Config, Box<dyn std::error::Error>> {
    if Path::new(SETTINGS_FILE).exists() {
        // Load the settings if the file exists
        let mut file = File::open(SETTINGS_FILE)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let config = toml::from_str(&contents)
            .map_err(|e| format!("Failed to parse the settings file: {}", e))?;

        Ok(config)
    } else {
        // Create default settings and save them if the file doesn't exist
        let config = Config::default();
        save_config(&config)?;
        Ok(config)
    }
}

fn save_config(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let toml_str = toml::to_string(config)?;
    let file = File::create(SETTINGS_FILE)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(toml_str.as_bytes())?;
    Ok(())
}

fn generate_small_primes(n: usize) -> Result<Vec<u64>, Box<dyn std::error::Error>> {
    let sieve_size = estimate_sieve_size(n);
    let mut is_prime = bitvec![1; sieve_size];
    let mut primes = Vec::with_capacity(n);

    is_prime.set(0, false);
    is_prime.set(1, false);

    for num in 2..sieve_size {
        if is_prime[num] {
            primes.push(num as u64);
            if primes.len() >= n {
                break;
            }
            let start = num
                .checked_mul(num)
                .ok_or("Integer overflow occurred")?;
            for multiple in (start..sieve_size).step_by(num) {
                is_prime.set(multiple, false);
            }
        }
    }

    Ok(primes)
}

fn estimate_sieve_size(n: usize) -> usize {
    if n < 6 {
        return 15;
    }
    let n_f64 = n as f64;
    let approx_nth_prime = (n_f64 * (n_f64.ln() + n_f64.ln().ln())).ceil() as usize;
    approx_nth_prime + 10
}

fn segmented_sieve_parallel(
    small_primes: &[u64],
    low: u64,
    high: u64,
    chunk_size: usize,
) -> Result<Vec<u64>, Box<dyn std::error::Error>> {
    let size = (high - low + 1) as usize;
    let mut is_prime = bitvec![1; size];

    // Divide the segment into chunks and process each chunk in parallel
    is_prime
        .chunks_mut(chunk_size)
        .enumerate()
        .par_bridge()
        .for_each(|(i, chunk)| {
            let chunk_low = low + (i as u64) * chunk_size as u64;
            let chunk_high = chunk_low + chunk.len() as u64 - 1;

            for &prime in small_primes {
                let prime_square = match prime.checked_mul(prime) {
                    Some(val) => val,
                    None => continue, // Skip if overflow occurs
                };
                if prime_square > chunk_high {
                    break;
                }

                // Calculate the starting position
                let mut start = if chunk_low % prime == 0 {
                    chunk_low
                } else {
                    chunk_low + (prime - (chunk_low % prime))
                };
                if start < prime_square {
                    start = prime_square;
                }

                while start <= chunk_high {
                    let chunk_index = (start - chunk_low) as usize;
                    if chunk_index < chunk.len() {
                        chunk.set(chunk_index, false);
                    }
                    match start.checked_add(prime) {
                        Some(val) => start = val,
                        None => break, // Break the loop if overflow occurs
                    }
                }
            }
        });

    let mut primes = Vec::new();
    for i in 0..size {
        if is_prime[i] {
            primes.push(low + i as u64);
        }
    }

    Ok(primes)
}
