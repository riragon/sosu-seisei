use serde::{Deserialize, Serialize};
use std::error::Error;
use std::process;
use std::io::{self, BufWriter, Write, Read};
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::time::Instant;
use bitvec::prelude::*;
use rayon::prelude::*;

const SETTINGS_FILE: &str = "settings.txt";

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    prime_cache_size: usize,
    segment_size: u64,
    chunk_size: usize,
    writer_buffer_size: usize,
    prime_max: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            prime_cache_size: 100_000,
            segment_size: 10_000_000,
            chunk_size: 16_384,
            writer_buffer_size: 8 * 1024 * 1024,
            prime_max: 10_000_000_000,
        }
    }
}

fn main() {
    if let Err(e) = run_program() {
        eprintln!("プログラム実行中にエラーが発生しました：{}", e);
        process::exit(1);
    }
}

fn run_program() -> Result<(), Box<dyn Error>> {
    // プログラム実行前の確認プロンプト
    println!("sosu.exeを実行します。実行しますか？\nEnterキーを押すと実行が開始されます。");
    let mut input = String::new();
    io::stdout().flush()?; // プロンプトを即座に表示

    // Enterキーが押されるまで待機
    io::stdin().read_line(&mut input)?;

    // デバッグメッセージ
    println!("入力を受け取りました。プログラムを続行します。");

    // 設定ファイルを読み込むか、作成する
    let config = load_or_create_config()?;

    // デバッグメッセージ
    println!("設定ファイルを読み込みました。");

    // プログラム全体の開始時間を記録
    let total_start_time = Instant::now();

    // 素数の総数をカウントする変数
    let mut total_primes_found: usize = 0;

    println!("使用する設定:");
    println!("Prime cache size: {}", config.prime_cache_size);
    println!("Segment size: {}", config.segment_size);
    println!("Chunk size: {}", config.chunk_size);
    println!("Writer buffer size: {}", config.writer_buffer_size);
    println!("Prime max: {}", config.prime_max);

    // 素数を保存するファイルを開く（新規作成・上書きモード）と大きなバッファサイズを設定
    let file = OpenOptions::new()
        .create(true)
        .truncate(true) // 既存のファイルを上書き
        .write(true)
        .open("primes.txt")
        .map_err(|e| format!("primes.txtのオープンに失敗しました：{}", e))?;

    let mut writer = BufWriter::with_capacity(config.writer_buffer_size, file);

    // 最初の PRIME_CACHE_SIZE 個の素数を計算
    let small_primes = generate_small_primes(config.prime_cache_size)?;
    println!("最初の{}個の素数を生成しました。", small_primes.len());

    // 素数の総数を更新
    total_primes_found += small_primes.len();

    // 最初の小さな素数をファイルに一括書き込み
    for &prime in &small_primes {
        writeln!(writer, "{}", prime)
            .map_err(|e| format!("ファイルへの書き込みに失敗しました：{}", e))?;
    }

    let mut low = small_primes.last().unwrap() + 1;

    while low <= config.prime_max {
        let high = low.saturating_add(config.segment_size - 1).min(config.prime_max);

        println!("セグメントを処理中：{} - {}", low, high);

        let segment_start_time = Instant::now();

        // セグメント内で並列処理を行う
        let segment_primes = segmented_sieve_parallel(
            &small_primes,
            low,
            high,
            config.chunk_size,
        )?;

        if !segment_primes.is_empty() {
            // ファイルへの書き込みをメインスレッドで一括処理
            for &prime in &segment_primes {
                writeln!(writer, "{}", prime)
                    .map_err(|e| format!("ファイルへの書き込みに失敗しました：{}", e))?;
            }

            // 素数の総数を更新
            total_primes_found += segment_primes.len();

            // コンソールへの出力を制限
            println!("このセグメントで{}個の素数を見つけました。", segment_primes.len());
        }

        let segment_duration = segment_start_time.elapsed();
        println!(
            "セグメント完了：{} - {}（処理時間：{:.2?}）",
            low, high, segment_duration
        );

        // 次のセグメントへ移動
        low = high + 1;
    }

    // バッファをフラッシュ
    writer.flush()
        .map_err(|e| format!("バッファのフラッシュに失敗しました：{}", e))?;

    // プログラム全体の終了時間を記録
    let total_duration = total_start_time.elapsed();
    println!("総計算時間：{:.2?}", total_duration);

    // 総素数数を表示
    println!("見つかった素数の総数：{}", total_primes_found);

    // プログラム終了前に、ユーザーが入力を行うまで待機
    println!("プログラムが完了しました。終了するにはEnterキーを押してください。");
    io::stdout().flush()?; // プロンプトを即座に表示
    let mut exit_input = String::new();
    io::stdin().read_line(&mut exit_input)?;

    Ok(())
}

fn load_or_create_config() -> Result<Config, Box<dyn Error>> {
    if Path::new(SETTINGS_FILE).exists() {
        // 設定ファイルが存在する場合、読み込む
        let mut file = File::open(SETTINGS_FILE)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let config = toml::from_str(&contents)
            .map_err(|e| format!("設定ファイルのパースに失敗しました：{}", e))?;

        println!("settings.txt を読み込みました。");

        Ok(config)
    } else {
        // 設定ファイルが存在しない場合、デフォルト設定を作成して書き出す
        let config = Config::default();
        save_config(&config)?;
        println!("settings.txt を書き出しました。");

        Ok(config)
    }
}

fn save_config(config: &Config) -> Result<(), Box<dyn Error>> {
    let toml_str = toml::to_string(config)?;
    let file = File::create(SETTINGS_FILE)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(toml_str.as_bytes())?;
    Ok(())
}

fn generate_small_primes(n: usize) -> Result<Vec<u64>, Box<dyn Error>> {
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
            let start = num.checked_mul(num).ok_or("整数オーバーフローが発生しました")?;
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
) -> Result<Vec<u64>, Box<dyn Error>> {
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
