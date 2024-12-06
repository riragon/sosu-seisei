use eframe::{egui, App};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Instant;

use num_bigint::BigUint;
use num_traits::{One, ToPrimitive};
use miller_rabin::is_prime;
use sysinfo::{System, SystemExt, CpuExt};

fn main() {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Sosu-Seisei Settings",
        options,
        Box::new(|cc| Box::new(MyApp::new(cc))),
    );
}

const SETTINGS_FILE: &str = "settings.txt";

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Config {
    segment_size: u64,
    chunk_size: usize,
    writer_buffer_size: usize,
    prime_min: String,
    prime_max: String,
    miller_rabin_rounds: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
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

#[derive(Serialize, Deserialize, Debug, Clone)]
enum WorkerMessage {
    Log(String),
    Progress { current: u64, total: u64 },
    Eta(String),
    CpuMemUsage { cpu_percent: f32, mem_usage: u64 },
    HistogramUpdate {
        histogram: Vec<(u64, u64)>,
        cumulative: Vec<(u64, u64)>,
        density: Vec<(u64, f64)>,
    },
    FoundPrimeIndex(u64, u64),
    Done,
    Stopped,
}

struct MyApp {
    config: Config,
    is_running: bool,
    log: String,
    receiver: Option<mpsc::Receiver<WorkerMessage>>,

    prime_min_input_old: String,
    prime_max_input_old: String,
    prime_min_input_new: String,
    prime_max_input_new: String,
    miller_rabin_rounds_input: String,

    progress: f32,
    eta: String,
    cpu_usage: f32,
    mem_usage: u64,

    histogram_data: Vec<(u64, u64)>,
    cumulative_data: Vec<(u64, u64)>,
    density_data: Vec<(u64, f64)>,

    stop_flag: Arc<AtomicBool>,

    recent_primes: Vec<u64>,
    max_recent_primes: usize,

    scatter_data: Vec<[f64;2]>,
    found_count: u64,

    current_processed: u64,
    total_range: u64,
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

            progress: 0.0,
            eta: "N/A".to_string(),
            cpu_usage: 0.0,
            mem_usage: 0,

            histogram_data: Vec::new(),
            cumulative_data: Vec::new(),
            density_data: Vec::new(),

            stop_flag: Arc::new(AtomicBool::new(false)),

            recent_primes: Vec::new(),
            max_recent_primes: 100,

            scatter_data: Vec::new(),
            found_count: 0,

            current_processed: 0,
            total_range: 0,
        }
    }
}

impl App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(ref receiver) = self.receiver {
            let mut remove_receiver = false;
            while let Ok(message) = receiver.try_recv() {
                match message {
                    WorkerMessage::Log(msg) => {
                        self.log.push_str(&msg);
                        if !msg.ends_with('\n') {
                            self.log.push('\n');
                        }
                    }
                    WorkerMessage::Progress { current, total } => {
                        self.progress = current as f32 / total as f32;
                        self.current_processed = current;
                        self.total_range = total;
                    }
                    WorkerMessage::Eta(eta_str) => {
                        self.eta = eta_str;
                    }
                    WorkerMessage::CpuMemUsage { cpu_percent, mem_usage } => {
                        self.cpu_usage = cpu_percent;
                        self.mem_usage = mem_usage;
                    }
                    WorkerMessage::HistogramUpdate { histogram, cumulative, density } => {
                        self.histogram_data = histogram;
                        self.cumulative_data = cumulative;
                        self.density_data = density;
                    }
                    WorkerMessage::FoundPrimeIndex(pr, idx) => {
                        self.recent_primes.push(pr);
                        if self.recent_primes.len() > self.max_recent_primes {
                            self.recent_primes.remove(0);
                        }
                        self.scatter_data.push([pr as f64, idx as f64]);
                    }
                    WorkerMessage::Done => {
                        self.is_running = false;
                        remove_receiver = true;
                    }
                    WorkerMessage::Stopped => {
                        self.is_running = false;
                        remove_receiver = true;
                        self.log.push_str("Process stopped by user.\n");
                    }
                }
            }
            if remove_receiver {
                self.receiver = None;
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Sosu-Seisei Settings");
            ui.separator();

            // 上段2カラム: Old / New
            ui.columns(2, |top_columns| {
                let left = &mut top_columns[0];
                left.heading("Old Method (Sieve)");
                left.separator();
                left.label("prime_min (u64):");
                left.text_edit_singleline(&mut self.prime_min_input_old);
                left.label("prime_max (u64):");
                left.text_edit_singleline(&mut self.prime_max_input_old);

                if left.button("Run (Old Method)").clicked() && !self.is_running {
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
                        self.log.clear();
                        self.config.prime_min = self.prime_min_input_old.clone();
                        self.config.prime_max = self.prime_max_input_old.clone();
                        if let Err(e) = save_config(&self.config) {
                            self.log.push_str(&format!("Failed to save settings: {}\n", e));
                        }

                        self.is_running = true;
                        self.progress = 0.0;
                        self.eta = "Calculating...".to_string();
                        self.histogram_data.clear();
                        self.cumulative_data.clear();
                        self.density_data.clear();
                        self.stop_flag.store(false, Ordering::SeqCst);
                        self.recent_primes.clear();
                        self.scatter_data.clear();
                        self.found_count = 0;

                        let config = self.config.clone();
                        let (sender, receiver) = mpsc::channel();
                        self.receiver = Some(receiver);
                        let stop_flag = self.stop_flag.clone();

                        thread::spawn(move || {
                            let monitor_handle = start_resource_monitor(sender.clone());
                            if let Err(e) = run_program_old(config, sender.clone(), stop_flag) {
                                let _ = sender.send(WorkerMessage::Log(format!("An error occurred: {}\n", e)));
                            }
                            let _ = sender.send(WorkerMessage::Done);
                            drop(monitor_handle);
                        });

                    } else {
                        for error in errors {
                            self.log.push_str(&format!("{}\n", error));
                        }
                    }
                }

                let right = &mut top_columns[1];
                right.heading("New Method (Miller-Rabin)");
                right.separator();
                right.label("prime_min (BigInt):");
                right.text_edit_singleline(&mut self.prime_min_input_new);

                right.label("prime_max (BigInt):");
                right.text_edit_singleline(&mut self.prime_max_input_new);

                right.label("Miller-Rabin rounds:");
                right.text_edit_singleline(&mut self.miller_rabin_rounds_input);

                if right.button("Run (Miller-Rabin)").clicked() && !self.is_running {
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
                        self.log.clear();
                        self.config.prime_min = self.prime_min_input_new.clone();
                        self.config.prime_max = self.prime_max_input_new.clone();
                        self.config.miller_rabin_rounds = mr_rounds;
                        if let Err(e) = save_config(&self.config) {
                            eprintln!("Failed to save settings: {}", e);
                        }

                        self.is_running = true;
                        self.progress = 0.0;
                        self.eta = "Calculating...".to_string();
                        self.histogram_data.clear();
                        self.cumulative_data.clear();
                        self.density_data.clear();
                        self.stop_flag.store(false, Ordering::SeqCst);
                        self.recent_primes.clear();
                        self.scatter_data.clear();
                        self.found_count = 0;

                        let config = self.config.clone();
                        let (sender, receiver) = mpsc::channel();
                        self.receiver = Some(receiver);
                        let stop_flag = self.stop_flag.clone();

                        thread::spawn(move || {
                            let monitor_handle = start_resource_monitor(sender.clone());
                            if let Err(e) = run_program_new(config, sender.clone(), stop_flag) {
                                let _ = sender.send(WorkerMessage::Log(format!("An error occurred: {}\n", e)));
                            }
                            let _ = sender.send(WorkerMessage::Done);
                            drop(monitor_handle);
                        });

                    } else {
                        for error in errors {
                            self.log.push_str(&format!("{}\n", error));
                        }
                    }
                }
            });

            ui.separator();

            // 下段2カラム: 左=Histogramのみチャート, 右=Recent PrimesとCumulative/Density/Scatterを数値表示
            ui.columns(2, |bottom_columns| {
                let left = &mut bottom_columns[0];
                left.heading("Analysis Charts");
                // Histogramはチャート表示のまま
                left.label("Prime Distribution (Histogram)");
                {
                    use egui::plot::{Plot, BarChart, Bar};
                    let bars: Vec<Bar> = self.histogram_data.iter().map(|(x, c)| {
                        Bar::new(*x as f64, *c as f64)
                    }).collect();
                    let chart = BarChart::new(bars).width(1.0);
                    Plot::new("Histogram").show(left, |plot_ui| {
                        plot_ui.bar_chart(chart);
                    });
                }

                let right = &mut bottom_columns[1];

                // Recent PrimesとCumulative/Density/Scatter数値表示を横並びに
                // columns(2, ...)で左にRecent Primes、右にCumulative/Density/Scatter
                right.columns(2, |cols| {
                    let rp_col = &mut cols[0];
                    rp_col.heading("Recent Primes (Latest)");
                    {
                        let show_count = 10.min(self.recent_primes.len());
                        if show_count > 0 {
                            for &prime in self.recent_primes.iter().rev().take(show_count) {
                                rp_col.label(format!("{}", prime));
                            }
                        } else {
                            rp_col.label("No primes yet");
                        }
                    }

                    let cd_col = &mut cols[1];
                    cd_col.heading("Cumulative / Density / Scatter (Numbers)");
                    
                    // Cumulative
                    cd_col.label("Cumulative:");
                    {
                        let show_count = 5.min(self.cumulative_data.len());
                        if show_count > 0 {
                            for &(x, count) in self.cumulative_data.iter().rev().take(show_count) {
                                cd_col.label(format!("At {}: {}", x, count));
                            }
                        } else {
                            cd_col.label("No data yet");
                        }
                    }

                    cd_col.separator();
                    cd_col.label("Density:");
                    {
                        let show_count = 5.min(self.density_data.len());
                        if show_count > 0 {
                            for &(x, d) in self.density_data.iter().rev().take(show_count) {
                                cd_col.label(format!("At {}: density={:.4}", x, d));
                            }
                        } else {
                            cd_col.label("No data yet");
                        }
                    }

                    cd_col.separator();
                    cd_col.label("Scatter (prime,index):");
                    {
                        let show_count = 5.min(self.scatter_data.len());
                        if show_count > 0 {
                            for &[prime, idx] in self.scatter_data.iter().rev().take(show_count) {
                                cd_col.label(format!("{} at idx {}", prime as u64, idx as u64));
                            }
                        } else {
                            cd_col.label("No data yet");
                        }
                    }
                });

                right.separator();
                right.heading("Log");
                {
                    let lines: Vec<&str> = self.log.lines().collect();
                    let show_count = 10.min(lines.len());
                    if show_count > 0 {
                        for &line in lines.iter().rev().take(show_count) {
                            right.label(line);
                        }
                    } else {
                        right.label("No logs yet");
                    }
                }

                right.separator();
                right.heading("Progress / System");
                right.add(egui::ProgressBar::new(self.progress).show_percentage());
                if self.total_range > 0 {
                    right.label(format!("Processed: {}/{}", self.current_processed, self.total_range));
                } else {
                    right.label("Processed: N/A");
                }
                right.label(format!("ETA: {}", self.eta));
                right.separator();
                right.label(format!("CPU Usage: {:.2}%", self.cpu_usage));
                right.label(format!("Memory Usage: {} KB", self.mem_usage));

                right.separator();
                right.heading("Controls");
                if self.is_running {
                    if right.button("STOP").clicked() {
                        self.stop_flag.store(true, Ordering::SeqCst);
                    }
                }
            });
        });

        ctx.request_repaint();
    }
}

fn start_resource_monitor(sender: mpsc::Sender<WorkerMessage>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut sys = System::new_all();
        loop {
            sys.refresh_all();
            let cpu_percent = sys.global_cpu_info().cpu_usage();
            let mem_usage = sys.used_memory();
            if sender.send(WorkerMessage::CpuMemUsage { cpu_percent, mem_usage }).is_err() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    })
}

fn run_program_old(config: Config, sender: mpsc::Sender<WorkerMessage>, stop_flag: Arc<AtomicBool>) -> Result<(), Box<dyn std::error::Error>> {
    sender.send(WorkerMessage::Log("Running old method (Sieve)".to_string())).ok();

    let prime_min = config.prime_min.parse::<u64>()?;
    let prime_max = config.prime_max.parse::<u64>()?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open("primes.txt")?;
    let mut writer = BufWriter::with_capacity(config.writer_buffer_size, file);

    let prime_cache_size = (prime_max as f64).sqrt() as u64 + 1;
    let small_primes = simple_sieve(prime_cache_size);

    let segment_size = config.segment_size as usize;
    let mut low = prime_min;
    let mut total_found = 0u64;

    let total_range = prime_max - prime_min + 1;
    let mut processed = 0u64; 
    let start_time = Instant::now();

    let mut histogram_data = Vec::new();
    let mut cumulative_data = Vec::new();
    let mut density_data = Vec::new();

    let histogram_interval = 10_000_000u64;
    let mut next_histogram_mark = prime_min + histogram_interval;
    let mut current_interval_count = 0u64;

    let mut found_count = 0u64;

    while low <= prime_max {
        if stop_flag.load(Ordering::SeqCst) {
            sender.send(WorkerMessage::Stopped).ok();
            return Ok(());
        }

        let high = if low + segment_size as u64 - 1 < prime_max {
            low + segment_size as u64 - 1
        } else {
            prime_max
        };

        let primes_in_segment = segmented_sieve(&small_primes, low, high);
        for &p in &primes_in_segment {
            writeln!(writer, "{}", p)?;
            total_found += 1;
            current_interval_count += 1;
            found_count += 1;
            sender.send(WorkerMessage::FoundPrimeIndex(p, found_count)).ok();
        }

        processed += high - low + 1;
        let progress = processed as f64 / total_range as f64;

        let elapsed = start_time.elapsed().as_secs_f64();
        let eta = if progress > 0.0 {
            let total_time = elapsed / progress;
            let remaining = total_time - elapsed;
            format!("ETA: {:.2} sec", remaining)
        } else {
            "Calculating...".to_string()
        };

        sender.send(WorkerMessage::Progress {
            current: processed,
            total: total_range,
        }).ok();
        sender.send(WorkerMessage::Eta(eta)).ok();

        cumulative_data.push((high, total_found));

        if high >= next_histogram_mark || high == prime_max {
            histogram_data.push((next_histogram_mark, current_interval_count));
            let density = current_interval_count as f64 / (histogram_interval as f64);
            density_data.push((next_histogram_mark, density));
            current_interval_count = 0;
            next_histogram_mark = high + histogram_interval;

            sender.send(WorkerMessage::HistogramUpdate {
                histogram: histogram_data.clone(),
                cumulative: cumulative_data.clone(),
                density: density_data.clone(),
            }).ok();
        }

        low = high + 1;
    }

    writer.flush()?;
    sender.send(WorkerMessage::Log(format!("Finished old method. Total primes found: {}", total_found))).ok();
    Ok(())
}

fn run_program_new(config: Config, sender: mpsc::Sender<WorkerMessage>, stop_flag: Arc<AtomicBool>) -> Result<(), Box<dyn std::error::Error>> {
    sender.send(WorkerMessage::Log("Running new method (Miller-Rabin)".to_string())).ok();

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

    let maybe_range = (&prime_max - &prime_min).to_f64();
    let start_time = Instant::now();
    let mut processed_f64 = 0.0;

    let mut histogram_data = Vec::new();
    let mut cumulative_data = Vec::new();
    let mut density_data = Vec::new();

    let histogram_interval = BigUint::from(10_000_u64);
    let interval_float = 10_000_f64;
    let mut next_histogram_mark = &prime_min + &histogram_interval;
    let mut current_interval_count = 0u64;

    let mut found_count = 0u64;

    while &current <= &prime_max {
        if stop_flag.load(Ordering::SeqCst) {
            sender.send(WorkerMessage::Stopped).ok();
            return Ok(());
        }

        if is_prime(&current, config.miller_rabin_rounds as usize) {
            writeln!(writer, "{}", current)?;
            total_found += 1;
            current_interval_count += 1;
            found_count += 1;

            let current_u64 = (&current).to_u64_digits().get(0).copied().unwrap_or(0);
            sender.send(WorkerMessage::FoundPrimeIndex(current_u64, found_count)).ok();
        }

        if let Some(c_f64) = (&current - &prime_min).to_f64() {
            processed_f64 = c_f64;
        }

        if let Some(range_f64) = maybe_range {
            let progress = processed_f64 / range_f64;
            let elapsed = start_time.elapsed().as_secs_f64();
            let eta = if progress > 0.0 {
                let total_time = elapsed / progress;
                let remaining = total_time - elapsed;
                format!("ETA: {:.2} sec", remaining)
            } else {
                "Calculating...".to_string()
            };

            sender.send(WorkerMessage::Progress {
                current: processed_f64 as u64,
                total: range_f64 as u64,
            }).ok();
            sender.send(WorkerMessage::Eta(eta)).ok();

            let current_u64 = (&current).to_u64_digits().get(0).copied().unwrap_or(0);
            cumulative_data.push((current_u64, total_found));

            if &current >= &next_histogram_mark || &current == &prime_max {
                let mark_u64 = next_histogram_mark.to_u64_digits().get(0).copied().unwrap_or(0);
                histogram_data.push((mark_u64, current_interval_count));
                let density = current_interval_count as f64 / interval_float;
                density_data.push((mark_u64, density));
                current_interval_count = 0;
                next_histogram_mark = &next_histogram_mark + &histogram_interval;

                sender.send(WorkerMessage::HistogramUpdate {
                    histogram: histogram_data.clone(),
                    cumulative: cumulative_data.clone(),
                    density: density_data.clone(),
                }).ok();
            }
        }

        current = &current + &one;
    }

    writer.flush()?;
    sender.send(WorkerMessage::Log(format!("Finished new method. Total primes found: {}", total_found))).ok();
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
