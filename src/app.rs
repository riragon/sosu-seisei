// Copyright (c) 2024 riragon
//
// This software is released under the MIT License.
// See LICENSE file in the project root directory for more information.

use crate::config::{Config, load_or_create_config, save_config, OutputFormat};
use eframe::{egui, App};
use std::sync::{mpsc, Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use crate::sieve::run_program_old;
use sysinfo::{System, SystemExt};
use rfd::FileDialog;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum WorkerMessage {
    Log(String),
    Progress { current: u64, total: u64 },
    Eta(String),
    MemUsage(u64),
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
    pub split_count_input_old: String, // split_count用

    pub progress: f32,
    pub eta: String,
    pub mem_usage: u64,
    pub stop_flag: Arc<AtomicBool>,

    pub total_mem: u64,
    pub current_processed: u64,
    pub total_range: u64,

    pub selected_format: OutputFormat,
    pub output_dir_input: String,
}

impl MyApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config = load_or_create_config().unwrap_or_default();
        let mut sys = System::new_all();
        sys.refresh_all();
        let total_mem = sys.total_memory(); // in KB

        let selected_format = config.output_format.clone();
        let output_dir_input = config.output_dir.clone();

        // グローバルなスタイル調整
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);  // 項目間の距離
        style.spacing.button_padding = egui::vec2(8.0, 4.0); // ボタン内パディング
        style.visuals.window_rounding = egui::Rounding::same(5.0); // 角をわずかに丸く
        style.visuals.widgets.active.rounding = egui::Rounding::same(4.0);
        cc.egui_ctx.set_style(style);

        MyApp {
            prime_min_input_old: config.prime_min.clone(),
            prime_max_input_old: config.prime_max.clone(),
            split_count_input_old: config.split_count.to_string(),

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

            selected_format,
            output_dir_input,
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
                    WorkerMessage::Eta(eta_str) => {
                        self.eta = eta_str;
                    }
                    WorkerMessage::MemUsage(mem_usage) => {
                        self.mem_usage = mem_usage;
                    }
                    WorkerMessage::FoundPrimeIndex(_pr, _idx) => {}
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

        // ヘッダーパネル
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.columns(2, |columns| {
                columns[0].heading("Sosu-Seisei Sieve");
                columns[0].add_space(4.0);

                columns[1].with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(4.0);
                    if !self.is_running {
                        if ui.add(egui::Button::new("Run").min_size(egui::vec2(100.0,40.0))).clicked() {
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

                            let split_count = match self.split_count_input_old.trim().parse::<u64>() {
                                Ok(v) => v,
                                Err(_) => {
                                    errors.push("split_count is not a valid u64 integer.");
                                    0
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
                                self.config.output_format = self.selected_format.clone();
                                self.config.output_dir = self.output_dir_input.clone();
                                self.config.split_count = split_count;

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
                        if ui.add(egui::Button::new("STOP").min_size(egui::vec2(100.0,40.0))).clicked() {
                            self.stop_flag.store(true, Ordering::SeqCst);
                        }
                    }
                });
            });
        });

        // 下部パネル（ログ）
        egui::TopBottomPanel::bottom("log_panel").show(ctx, |ui| {
            ui.heading("Log");
            ui.separator();
            ui.add_space(4.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                let lines: Vec<&str> = self.log.lines().collect();
                if !lines.is_empty() {
                    for &line in lines.iter() {
                        ui.label(line);
                    }
                } else {
                    ui.label("No logs yet");
                }
            });
        });

        // 中央パネル
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(2, |columns| {
                // 左列（Settings）
                columns[0].heading("Settings");
                columns[0].add_space(8.0);
                columns[0].separator();
                columns[0].add_space(8.0);

                columns[0].label("prime_min (u64):");
                columns[0].text_edit_singleline(&mut self.prime_min_input_old);
                columns[0].add_space(4.0);

                columns[0].label("prime_max (u64):");
                columns[0].text_edit_singleline(&mut self.prime_max_input_old);
                columns[0].add_space(8.0);

                // split_count 項目追加
                columns[0].separator();
                columns[0].add_space(8.0);
                columns[0].label("split_count (u64):");
                columns[0].text_edit_singleline(&mut self.split_count_input_old);
                columns[0].label("0 means no splitting. If a number is specified, the output primes file\nwill be split into multiple files every specified number of primes.");
                columns[0].add_space(8.0);

                columns[0].separator();
                columns[0].add_space(8.0);
                columns[0].label("Output Format:");
                egui::ComboBox::new("output_format", "")
                    .selected_text(format!("{:?}", self.selected_format))
                    .show_ui(&mut columns[0], |ui| {
                        ui.selectable_value(&mut self.selected_format, OutputFormat::Text, "Text");
                        ui.selectable_value(&mut self.selected_format, OutputFormat::CSV, "CSV");
                        ui.selectable_value(&mut self.selected_format, OutputFormat::JSON, "JSON");
                    });
                columns[0].add_space(8.0);

                columns[0].separator();
                columns[0].add_space(8.0);
                columns[0].label("Output Directory:");
                columns[0].text_edit_singleline(&mut self.output_dir_input);
                columns[0].add_space(4.0);
                columns[0].horizontal(|ui| {
                    if ui.add_sized([90.0, 0.0], egui::Button::new("Select Folder")).clicked() {
                        if let Some(folder) = FileDialog::new().pick_folder() {
                            self.output_dir_input = folder.display().to_string();
                        }
                    }
                });

                // 右列（Progress / System）
                columns[1].heading("Progress / System");
                columns[1].add_space(8.0);
                columns[1].separator();
                columns[1].add_space(8.0);

                columns[1].add(egui::ProgressBar::new(self.progress).show_percentage());
                if self.total_range > 0 {
                    columns[1].label(format!("Processed: {}/{}", self.current_processed, self.total_range));
                } else {
                    columns[1].label("Processed: N/A");
                }
                columns[1].label(format!("ETA: {}", self.eta));
                columns[1].add_space(8.0);
                columns[1].separator();
                columns[1].add_space(8.0);
                columns[1].label(format!("Memory Usage: {} KB / {} KB", self.mem_usage, self.total_mem));
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
