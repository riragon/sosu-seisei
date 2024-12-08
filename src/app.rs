use crate::config::{Config, load_or_create_config, save_config};
use eframe::{egui, App};
use std::sync::{mpsc, Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::HashMap;
use num_bigint::BigUint;
use num_traits::One;
use std::thread;
use crate::verification::verify_primes_bpsw_all_composites;
use crate::sieve::run_program_old;
use crate::miller_rabin::run_program_new;
use sysinfo::{System, SystemExt, CpuExt};

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum WorkerMessage {
    Log(String),
    Progress { current: u64, total: u64 },
    Eta(String),
    CpuMemUsage { cpu_percent: f32, mem_usage: u64 },
    HistogramUpdate {
        histogram: Vec<(u64, u64)>,
    },
    FoundPrimeIndex(u64, u64),
    Done,
    Stopped,
    VerificationDone(String),
}

pub struct MyApp {
    pub config: Config,
    pub is_running: bool,
    pub log: String,
    pub receiver: Option<mpsc::Receiver<WorkerMessage>>,

    pub prime_min_input_old: String,
    pub prime_max_input_old: String,
    pub prime_min_input_new: String,
    pub prime_max_input_new: String,
    pub miller_rabin_rounds_input: String,

    pub progress: f32,
    pub eta: String,
    pub cpu_usage: f32,
    pub mem_usage: u64,

    pub histogram_data: Vec<(u64, u64)>,

    pub stop_flag: Arc<AtomicBool>,

    pub recent_primes: Vec<u64>,
    pub max_recent_primes: usize,

    pub scatter_data: Vec<[f64;2]>,
    pub found_count: u64,

    pub current_processed: u64,
    pub total_range: u64,

    pub final_digit_counts: [u64; 10],
    pub total_found_primes: u64,

    pub last_prime: Option<u64>,
    pub gap_counts: HashMap<u64, u64>,
    pub gap_max: u64,
    pub gap_sum: u64,
    pub gap_count: u64,

    pub too_large_value: bool,

    pub is_verifying: bool,

    pub start_time: std::time::Instant,

    // 総メモリ量を保持するフィールドを追加
    pub total_mem: u64,
}

impl MyApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let config = load_or_create_config().unwrap_or_default();

        // システム情報から総メモリ量を取得
        let mut sys = System::new_all();
        sys.refresh_all();
        let total_mem = sys.total_memory(); // KB単位

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

            stop_flag: Arc::new(AtomicBool::new(false)),

            recent_primes: Vec::new(),
            max_recent_primes: 100,

            scatter_data: Vec::new(),
            found_count: 0,
            current_processed: 0,
            total_range: 0,

            final_digit_counts: [0;10],
            total_found_primes: 0,

            last_prime: None,
            gap_counts: HashMap::new(),
            gap_max: 0,
            gap_sum: 0,
            gap_count: 0,

            too_large_value: false,

            is_verifying: false,

            start_time: std::time::Instant::now(),
            total_mem,
        }
    }

    pub fn start_verification(&mut self) {
        self.log.push_str("Starting prime verification (Baillie-PSW)...\n");
        let (sender, receiver) = mpsc::channel::<WorkerMessage>();
        let stop_flag = self.stop_flag.clone();

        thread::spawn(move || {
            if let Err(e) = verify_primes_bpsw_all_composites(sender.clone(), stop_flag) {
                let _ = sender.send(WorkerMessage::Log(format!("Verification error: {}", e)));
                let _ = sender.send(WorkerMessage::VerificationDone("Error occurred".to_string()));
            }
        });

        self.receiver = Some(receiver);
        self.is_verifying = true;
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
                        let p = current as f32 / total as f32;
                        self.progress = p;
                    }
                    WorkerMessage::Eta(eta_str)=> {
                        self.eta=eta_str;
                    }
                    WorkerMessage::CpuMemUsage { cpu_percent, mem_usage }=> {
                        self.cpu_usage=cpu_percent;
                        self.mem_usage=mem_usage;
                    }
                    WorkerMessage::HistogramUpdate { histogram }=> {
                        if !histogram.is_empty() {
                            self.histogram_data.extend_from_slice(&histogram);
                        }
                    }
                    WorkerMessage::FoundPrimeIndex(pr,idx)=> {
                        self.recent_primes.push(pr);
                        if self.recent_primes.len()>self.max_recent_primes {
                            self.recent_primes.remove(0);
                        }
                        self.scatter_data.push([pr as f64, idx as f64]);

                        let last_digit=(pr%10) as usize;
                        self.final_digit_counts[last_digit]+=1;
                        self.total_found_primes+=1;

                        if let Some(lp)=self.last_prime {
                            let gap = pr.saturating_sub(lp);
                            *self.gap_counts.entry(gap).or_insert(0)+=1;
                            if gap>self.gap_max{
                                self.gap_max=gap;
                            }
                            self.gap_sum+=gap;
                            self.gap_count+=1;
                        }
                        self.last_prime=Some(pr);
                    }
                    WorkerMessage::Done=> {
                        self.is_running=false;
                        remove_receiver=true;
                    }
                    WorkerMessage::Stopped=> {
                        self.is_running=false;
                        remove_receiver=true;
                        self.log.push_str("Process stopped by user.\n");
                    }
                    WorkerMessage::VerificationDone(msg)=> {
                        self.log.push_str(&format!("Verification: {}\n",msg));
                        self.is_verifying=false;
                        remove_receiver=true;
                    }
                }
            }
            if remove_receiver {
                self.receiver=None;
            }
        }

        egui::CentralPanel::default().show(ctx,|ui| {
            egui::ScrollArea::vertical().show(ui,|ui| {
                ui.heading("Sosu-Seisei Settings");
                ui.separator();

                ui.columns(2, |columns| {
                    let (left_cols, right_cols) = columns.split_at_mut(1);
                    let left = &mut left_cols[0];
                    let right = &mut right_cols[0];

                    // 左カラム
                    left.heading("Old Method (Sieve)");
                    if !self.is_running {
                        if left.button("Run (Old Method)").clicked() {
                            self.too_large_value = false;
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
                                self.stop_flag.store(false, Ordering::SeqCst);
                                self.recent_primes.clear();
                                self.scatter_data.clear();
                                self.found_count = 0;
                                self.total_found_primes = 0;
                                self.final_digit_counts = [0;10];
                                self.last_prime = None;
                                self.gap_counts.clear();
                                self.gap_max = 0;
                                self.gap_sum = 0;
                                self.gap_count = 0;

                                let config = self.config.clone();
                                let (sender, receiver) = mpsc::channel();
                                self.receiver = Some(receiver);
                                let stop_flag = self.stop_flag.clone();

                                thread::spawn(move || {
                                    let monitor_handle = start_resource_monitor(sender.clone());
                                    if let Err(e) = run_program_old(config, sender.clone(), stop_flag) {
                                        let _ = sender.send(WorkerMessage::Log(format!("An error occurred: {}\n", e)));
                                    }
                                    let _ = sender.send(WorkerMessage::HistogramUpdate {
                                        histogram: vec![]
                                    });
                                    let _ = sender.send(WorkerMessage::Done);
                                    drop(monitor_handle);
                                });

                            } else {
                                for error in errors {
                                    self.log.push_str(&format!("{}\n", error));
                                }
                            }
                        }
                    } else {
                        if left.button("STOP").clicked() {
                            self.stop_flag.store(true, Ordering::SeqCst);
                        }
                    }

                    left.label("prime_min (u64):");
                    left.text_edit_singleline(&mut self.prime_min_input_old);
                    left.label("prime_max (u64):");
                    left.text_edit_singleline(&mut self.prime_max_input_old);

                    left.separator();
                    left.heading("Prime Gaps Distribution");
                    if !self.histogram_data.is_empty() {
                        left.label("Histogram data collected, but not displayed as bar chart now.");
                        // 必要であれば別途バーグラフ表示処理を入れられますが、
                        // 今回はProgressバー化の要望なので割愛します。
                    } else {
                        left.label("No histogram data yet");
                    }

                    left.separator();
                    left.heading("Progress / System");
                    left.add(egui::ProgressBar::new(self.progress).show_percentage());
                    if self.total_range > 0 {
                        left.label(format!("Processed: {}/{}", self.current_processed, self.total_range));
                    } else {
                        left.label("Processed: N/A");
                    }
                    left.label(format!("ETA: {}", self.eta));
                    left.separator();

                    // CPU使用率0-100%表示のProgressBar
                    left.label("CPU Usage:");
                    left.add(egui::ProgressBar::new(self.cpu_usage/100.0).show_percentage());

                    // メモリ使用率0-100%表示のProgressBar
                    // total_mem はKB単位、mem_usageもKB単位。
                    // 0%〜100%に正規化するには mem_usage / total_mem
                    let mem_ratio = if self.total_mem > 0 {
                        self.mem_usage as f32 / self.total_mem as f32
                    } else {
                        0.0
                    };
                    left.label("Memory Usage:");
                    left.add(egui::ProgressBar::new(mem_ratio).show_percentage());
                    left.label(format!("{} KB / {} KB", self.mem_usage, self.total_mem));

                    left.separator();
                    left.heading("Verify Primes");
                    if !self.is_running && !self.is_verifying {
                        if left.button("Verify Primes").clicked() {
                            self.start_verification();
                        }
                    } else if self.is_verifying {
                        left.label("Verifying primes...");
                        left.add(egui::ProgressBar::new(self.progress).show_percentage());
                    }

                    left.separator();
                    left.heading("Log");
                    {
                        let lines: Vec<&str> = self.log.lines().collect();
                        let show_count = 10.min(lines.len());
                        if show_count > 0 {
                            for &line in lines.iter().rev().take(show_count) {
                                left.label(line);
                            }
                        } else {
                            left.label("No logs yet");
                        }
                    }

                    // 右カラム
                    right.heading("New Method (Miller-Rabin)");
                    if !self.is_running {
                        if right.button("Run (Miller-Rabin)").clicked() {
                            self.too_large_value = false;
                            let mut errors = Vec::new();

                            let prime_min_bi = match self.prime_min_input_new.trim().parse::<BigUint>() {
                                Ok(v) => v,
                                Err(_) => {
                                    errors.push("Invalid prime_min (new).");
                                    BigUint::one()
                                }
                            };

                            let prime_max_bi = match self.prime_max_input_new.trim().parse::<BigUint>() {
                                Ok(v) => v,
                                Err(_) => {
                                    errors.push("Invalid prime_max (new).");
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

                                let prime_max_str = &self.config.prime_max;
                                if prime_max_str.len() >= 20 {
                                    self.too_large_value = true;
                                }

                                self.is_running = true;
                                self.progress = 0.0;
                                self.eta = "Calculating...".to_string();
                                self.histogram_data.clear();
                                self.stop_flag.store(false, Ordering::SeqCst);
                                self.recent_primes.clear();
                                self.scatter_data.clear();
                                self.found_count = 0;
                                self.total_found_primes = 0;
                                self.final_digit_counts = [0;10];
                                self.last_prime = None;
                                self.gap_counts.clear();
                                self.gap_max = 0;
                                self.gap_sum = 0;
                                self.gap_count = 0;

                                let config = self.config.clone();
                                let (sender, receiver) = mpsc::channel();
                                self.receiver = Some(receiver);
                                let stop_flag = self.stop_flag.clone();

                                thread::spawn(move || {
                                    let monitor_handle = start_resource_monitor(sender.clone());
                                    if let Err(e) = run_program_new(config, sender.clone(), stop_flag) {
                                        let _ = sender.send(WorkerMessage::Log(format!("An error occurred: {}\n", e)));
                                    }
                                    let _ = sender.send(WorkerMessage::HistogramUpdate {
                                        histogram: vec![]
                                    });
                                    let _ = sender.send(WorkerMessage::Done);
                                    drop(monitor_handle);
                                });

                            } else {
                                for error in errors {
                                    self.log.push_str(&format!("{}\n", error));
                                }
                            }
                        }
                    } else {
                        if right.button("STOP").clicked() {
                            self.stop_flag.store(true, Ordering::SeqCst);
                        }
                    }

                    right.label("prime_min (BigInt):");
                    right.text_edit_singleline(&mut self.prime_min_input_new);
                    right.label("prime_max (BigInt):");
                    right.text_edit_singleline(&mut self.prime_max_input_new);
                    right.label("Miller-Rabin rounds:");
                    right.text_edit_singleline(&mut self.miller_rabin_rounds_input);

                    right.separator();
                    right.heading("Gap Statistics");
                    if self.gap_count > 0 {
                        let mut gaps: Vec<u64> = self.gap_counts.iter().flat_map(|(g,c)| std::iter::repeat(*g).take(*c as usize)).collect();
                        gaps.sort();

                        let avg_gap = self.gap_sum as f64 / self.gap_count as f64;
                        let min_gap = gaps[0];
                        let max_gap = self.gap_max;

                        right.label(format!("Count: {}", self.gap_count));
                        right.label(format!("Min Gap: {}", min_gap));
                        right.label(format!("Max Gap: {}", max_gap));
                        right.label(format!("Average Gap: {:.2}", avg_gap));
                    } else {
                        right.label("No gap data yet");
                    }

                    right.separator();
                    right.heading("Interval Prime Counts");
                    {
                        let show_count = 5.min(self.histogram_data.len());
                        if show_count > 0 {
                            for &(x, c) in self.histogram_data.iter().rev().take(show_count) {
                                right.label(format!("Interval near {}: {} primes", x, c));
                            }
                        } else {
                            right.label("No interval data yet");
                        }
                    }

                    right.separator();
                    right.heading("Scatter");
                    right.label("Scatter (prime,index) (last 5):");
                    {
                        let show_count = 5.min(self.scatter_data.len());
                        if show_count > 0 {
                            for &[prime, idx] in self.scatter_data.iter().rev().take(show_count) {
                                right.label(format!("prime={} at idx={}", prime as u64, idx as u64));
                            }
                        } else {
                            right.label("No scatter data");
                        }
                    }

                    right.separator();
                    right.heading("Final Digit Distribution (%)");
                    if self.total_found_primes > 0 {
                        for digit in 0..10 {
                            let count = self.final_digit_counts[digit];
                            let percent = (count as f64 / self.total_found_primes as f64) * 100.0;
                            right.label(format!("Digit {}: {:.2}%", digit, percent));
                        }
                    } else {
                        right.label("No primes yet");
                    }

                });
            });
        });

        ctx.request_repaint();
    }
}

pub fn start_resource_monitor(sender:mpsc::Sender<WorkerMessage>)->std::thread::JoinHandle<()>{
    std::thread::spawn(move||{
        let mut sys=System::new_all();
        loop {
            sys.refresh_all();
            let cpu_percent=sys.global_cpu_info().cpu_usage();
            let mem_usage=sys.used_memory();
            if sender.send(WorkerMessage::CpuMemUsage { cpu_percent, mem_usage }).is_err(){
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    })
}
