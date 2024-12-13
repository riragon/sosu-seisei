use crate::config::{Config, load_or_create_config, save_config};
use eframe::{egui, App};
use std::sync::{mpsc, Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use crate::sieve::run_program_old;
use sysinfo::{System, SystemExt};

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum WorkerMessage {
    Log(String),
    Progress { current: u64, total: u64 },
    Eta(String),
    MemUsage(u64), // CPU Usage 削除、MemUsageのみ
    FoundPrimeIndex(u64, u64),
    Done,
    Stopped,
}

pub struct MyApp {
    pub config: Config,
    pub is_running: bool,
    pub log: String,
    pub receiver: Option<mpsc::Receiver<WorkerMessage>>,

    pub prime_min_input_old: String,
    pub prime_max_input_old: String,

    pub progress: f32,
    pub eta: String,
    pub mem_usage: u64, // CPU usage 削除
    pub stop_flag: Arc<AtomicBool>,

    pub total_mem: u64,
    pub current_processed: u64,
    pub total_range: u64,
}

impl MyApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let config = load_or_create_config().unwrap_or_default();

        let mut sys = System::new_all();
        sys.refresh_all();
        let total_mem = sys.total_memory(); // KB単位

        MyApp {
            prime_min_input_old: config.prime_min.clone(),
            prime_max_input_old: config.prime_max.clone(),
            config,
            is_running: false,
            log: String::new(),
            receiver: None,

            progress: 0.0,
            eta: "N/A".to_string(),
            mem_usage: 0,
            stop_flag: Arc::new(AtomicBool::new(false)),

            total_mem,
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
                        let p = current as f32 / total as f32;
                        self.progress = p;
                        self.current_processed = current;
                        self.total_range = total;
                    }
                    WorkerMessage::Eta(eta_str)=> {
                        self.eta=eta_str;
                    }
                    WorkerMessage::MemUsage(mem_usage) => {
                        self.mem_usage = mem_usage;
                    }
                    WorkerMessage::FoundPrimeIndex(_pr, _idx)=> {
                        // 必要に応じてログなどに反映可能
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
                }
            }
            if remove_receiver {
                self.receiver=None;
            }
        }

        egui::CentralPanel::default().show(ctx,|ui| {
            egui::ScrollArea::vertical().show(ui,|ui| {
                ui.heading("Sosu-Seisei (Old Method Sieve)");
                ui.separator();

                ui.label("Old Method (Sieve)");
                if !self.is_running {
                    if ui.button("Run (Old Method)").clicked() {
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

                        let max_limit = 999_999_999_999_999_999u64;
                        if prime_max > max_limit {
                            errors.push("prime_max must be <= 999999999999999999.");
                        }

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
                            self.stop_flag.store(false, Ordering::SeqCst);
                            self.current_processed = 0;
                            self.total_range = 0;

                            let config = self.config.clone();
                            let (sender, receiver) = mpsc::channel();
                            self.receiver = Some(receiver);
                            let stop_flag = self.stop_flag.clone();

                            std::thread::spawn(move || {
                                let monitor_handle = super::app::start_resource_monitor(sender.clone());
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
                } else {
                    if ui.button("STOP").clicked() {
                        self.stop_flag.store(true, Ordering::SeqCst);
                    }
                }

                ui.label("prime_min (u64):");
                ui.text_edit_singleline(&mut self.prime_min_input_old);
                ui.label("prime_max (u64):");
                ui.text_edit_singleline(&mut self.prime_max_input_old);

                ui.separator();
                ui.heading("Progress / System");
                ui.add(egui::ProgressBar::new(self.progress).show_percentage());
                if self.total_range > 0 {
                    ui.label(format!("Processed: {}/{}", self.current_processed, self.total_range));
                } else {
                    ui.label("Processed: N/A");
                }
                ui.label(format!("ETA: {}", self.eta));
                ui.separator();

                // CPU usage削除、Memory Usageを数値でのみ表示
                ui.label(format!("Memory Usage: {} KB / {} KB", self.mem_usage, self.total_mem));

                ui.separator();
                ui.heading("Log");
                {
                    let lines: Vec<&str> = self.log.lines().collect();
                    let show_count = 10.min(lines.len());
                    if show_count > 0 {
                        for &line in lines.iter().rev().take(show_count) {
                            ui.label(line);
                        }
                    } else {
                        ui.label("No logs yet");
                    }
                }

            });
        });

        ctx.request_repaint();
    }
}

pub fn start_resource_monitor(sender:mpsc::Sender<WorkerMessage>)->std::thread::JoinHandle<()> {
    std::thread::spawn(move|| {
        let mut sys = sysinfo::System::new_all();
        sys.refresh_memory();

        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            sys.refresh_memory();

            let mem_usage = sys.used_memory();

            if sender.send(WorkerMessage::MemUsage(mem_usage)).is_err() {
                break;
            }
        }
    })
}
