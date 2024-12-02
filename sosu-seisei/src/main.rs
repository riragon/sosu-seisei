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
        "素数生成プログラム",
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
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // フォントを設定
        let mut fonts = egui::FontDefinitions::default();

        // フォントファイルのパス
        let font_path = "assets/NotoSansJP-Black.ttf";

        // フォントデータを読み込み
        if let Ok(mut font_file) = File::open(font_path) {
            let mut font_data = Vec::new();
            if font_file.read_to_end(&mut font_data).is_ok() {
                // フォントデータを追加
                fonts.font_data.insert(
                    "NotoSansJP".to_owned(),
                    egui::FontData::from_owned(font_data),
                );

                // フォントの優先度を設定
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .insert(0, "NotoSansJP".to_owned());
                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .insert(0, "NotoSansJP".to_owned());
            } else {
                eprintln!("フォントファイルの読み込みに失敗しました。");
            }
        } else {
            eprintln!("フォントファイルが見つかりません。パスを確認してください。");
        }

        // コンテキストにフォント設定を適用
        cc.egui_ctx.set_fonts(fonts);

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
        // スレッドからのメッセージを受信
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
            ui.heading("設定");

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

            if ui.button("設定を保存").clicked() {
                let mut errors = Vec::new();

                // prime_min の値をパース
                match self.prime_min_input.trim().parse::<u64>() {
                    Ok(value) => self.config.prime_min = value,
                    Err(_) => errors.push("prime_min の値が不正です。正の整数を入力してください。"),
                }

                // prime_max の値をパース
                match self.prime_max_input.trim().parse::<u64>() {
                    Ok(value) => self.config.prime_max = value,
                    Err(_) => errors.push("prime_max の値が不正です。正の整数を入力してください。"),
                }

                // prime_min と prime_max の関係を確認
                if self.config.prime_min >= self.config.prime_max {
                    errors.push("prime_min は prime_max より小さい必要があります。");
                }

                if errors.is_empty() {
                    if let Err(e) = save_config(&self.config) {
                        self.log
                            .push_str(&format!("設定の保存に失敗しました: {}\n", e));
                    } else {
                        self.log.push_str("設定を保存しました。\n");
                    }
                } else {
                    for error in errors {
                        self.log.push_str(&format!("{}\n", error));
                    }
                }
            }

            if ui.button("実行").clicked() && !self.is_running {
                // 実行前に入力値を検証
                let mut errors = Vec::new();

                // prime_min の値をパース
                match self.prime_min_input.trim().parse::<u64>() {
                    Ok(value) => self.config.prime_min = value,
                    Err(_) => errors.push("prime_min の値が不正です。正の整数を入力してください。"),
                }

                // prime_max の値をパース
                match self.prime_max_input.trim().parse::<u64>() {
                    Ok(value) => self.config.prime_max = value,
                    Err(_) => errors.push("prime_max の値が不正です。正の整数を入力してください。"),
                }

                // prime_min と prime_max の関係を確認
                if self.config.prime_min >= self.config.prime_max {
                    errors.push("prime_min は prime_max より小さい必要があります。");
                }

                if errors.is_empty() {
                    self.is_running = true;
                    let config = self.config.clone();
                    let (sender, receiver) = mpsc::channel();
                    self.receiver = Some(receiver);

                    // 別スレッドで実行
                    thread::spawn(move || {
                        if let Err(e) = run_program(config, sender.clone()) {
                            let _ = sender.send(format!("エラーが発生しました: {}\n", e));
                        }
                        // 完了を通知
                        let _ = sender.send("done".to_string());
                    });
                } else {
                    for error in errors {
                        self.log.push_str(&format!("{}\n", error));
                    }
                }
            }

            if self.is_running {
                ui.label("実行中...");
            } else {
                ui.label("待機中");
            }

            ui.separator();
            ui.heading("ログ");
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(&self.log);
            });
        });

        // 再描画をリクエスト
        ctx.request_repaint();
    }
}

// run_program関数でログメッセージを逐次送信
fn run_program(
    config: Config,
    sender: mpsc::Sender<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    sender.send("プログラムを開始します。\n".to_string()).ok();

    // プログラム全体の開始時間を記録
    let total_start_time = Instant::now();

    // 素数を保存するファイルを開く（新規作成・上書きモード）と大きなバッファサイズを設定
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open("primes.txt")
        .map_err(|e| format!("primes.txtのオープンに失敗しました：{}", e))?;

    let mut writer = BufWriter::with_capacity(config.writer_buffer_size, file);

    // 最初の prime_cache_size 個の素数を計算
    let small_primes = generate_small_primes(config.prime_cache_size)?;
    sender
        .send(format!(
            "最初の{}個の素数を生成しました。\n",
            small_primes.len()
        ))
        .ok();

    // 素数の総数をカウント
    let mut total_primes_found = 0;

    // 最初の小さな素数をファイルに一括書き込み
    for &prime in &small_primes {
        if prime >= config.prime_min {
            writeln!(writer, "{}", prime)
                .map_err(|e| format!("ファイルへの書き込みに失敗しました：{}", e))?;
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
            .send(format!("セグメントを処理中：{} - {}\n", low, high))
            .ok();

        let segment_start_time = Instant::now();

        // セグメント内で並列処理を行う
        let segment_primes =
            segmented_sieve_parallel(&small_primes, low, high, config.chunk_size)?;

        if !segment_primes.is_empty() {
            // ファイルへの書き込みをメインスレッドで一括処理
            for &prime in &segment_primes {
                if prime >= config.prime_min && prime <= config.prime_max {
                    writeln!(writer, "{}", prime)
                        .map_err(|e| format!("ファイルへの書き込みに失敗しました：{}", e))?;
                    total_primes_found += 1;
                }
            }

            sender
                .send(format!(
                    "このセグメントで{}個の素数を見つけました。\n",
                    segment_primes.len()
                ))
                .ok();
        }

        let segment_duration = segment_start_time.elapsed();
        sender
            .send(format!(
                "セグメント完了：{} - {}（処理時間：{:.2?}）\n",
                low, high, segment_duration
            ))
            .ok();

        // 次のセグメントへ移動
        low = high + 1;
    }

    // バッファをフラッシュ
    writer
        .flush()
        .map_err(|e| format!("バッファのフラッシュに失敗しました：{}", e))?;

    // プログラム全体の終了時間を記録
    let total_duration = total_start_time.elapsed();
    sender
        .send(format!("総計算時間：{:.2?}\n", total_duration))
        .ok();

    // 総素数数を表示
    sender
        .send(format!("見つかった素数の総数：{}\n", total_primes_found))
        .ok();

    Ok(())
}

// 他の関数（load_or_create_config、save_config、generate_small_primes、estimate_sieve_size、segmented_sieve_parallel）は変更ありません。

fn load_or_create_config() -> Result<Config, Box<dyn std::error::Error>> {
    if Path::new(SETTINGS_FILE).exists() {
        // 設定ファイルが存在する場合、読み込む
        let mut file = File::open(SETTINGS_FILE)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let config = toml::from_str(&contents)
            .map_err(|e| format!("設定ファイルのパースに失敗しました：{}", e))?;

        Ok(config)
    } else {
        // 設定ファイルが存在しない場合、デフォルト設定を作成して書き出す
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
                .ok_or("整数オーバーフローが発生しました")?;
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

    // セグメントをチャンクに分割し、各チャンクを並列に処理
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
                    None => continue, // オーバーフローした場合は次の素数へ
                };
                if prime_square > chunk_high {
                    break;
                }

                // 開始位置を計算
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
                        None => break, // オーバーフローした場合はループを抜ける
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
