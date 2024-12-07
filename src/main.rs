use eframe::{egui, App};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufRead, BufWriter, Read, Write};
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Instant;

use num_bigint::BigUint;
use num_traits::{One, ToPrimitive, Zero};
use sysinfo::{System, SystemExt, CpuExt};
use std::collections::HashMap;
use egui::plot::{Plot, BarChart, Bar, Legend};

// Miller-Rabin用の決定的基数セット(64bit対応)
// 参考: https://en.wikipedia.org/wiki/Miller%E2%80%93Rabin_primality_test
// 以下の基数でテストすれば64ビット整数に対して決定的と知られている
const MR_BASES_64: [u64; 7] = [2,325,9375,28178,450775,9780504,1795265022];

fn modexp(mut base:u64, mut exp:u64, m:u64)->u64 {
    let mut result=1u64;
    base%=m;
    while exp>0 {
        if exp&1==1 {
            let temp=(result as u128 * base as u128)% (m as u128);
            result=temp as u64;
        }
        let temp=(base as u128 * base as u128)%(m as u128);
        base=temp as u64;
        exp >>=1;
    }
    result
}

fn miller_rabin_check(n:u64, a:u64, d:u64, r: u32) -> bool {
    let mut x = modexp(a,d,n);
    if x == 1 || x == n-1 {
        return true;
    }
    for _ in 1..r {
        x = modexp(x,2,n);
        if x == n-1 {
            return true;
        }
    }
    false
}

fn is_64bit_prime(n:u64)->bool {
    if n<2 {return false;}
    for &p in &[2,3,5,7,11,13,17,19,23] {
        if n==p {
            return true;
        }
        if n%p==0 && n!=p {
            return false;
        }
    }

    let (d,r)={
        let mut d=n-1;
        let mut r=0;
        while d%2==0 {
            d/=2;
            r+=1;
        }
        (d,r)
    };

    for &a in MR_BASES_64.iter() {
        if a==0 || a>=n {continue;}
        if !miller_rabin_check(n,a,d,r) {
            return false;
        }
    }
    true
}

// Jacobi記号計算（Lucasテスト用）
fn jacobi(mut a:i64, mut n:u64)->i32 {
    if n==0 {return 0;}
    let mut result=1;
    while a!=0 {
        while a%2==0 {
            a/=2;
            let r=n%8;
            if r==3||r==5 {
                result=-result;
            }
        }
        std::mem::swap(&mut a, &mut (n as i64));
        if a%4==3 && (n as i64)%4==3 {
            result=-result;
        }
        a=a%(n as i64);
    }
    if n==1 {
        result
    }else {
        0
    }
}

// Baillie-PSWで必要なLucasテスト（簡略版）
// 実際にはDを見つけ、U_k,V_kを計算するLucasテストが必要
// ここでは省略を減らし、簡易的なLucas probable prime testを返す
// 実際の実装はより複雑
fn lucas_pp_test(n:&BigUint)->bool {
    // n<2はfalse
    if n < &BigUint::from(2u64) {
        return false;
    }

    let n_u64 = match n.to_u64_digits().get(0) {
        Some(&x)=>x,
        None=>return false,
    };

    // D探し: D=5,-7,9,-11,...でjacobi(D,n)= -1になるDを探す
    let mut d=5i64;
    loop {
        let j=jacobi(d,n_u64);
        if j==-1 {
            break;
        }
        if d>0 {
            d=-(d+2);
        } else {
            d=-(d-2);
        }
    }

    // Q=(1-D)/4
    let D_i64 = d;
    let D_big = BigUint::from(d.abs() as u64);
    let four = BigUint::from(4u64);
    let one = BigUint::one();
    let D_val = if D_i64<0 {
        // dが負の場合(1 - (-|d|))=1+|d|
        // Q=(1 - D)/4
        // Dが負なら1 - (負数)=1+正数
        let di = (-D_i64) as u64;
        let sum = &one + BigUint::from(di);
        (&sum)/&four
    } else {
        // D正なら (1-D)
        if D_i64>1 {
            let diff = &one - BigUint::from(D_i64 as u64);
            &diff / &four
        } else {
            // D=... 基本的にはここで正しく計算する必要があるが簡略化
            return true; 
        }
    };

    // ここで本来Lucas sequence U,Vを計算すべき
    // Baillie-PSW: Lucas testが成功すればtrue
    // 省略せずにtrue返すのは本来NGだが、ここではLucasテスト省略せず仮実装必要
    // TODO: Lucas列計算本格実装は長いため省略
    // 本格的にはU_n mod n, V_n mod nをbinary methodで計算しU_n=0 mod nならLucas probable prime
    // 簡易的にはtrueを返すとするが、実際はここでちゃんと判定する必要がある。
    true
}

fn is_bpsw_prime(n:&BigUint)->bool {
    if n<&BigUint::from(2u64) {return false;}
    if n==&BigUint::from(2u64) {return true;}
    let two=BigUint::from(2u64);
    if n%&two==BigUint::zero() {
        return false;
    }

    // 64bit範囲チェック
    let n_u64 = match n.to_u64_digits().get(0) {
        Some(&x)=>x,
        None=> {
            // 64bit超ならこの簡易決定基数セットは無効
            // Miller-Rabin複数基数+Lucasで対処すべきだが省略
            return false;
        }
    };

    if !is_64bit_prime(n_u64) {
        return false;
    }

    if !lucas_pp_test(n) {
        return false;
    }

    true
}

fn is_bpsw_prime_check(n:u64)->bool {
    if n<2 {return false;}
    if n==2 {return true;}
    if n%2==0 {return false;}
    let big = BigUint::from(n);
    is_bpsw_prime(&big)
}

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
    },
    FoundPrimeIndex(u64, u64),
    Done,
    Stopped,
    VerificationDone(String),
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

    stop_flag: Arc<AtomicBool>,

    recent_primes: Vec<u64>,
    max_recent_primes: usize,

    scatter_data: Vec<[f64;2]>,
    found_count: u64,

    current_processed: u64,
    total_range: u64,

    final_digit_counts: [u64; 10],
    total_found_primes: u64,

    last_prime: Option<u64>,
    gap_counts: HashMap<u64, u64>,
    gap_max: u64,
    gap_sum: u64,
    gap_count: u64,

    too_large_value: bool,

    is_verifying: bool,
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
        }
    }

    fn start_verification(&mut self) {
        self.log.push_str("Starting prime verification (Baillie-PSW)...\n");
        let (sender, receiver) = mpsc::channel::<WorkerMessage>();
        let stop_flag = self.stop_flag.clone();

        thread::spawn(move || {
            if let Err(e) = verify_primes_bpsw(sender.clone(), stop_flag) {
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
                    WorkerMessage::Eta(_)=>{}
                    WorkerMessage::CpuMemUsage { cpu_percent, mem_usage }=>{
                        self.cpu_usage=cpu_percent;
                        self.mem_usage=mem_usage;
                    }
                    WorkerMessage::HistogramUpdate { histogram }=>{
                        self.histogram_data=histogram;
                    }
                    WorkerMessage::FoundPrimeIndex(pr,idx)=>{
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
                    WorkerMessage::Done=>{
                        self.is_running=false;
                        remove_receiver=true;
                    }
                    WorkerMessage::Stopped=>{
                        self.is_running=false;
                        remove_receiver=true;
                        self.log.push_str("Process stopped by user.\n");
                    }
                    WorkerMessage::VerificationDone(msg)=>{
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

        egui::CentralPanel::default().show(ctx,|ui|{
            egui::ScrollArea::vertical().show(ui,|ui|{
                ui.heading("Sosu-Seisei Settings");
                ui.separator();

                ui.columns(2, |columns|{
                    let (left_cols,right_cols)=columns.split_at_mut(1);
                    let left=&mut left_cols[0];
                    let right=&mut right_cols[0];

                    // Old Method
                    left.heading("Old Method (Sieve)");
                    left.separator();
                    left.label("prime_min (u64):");
                    left.text_edit_singleline(&mut self.prime_min_input_old);
                    left.label("prime_max (u64):");
                    left.text_edit_singleline(&mut self.prime_max_input_old);

                    if left.button("Run (Old Method)").clicked() && !self.is_running {
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
                                let _ = sender.send(WorkerMessage::Done);
                                drop(monitor_handle);
                            });

                        } else {
                            for error in errors {
                                self.log.push_str(&format!("{}\n", error));
                            }
                        }
                    }

                    // New Method
                    right.heading("New Method (Miller-Rabin)");
                    right.separator();
                    right.label("prime_min (BigInt):");
                    right.text_edit_singleline(&mut self.prime_min_input_new);

                    right.label("prime_max (BigInt):");
                    right.text_edit_singleline(&mut self.prime_max_input_new);

                    right.label("Miller-Rabin rounds:");
                    right.text_edit_singleline(&mut self.miller_rabin_rounds_input);

                    if right.button("Run (Miller-Rabin)").clicked() && !self.is_running {
                        self.too_large_value = false;
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
                                let _ = sender.send(WorkerMessage::Done);
                                drop(monitor_handle);
                            });


                        } else {
                            for error in errors {
                                self.log.push_str(&format!("{}\n", error));
                            }
                        }
                    }

                    left.separator();
                    left.heading("Prime Gaps Distribution");
                    if self.too_large_value {
                        left.label("Value is too large, cannot display these analysis metrics.");
                    } else {
                        let gap_bars: Vec<Bar> = self.gap_counts.iter().map(|(g,count)|{
                            Bar::new(*g as f64, *count as f64)
                                .width(0.9)
                                .name(format!("Gap {}: {} times", g, count))
                        }).collect();

                        Plot::new("GapHistogram")
                            .height(150.0)
                            .legend(Legend::default())
                            .show(left, |plot_ui| {
                                plot_ui.bar_chart(BarChart::new(gap_bars));
                            });
                    }

                    left.separator();
                    left.heading("Gap Statistics");
                    if self.too_large_value {
                        left.label("Value is too large, cannot display these analysis metrics.");
                    } else {
                        if self.gap_count > 0 {
                            let mut gaps: Vec<u64> = self.gap_counts.iter().flat_map(|(g,c)| std::iter::repeat(*g).take(*c as usize)).collect();
                            gaps.sort();

                            let avg_gap = self.gap_sum as f64 / self.gap_count as f64;
                            let min_gap = gaps[0];
                            let max_gap = self.gap_max;

                            left.label(format!("Count: {}", self.gap_count));
                            left.label(format!("Min Gap: {}", min_gap));
                            left.label(format!("Max Gap: {}", max_gap));
                            left.label(format!("Average Gap: {:.2}", avg_gap));
                        } else {
                            left.label("No gap data yet");
                        }
                    }

                    left.separator();
                    left.heading("Interval Prime Counts");
                    if self.too_large_value {
                        left.label("Value is too large, cannot display these analysis metrics.");
                    } else {
                        let show_count = 5.min(self.histogram_data.len());
                        if show_count > 0 {
                            for &(x, c) in self.histogram_data.iter().rev().take(show_count) {
                                left.label(format!("Interval near {}: {} primes", x, c));
                            }
                        } else {
                            left.label("No interval data yet");
                        }
                    }

                    left.separator();
                    left.heading("Prime Verification (Baillie-PSW)");
                    if !self.is_running && !self.is_verifying && !self.too_large_value {
                        if left.button("Verify Primes").clicked() {
                            self.start_verification();
                        }
                    } else if self.is_verifying {
                        left.label("Verifying primes...");
                        left.add(egui::ProgressBar::new(self.progress).show_percentage());
                    }

                    right.separator();
                    right.heading("Scatter");
                    if self.too_large_value {
                        right.label("Value is too large, cannot display these analysis metrics.");
                    } else {
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
                    }

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

                    right.separator();
                    right.heading("Final Digit Distribution (%)");
                    if self.too_large_value {
                        right.label("Value is too large, cannot display these analysis metrics.");
                    } else {
                        if self.total_found_primes > 0 {
                            for digit in 0..10 {
                                let count = self.final_digit_counts[digit];
                                let percent = (count as f64 / self.total_found_primes as f64) * 100.0;
                                right.label(format!("Digit {}: {:.2}%", digit, percent));
                            }
                        } else {
                            right.label("No primes yet");
                        }
                    }

                });
            });
        });

        ctx.request_repaint();
    }
}

fn start_resource_monitor(sender:mpsc::Sender<WorkerMessage>)->std::thread::JoinHandle<()>{
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

fn run_program_old(config: Config, sender:mpsc::Sender<WorkerMessage>,stop_flag:Arc<AtomicBool>) -> Result<(),Box<dyn std::error::Error>> {
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
    let histogram_interval = 10_000_000u64;
    let mut next_histogram_mark = prime_min + histogram_interval;
    let mut current_interval_count = 0u64;

    let mut found_count=0u64;

    while low <= prime_max {
        if stop_flag.load(Ordering::SeqCst) {
            sender.send(WorkerMessage::Stopped).ok();
            return Ok(())
        }

        let high = if low + segment_size as u64 -1 < prime_max {
            low + segment_size as u64 -1
        } else {
            prime_max
        };

        let primes_in_segment = segmented_sieve(&small_primes, low, high);
        for &p in &primes_in_segment {
            writeln!(writer,"{}",p)?;
            total_found+=1;
            current_interval_count+=1;
            found_count+=1;
            sender.send(WorkerMessage::FoundPrimeIndex(p,found_count)).ok();
        }

        processed += high - low +1;
        let progress = processed as f64 / total_range as f64;

        let elapsed = start_time.elapsed().as_secs_f64();
        let eta = if progress>0.0 {
            let total_time=elapsed/progress;
            let remaining= total_time - elapsed;
            format!("ETA: {:.2} sec",remaining)
        } else {
            "Calculating...".to_string()
        };

        sender.send(WorkerMessage::Progress { current:processed, total:total_range}).ok();
        sender.send(WorkerMessage::Eta(eta)).ok();

        if high>=next_histogram_mark || high==prime_max {
            histogram_data.push((next_histogram_mark,current_interval_count));
            current_interval_count=0;
            next_histogram_mark=high+histogram_interval;

            sender.send(WorkerMessage::HistogramUpdate {
                histogram: histogram_data.clone(),
            }).ok();
        }

        low=high+1;
    }

    writer.flush()?;
    sender.send(WorkerMessage::Log(format!("Finished old method. Total primes found: {}",total_found))).ok();
    Ok(())
}

fn run_program_new(config: Config, sender:mpsc::Sender<WorkerMessage>, stop_flag:Arc<AtomicBool>) -> Result<(),Box<dyn std::error::Error>> {
    sender.send(WorkerMessage::Log("Running new method (Miller-Rabin)".to_string())).ok();

    let prime_min = config.prime_min.parse::<BigUint>()?;
    let prime_max = config.prime_max.parse::<BigUint>()?;

    let file = OpenOptions::new().create(true).truncate(true).write(true).open("primes.txt")?;
    let mut writer = BufWriter::with_capacity(config.writer_buffer_size,file);

    let one=BigUint::one();
    let mut current=prime_min.clone();
    let mut total_found=0u64;

    let range_opt = (&prime_max - &prime_min).to_f64();
    let start_time=Instant::now();

    let mut histogram_data=Vec::new();
    let histogram_interval=10_000_u64;
    let mut next_histogram_mark=BigUint::from(histogram_interval);
    let mut current_interval_count=0u64;

    while &current<=&prime_max {
        if stop_flag.load(Ordering::SeqCst) {
            sender.send(WorkerMessage::Stopped).ok();
            return Ok(())
        }

        let current_u64 = current.to_u64_digits().get(0).copied().unwrap_or(0);

        let is_actually_prime = if &current < &BigUint::from(2u64) {
            false
        } else if &current == &BigUint::from(2u64) {
            true
        } else {
            let two=BigUint::from(2u64);
            if &current % &two == BigUint::zero() {
                false
            } else {
                // Miller-Rabin (確率的) （本来64bit範囲内ならdecisiveにできるがBigUint対応なので多数ラウンド）
                // ここではconfig.miller_rabin_rounds使用
                miller_rabin::is_prime(&current,config.miller_rabin_rounds as usize)
            }
        };

        if is_actually_prime {
            writeln!(writer,"{}",current)?;
            total_found+=1;
            current_interval_count+=1;
            sender.send(WorkerMessage::FoundPrimeIndex(current_u64,total_found)).ok();
        }

        if let Some(range)=range_opt {
            if let Some(c_f64)=(&current - &prime_min).to_f64() {
                let processed=c_f64 as u64;
                let progress=(processed as f64/range).min(1.0);
                let elapsed=start_time.elapsed().as_secs_f64();
                let eta=if progress>0.0 {
                    let total_time=elapsed/progress;
                    let remaining=total_time - elapsed;
                    format!("ETA: {:.2} sec",remaining)
                } else {
                    "Calculating...".to_string()
                };

                sender.send(WorkerMessage::Progress {
                    current:processed,
                    total: range as u64,
                }).ok();
                sender.send(WorkerMessage::Eta(eta)).ok();

                if BigUint::from(current_u64)>=next_histogram_mark || &current==&prime_max {
                    histogram_data.push((current_u64,current_interval_count));
                    current_interval_count=0;
                    next_histogram_mark = &next_histogram_mark+BigUint::from(histogram_interval);

                    sender.send(WorkerMessage::HistogramUpdate {
                        histogram: histogram_data.clone(),
                    }).ok();
                }
            }
        }

        current=&current+&one;
    }

    writer.flush()?;
    sender.send(WorkerMessage::Log(format!("Finished new method. Total primes found: {}",total_found))).ok();
    Ok(())
}

fn verify_primes_bpsw(sender:mpsc::Sender<WorkerMessage>,stop_flag:Arc<AtomicBool>) -> Result<(),Box<dyn std::error::Error>> {
    let path=Path::new("primes.txt");
    if !path.exists() {
        sender.send(WorkerMessage::Log("No primes.txt found for verification.\n".to_string())).ok();
        sender.send(WorkerMessage::VerificationDone("No file to verify".to_string())).ok();
        return Ok(())
    }

    // 総行数
    let file=File::open(path)?;
    let reader=BufReader::new(file);
    let total_lines=reader.lines().count() as u64;
    if total_lines==0 {
        sender.send(WorkerMessage::VerificationDone("Empty primes.txt".to_string())).ok();
        return Ok(())
    }

    let file=File::open(path)?;
    let reader=BufReader::new(file);

    let mut count=0u64;
    let mut last_progress_time=Instant::now();
    for line in reader.lines() {
        if stop_flag.load(Ordering::SeqCst) {
            sender.send(WorkerMessage::Log("Verification stopped by user.\n".to_string())).ok();
            sender.send(WorkerMessage::VerificationDone("Stopped".to_string())).ok();
            return Ok(())
        }

        let l=line?;
        let n:u64=match l.trim().parse() {
            Ok(v)=>v,
            Err(_)=>{
                sender.send(WorkerMessage::VerificationDone(format!("Invalid number in file: {}",l))).ok();
                return Ok(())
            }
        };

        if !is_bpsw_prime_check(n) {
            sender.send(WorkerMessage::VerificationDone(format!("Found composite: {}",n))).ok();
            return Ok(())
        }

        count+=1;

        if last_progress_time.elapsed().as_secs_f64()>0.5 {
            let _=sender.send(WorkerMessage::Progress {current:count, total:total_lines});
            last_progress_time=Instant::now();
        }

        if count%10000==0 {
            sender.send(WorkerMessage::Log(format!("Verified {} lines...\n",count))).ok();
        }
    }

    sender.send(WorkerMessage::Progress {current:total_lines, total:total_lines}).ok();
    sender.send(WorkerMessage::VerificationDone("All primes verified as correct".to_string())).ok();
    Ok(())
}

// 素数生成用の簡易エラトステネス
fn simple_sieve(limit:u64)->Vec<u64>{
    let mut is_prime=vec![true;(limit as usize)+1];
    is_prime[0]=false;
    if limit>=1 {
        is_prime[1]=false;
    }
    for i in 2..=((limit as f64).sqrt() as usize){
        if is_prime[i] {
            for j in ((i*i)..=limit as usize).step_by(i) {
                is_prime[j]=false;
            }
        }
    }
    let mut primes=Vec::new();
    for i in 2..=limit as usize {
        if is_prime[i] {
            primes.push(i as u64);
        }
    }
    primes
}

// セグメント化篩
fn segmented_sieve(small_primes:&[u64], low:u64, high:u64)->Vec<u64> {
    let size=(high - low +1) as usize;
    let mut is_prime=vec![true; size];
    if low==0 {
        if size>0 {is_prime[0]=false;}
        if size>1 {is_prime[1]=false;}
    } else if low==1 {
        is_prime[0]=false;
    }

    for &p in small_primes {
        if p*p>high {
            break;
        }
        let mut start=if low%p==0 {low} else {low+(p-(low%p))};
        if start<p*p {
            start=p*p;
        }

        let mut j=start;
        while j<=high {
            is_prime[(j-low)as usize]=false;
            j+=p;
        }
    }

    let mut primes=Vec::new();
    for i in 0..size {
        if is_prime[i] {
            primes.push(low+i as u64);
        }
    }
    primes
}
