use eframe::{egui, App};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use std::sync::mpsc;
use std::thread;

use num_bigint::BigUint;
use num_traits::{One, FromPrimitive, ToPrimitive};
use miller_rabin::is_prime;

const SETTINGS_FILE: &str = "settings.txt";

#[derive(Serialize, Deserialize, Debug, Clone)]
enum PrimalityMethod {
    MillerRabin,
    ECPP,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Config {
    // Old method (Sieve) settings (remain in struct and settings.txt, but not shown in GUI)
    prime_cache_size: usize,
    segment_size: u64,
    chunk_size: usize,
    writer_buffer_size: usize,

    prime_min: String,
    prime_max: String,

    // New method (Miller-Rabin) settings
    miller_rabin_rounds: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            prime_cache_size: 100_000,
            segment_size: 10_000_000,
            chunk_size: 16_384,
            writer_buffer_size: 8 * 1024 * 1024,
            prime_min: "1".to_string(),
            prime_max: "1000000".to_string(),
            miller_rabin_rounds: 64,
        }
    }
}

fn load_or_create_config() -> Result<Config, Box<dyn std::error::Error>> {
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

fn save_config(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let toml_str = toml::to_string(config)?;
    let file = File::create(SETTINGS_FILE)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(toml_str.as_bytes())?;
    Ok(())
}

fn main() {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Sosu-Seisei Settings",
        options,
        Box::new(|cc| Box::new(MyApp::new(cc))),
    );
}

struct MyApp {
    config: Config,
    is_running: bool,
    log: String,
    receiver: Option<mpsc::Receiver<String>>,

    prime_min_input_old: String,
    prime_max_input_old: String,
    prime_min_input_new: String,
    prime_max_input_new: String,
    miller_rabin_rounds_input: String,
}

impl MyApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let config = load_or_create_config().unwrap_or_default();
        MyApp {
            prime_min_input_old: config.prime_min.clone(),
            prime_max_input_old: config.prime_max.clone(),
            prime_min_input_new: config.prime_min.clone(),
            prime_max_input_new: config.prime_max.clone(),
            miller_rabin_rounds_input: config.miller_rabin_rounds.to_string(),
            config,
            is_running: false,
            log: String::new(),
            receiver: None,
        }
    }
}

impl App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
            ui.heading("Sosu-Seisei Settings");

            ui.columns(2, |columns| {
                // Left column: Old method (Sieve)
                columns[0].heading("Old Method (Sieve)");
                // Removed prime_cache_size, segment_size, chunk_size, writer_buffer_size from GUI.

                columns[0].separator();
                columns[0].label("prime_min (Recommended within 64-bit range):");
                columns[0].text_edit_singleline(&mut self.prime_min_input_old);
                columns[0].label("prime_max (Recommended within 64-bit range):");
                columns[0].text_edit_singleline(&mut self.prime_max_input_old);

                if columns[0].button("Run (Old Method)").clicked() && !self.is_running {
                    let mut errors = Vec::new();

                    let prime_min = match self.prime_min_input_old.trim().parse::<u64>() {
                        Ok(v) => v,
                        Err(_) => {
                            errors.push("prime_min (old) is not a valid u64 integer.");
                            1
                        }
                    };

                    let prime_max = match self.prime_max_input_old.trim().parse::<u64>() {
                        Ok(v) => v,
                        Err(_) => {
                            errors.push("prime_max (old) is not a valid u64 integer.");
                            10_000_000_000
                        }
                    };

                    if prime_min >= prime_max {
                        errors.push("prime_min must be less than prime_max (old).");
                    }

                    if errors.is_empty() {
                        self.config.prime_min = self.prime_min_input_old.clone();
                        self.config.prime_max = self.prime_max_input_old.clone();
                        if let Err(e) = save_config(&self.config) {
                            self.log.push_str(&format!("Failed to save settings: {}\n", e));
                        }

                        self.is_running = true;
                        let config = self.config.clone();
                        let (sender, receiver) = mpsc::channel();
                        self.receiver = Some(receiver);

                        thread::spawn(move || {
                            if let Err(e) = run_program_old(config, sender.clone()) {
                                let _ = sender.send(format!("An error occurred: {}\n", e));
                            }
                            let _ = sender.send("done".to_string());
                        });

                    } else {
                        for error in errors {
                            self.log.push_str(&format!("{}\n", error));
                        }
                    }
                }

                // Right column: New method (Miller-Rabin)
                columns[1].heading("New Method (Miller-Rabin)");
                columns[1].label("prime_min (BigInt allowed):");
                columns[1].text_edit_singleline(&mut self.prime_min_input_new);

                columns[1].label("prime_max (BigInt allowed):");
                columns[1].text_edit_singleline(&mut self.prime_max_input_new);

                columns[1].label("Miller-Rabin rounds:");
                columns[1].text_edit_singleline(&mut self.miller_rabin_rounds_input);

                if columns[1].button("Run (Miller-Rabin)").clicked() && !self.is_running {
                    let mut errors = Vec::new();

                    let prime_min_bi = match self.prime_min_input_new.trim().parse::<BigUint>() {
                        Ok(v) => v,
                        Err(_) => {
                            errors.push("Invalid prime_min (new). Must be a positive integer.");
                            BigUint::one()
                        }
                    };

                    let prime_max_bi = match self.prime_max_input_new.trim().parse::<BigUint>() {
                        Ok(v) => v,
                        Err(_) => {
                            errors.push("Invalid prime_max (new). Must be a positive integer.");
                            BigUint::one()
                        }
                    };

                    if &prime_min_bi >= &prime_max_bi {
                        errors.push("prime_min must be less than prime_max (new).");
                    }

                    let mr_rounds = match self.miller_rabin_rounds_input.trim().parse::<u64>() {
                        Ok(v) => v,
                        Err(_) => {
                            errors.push("Invalid Miller-Rabin rounds (new).");
                            64
                        }
                    };

                    if errors.is_empty() {
                        self.config.prime_min = self.prime_min_input_new.clone();
                        self.config.prime_max = self.prime_max_input_new.clone();
                        self.config.miller_rabin_rounds = mr_rounds;
                        if let Err(e) = save_config(&self.config) {
                            eprintln!("Failed to save settings: {}", e);
                        }

                        self.is_running = true;
                        let config = self.config.clone();
                        let (sender, receiver) = mpsc::channel();
                        self.receiver = Some(receiver);

                        thread::spawn(move || {
                            if let Err(e) = run_program_new(config, sender.clone()) {
                                let _ = sender.send(format!("An error occurred: {}\n", e));
                            }
                            let _ = sender.send("done".to_string());
                        });

                    } else {
                        for error in errors {
                            self.log.push_str(&format!("{}\n", error));
                        }
                    }
                }
            });

            ui.separator();
            ui.heading("Log");
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(&self.log);
            });
        });

        ctx.request_repaint();
    }
}

// 以下は前回の例と同様の実装
fn run_program_old(config: Config, sender: mpsc::Sender<String>) -> Result<(), Box<dyn std::error::Error>> {
    sender.send("Running old method (Sieve)\n".to_string()).ok();

    let prime_min = config.prime_min.parse::<u64>()?;
    let prime_max = config.prime_max.parse::<u64>()?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open("primes.txt")?;
    let mut writer = BufWriter::with_capacity(config.writer_buffer_size, file);

    let limit = (prime_max as f64).sqrt() as u64 + 1;
    let small_primes = simple_sieve(limit);

    let segment_size = config.segment_size as usize;
    let mut low = prime_min;
    let mut total_found = 0u64;

    while low <= prime_max {
        let high = if low + segment_size as u64 - 1 < prime_max {
            low + segment_size as u64 - 1
        } else {
            prime_max
        };

        let primes_in_segment = segmented_sieve(&small_primes, low, high);
        for &p in &primes_in_segment {
            writeln!(writer, "{}", p)?;
            total_found += 1;
        }

        low = high + 1;
    }

    writer.flush()?;
    sender.send(format!("Finished old method. Total primes found: {}\n", total_found)).ok();
    Ok(())
}

fn run_program_new(config: Config, sender: mpsc::Sender<String>) -> Result<(), Box<dyn std::error::Error>> {
    sender.send("Running new method (Miller-Rabin)\n".to_string()).ok();

    let prime_min = config.prime_min.parse::<BigUint>()?;
    let prime_max = config.prime_max.parse::<BigUint>()?;

    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open("primes.txt")?;
    let mut writer = BufWriter::with_capacity(config.writer_buffer_size, file);

    let mut current = prime_min.clone();
    let one = BigUint::one();
    let mut total_found = 0u64;

    while &current <= &prime_max {
        if is_prime(&current, config.miller_rabin_rounds as usize) {
            writeln!(writer, "{}", current)?;
            total_found += 1;
        }
        current = &current + &one;
    }

    writer.flush()?;
    sender.send(format!("Finished new method. Total primes found: {}\n", total_found)).ok();
    Ok(())
}

fn simple_sieve(limit: u64) -> Vec<u64> {
    let mut is_prime = vec![true; (limit as usize) + 1];
    is_prime[0] = false;
    if limit >= 1 {
        is_prime[1] = false;
    }
    for i in 2..=((limit as f64).sqrt() as usize) {
        if is_prime[i] {
            for j in ((i*i)..=limit as usize).step_by(i) {
                is_prime[j] = false;
            }
        }
    }
    let mut primes = Vec::new();
    for i in 2..=limit as usize {
        if is_prime[i] {
            primes.push(i as u64);
        }
    }
    primes
}

fn segmented_sieve(small_primes: &[u64], low: u64, high: u64) -> Vec<u64> {
    let size = (high - low + 1) as usize;
    let mut is_prime = vec![true; size];
    if low == 0 {
        if size > 0 {
            is_prime[0] = false;
        }
        if size > 1 {
            is_prime[1] = false;
        }
    } else if low == 1 {
        is_prime[0] = false;
    }

    for &p in small_primes {
        if p*p > high {
            break;
        }
        let mut start = if low % p == 0 { low } else { low + (p - (low % p)) };
        if start < p*p {
            start = p*p;
        }

        let mut j = start;
        while j <= high {
            is_prime[(j - low) as usize] = false;
            j += p;
        }
    }

    let mut primes = Vec::new();
    for i in 0..size {
        if is_prime[i] {
            primes.push(low + i as u64);
        }
    }
    primes
}
