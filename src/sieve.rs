use std::sync::{mpsc,Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::io::{BufWriter, Write};
use std::fs::{OpenOptions, create_dir_all};
use std::path::Path;
use std::time::Instant;
use crate::config::{Config, OutputFormat};
use crate::app::WorkerMessage;
use rayon::prelude::*;

fn integer_sqrt(n: u64) -> u64 {
    let mut low = 0u64;
    let mut high = n;
    while low <= high {
        let mid = (low + high) >> 1;
        match mid.checked_mul(mid) {
            Some(val) if val == n => return mid,
            Some(val) if val < n  => low = mid + 1,
            _ => high = mid - 1,
        }
    }
    high
}

pub fn run_program_old(config: Config, sender:mpsc::Sender<WorkerMessage>, stop_flag:Arc<AtomicBool>) -> Result<(),Box<dyn std::error::Error>> {
    sender.send(WorkerMessage::Log("Running old method (Sieve) with parallelization".to_string())).ok();

    let prime_min = config.prime_min.parse::<u64>()?;
    let prime_max = config.prime_max.parse::<u64>()?;

    let root = integer_sqrt(prime_max) + 1;

    let small_primes = simple_sieve(root);

    let segment_size = config.segment_size as u64;
    let mut segments = Vec::new();
    {
        let mut start = prime_min;
        while start <= prime_max {
            let end = (start + segment_size -1).min(prime_max);
            segments.push((start, end));
            start = end + 1;
        }
    }

    let writer_buffer_size = config.writer_buffer_size;

    let (primes_sender, primes_receiver) = mpsc::channel::<Vec<u64>>();
    let stop_flag_clone = stop_flag.clone();
    let sender_clone = sender.clone();

    let start_time = Instant::now();
    let total_range = prime_max - prime_min + 1;

    let output_format = config.output_format.clone();
    let split_count = config.split_count;

    if !config.output_dir.is_empty() {
        create_dir_all(&config.output_dir)?;
    }

    let write_handle = std::thread::spawn(move || {
        let mut found_count=0u64;
        let mut processed = 0u64;

        let mut file_index = 1;
        let mut current_prime_count_in_file = 0u64;

        let open_file = |index: usize| {
            let base_name = match output_format {
                OutputFormat::Text => "primes",
                OutputFormat::CSV  => "primes",
                OutputFormat::JSON => "primes",
            };
            let file_ext = match output_format {
                OutputFormat::Text => "txt",
                OutputFormat::CSV  => "csv",
                OutputFormat::JSON => "json",
            };

            let file_name = if split_count > 0 {
                format!("{}_{:02}.{}", base_name, index, file_ext)
            } else {
                format!("{}.{}", base_name, file_ext)
            };

            let full_path = Path::new(&config.output_dir).join(file_name);
            let file = OpenOptions::new().create(true).truncate(true).write(true).open(&full_path).unwrap();
            BufWriter::with_capacity(writer_buffer_size, file)
        };

        let mut writer = open_file(file_index);

        let mut first_item = true;
        if let OutputFormat::JSON = output_format {
            write!(writer, "[").unwrap();
        }

        for primes_in_segment in primes_receiver {
            if stop_flag_clone.load(Ordering::SeqCst) {
                break;
            }

            match output_format {
                OutputFormat::Text => {
                    for p in &primes_in_segment {
                        writeln!(writer,"{}",p).unwrap();
                        found_count+=1;
                        current_prime_count_in_file += 1;
                        sender_clone.send(WorkerMessage::FoundPrimeIndex(*p,found_count)).ok();

                        if stop_flag_clone.load(Ordering::SeqCst) {
                            break;
                        }

                        if split_count > 0 && current_prime_count_in_file >= split_count {
                            writer.flush().unwrap();
                            if let OutputFormat::JSON = output_format {
                                write!(writer, "]").unwrap();
                                writer.flush().unwrap();
                            }
                            file_index += 1;
                            writer = open_file(file_index);
                            current_prime_count_in_file = 0;
                            if let OutputFormat::JSON = output_format {
                                write!(writer, "[").unwrap();
                                first_item = true;
                            }
                        }
                    }
                },
                OutputFormat::CSV => {
                    let line: String = primes_in_segment.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",");
                    writeln!(writer,"{}",line).unwrap();
                    for p in primes_in_segment {
                        found_count+=1;
                        current_prime_count_in_file += 1;
                        sender_clone.send(WorkerMessage::FoundPrimeIndex(p,found_count)).ok();

                        if stop_flag_clone.load(Ordering::SeqCst) {
                            break;
                        }

                        if split_count > 0 && current_prime_count_in_file >= split_count {
                            writer.flush().unwrap();
                            if let OutputFormat::JSON = output_format {
                                write!(writer, "]").unwrap();
                                writer.flush().unwrap();
                            }
                            file_index += 1;
                            writer = open_file(file_index);
                            current_prime_count_in_file = 0;
                            if let OutputFormat::JSON = output_format {
                                write!(writer, "[").unwrap();
                                first_item = true;
                            }
                        }
                    }
                },
                OutputFormat::JSON => {
                    for p in primes_in_segment {
                        if !first_item {
                            write!(writer,",{}", p).unwrap();
                        } else {
                            write!(writer,"{}", p).unwrap();
                            first_item = false;
                        }
                        found_count+=1;
                        current_prime_count_in_file += 1;
                        sender_clone.send(WorkerMessage::FoundPrimeIndex(p,found_count)).ok();

                        if stop_flag_clone.load(Ordering::SeqCst) {
                            break;
                        }

                        if split_count > 0 && current_prime_count_in_file >= split_count {
                            writer.flush().unwrap();
                            write!(writer, "]").unwrap();
                            writer.flush().unwrap();

                            file_index += 1;
                            writer = open_file(file_index);
                            current_prime_count_in_file = 0;
                            write!(writer, "[").unwrap();
                            first_item = true;
                        }
                    }
                },
            }

            if stop_flag_clone.load(Ordering::SeqCst) {
                break; 
            }

            let segment_range = segment_size.min(total_range-processed);
            processed += segment_range;
            let progress = processed as f64 / total_range as f64;

            let elapsed = start_time.elapsed().as_secs_f64();
            let eta= if progress>0.0 {
                let total_time=elapsed/progress;
                let remaining= total_time - elapsed;
                let remaining_sec = remaining.round() as u64;
                let hours = remaining_sec / 3600;
                let minutes = (remaining_sec % 3600) / 60;
                let seconds = remaining_sec % 60;
                format!("ETA: {} hour {} min {} sec", hours, minutes, seconds)
            } else {
                "Calculating...".to_string()
            };

            sender_clone.send(WorkerMessage::Progress { current:processed, total:total_range}).ok();
            sender_clone.send(WorkerMessage::Eta(eta)).ok();

            if stop_flag_clone.load(Ordering::SeqCst) {
                break;
            }
        }

        if let OutputFormat::JSON = output_format {
            write!(writer, "]").unwrap();
        }
        writer.flush().unwrap();

        sender_clone.send(WorkerMessage::Log(format!("Finished old method. Total primes found: {}",found_count))).ok();
        sender_clone.send(WorkerMessage::Done).ok();
    });

    segments.into_par_iter().for_each(|(low, high)| {
        if stop_flag.load(Ordering::SeqCst) {
            return;
        }
        let primes_in_segment = segmented_sieve(&small_primes, low, high, &stop_flag);
        if stop_flag.load(Ordering::SeqCst) {
            return;
        }
        primes_sender.send(primes_in_segment).ok();
    });

    drop(primes_sender);
    write_handle.join().unwrap();

    if stop_flag.load(Ordering::SeqCst) {
        sender.send(WorkerMessage::Stopped).ok();
    }

    Ok(())
}

pub fn simple_sieve(limit:u64)->Vec<u64>{
    let size = (limit as usize) + 1;
    let mut is_prime = vec![true; size];
    is_prime[0] = false;
    if limit >= 1 {
        is_prime[1] = false;
    }

    let lim_sqrt = integer_sqrt(limit);
    for i in 2..=lim_sqrt as usize {
        if is_prime[i] {
            let mut j = i*i;
            while j <= limit as usize {
                is_prime[j] = false;
                j += i;
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

pub fn segmented_sieve(small_primes:&[u64], low:u64, high:u64, stop_flag: &Arc<AtomicBool>)->Vec<u64> {
    let size=(high - low +1) as usize;
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
        if stop_flag.load(Ordering::SeqCst) {
            return Vec::new();
        }

        if p*p>high {
            break;
        }

        let mut start=if low%p==0 {low} else {low+(p-(low%p))};
        if start<p*p {
            start=p*p;
        }

        let mut j=start;
        while j<=high {
            if stop_flag.load(Ordering::SeqCst) {
                return Vec::new();
            }
            is_prime[(j - low) as usize] = false;
            j+=p;
        }
    }

    let mut primes=Vec::new();
    for i in 0..size {
        if stop_flag.load(Ordering::SeqCst) {
            return primes;
        }
        if is_prime[i] {
            primes.push(low+i as u64);
        }
    }
    primes
}
