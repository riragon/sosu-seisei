use std::sync::{mpsc,Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::io::{BufWriter, Write};
use std::fs::OpenOptions;
use std::time::Instant;
use crate::config::{Config, OutputFormat};
use crate::app::WorkerMessage;
use rayon::prelude::*;
use bitvec::prelude::*;

/// Compute integer sqrt: floor(sqrt(n))
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

    // Generate small primes up to root
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

    // Choose file name based on output format
    let file_name = match output_format {
        OutputFormat::Text => "primes.txt",
        OutputFormat::CSV  => "primes.csv",
        OutputFormat::JSON => "primes.json",
    };

    let write_handle = std::thread::spawn(move || {
        let file = OpenOptions::new().create(true).truncate(true).write(true).open(file_name).unwrap();
        let mut writer = BufWriter::with_capacity(writer_buffer_size, file);

        let mut found_count=0u64;
        let mut processed = 0u64;

        // For JSON format, start the array
        if let OutputFormat::JSON = output_format {
            write!(writer, "[").unwrap();
        }

        let mut first_item = true; // used for JSON formatting

        for primes_in_segment in primes_receiver {
            match output_format {
                OutputFormat::Text => {
                    // One prime per line
                    for p in primes_in_segment {
                        writeln!(writer,"{}",p).unwrap();
                        found_count+=1;
                        sender_clone.send(WorkerMessage::FoundPrimeIndex(p,found_count)).ok();
                    }
                },
                OutputFormat::CSV => {
                    // Comma-separated line
                    let line: String = primes_in_segment.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",");
                    writeln!(writer,"{}",line).unwrap();
                    for p in primes_in_segment {
                        found_count+=1;
                        sender_clone.send(WorkerMessage::FoundPrimeIndex(p,found_count)).ok();
                    }
                },
                OutputFormat::JSON => {
                    // JSON array: [2,3,5,7,...]
                    for p in primes_in_segment {
                        if !first_item {
                            write!(writer,",{}", p).unwrap();
                        } else {
                            write!(writer,"{}", p).unwrap();
                            first_item = false;
                        }
                        found_count+=1;
                        sender_clone.send(WorkerMessage::FoundPrimeIndex(p,found_count)).ok();
                    }
                },
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
        }

        // If JSON, close the array
        if let OutputFormat::JSON = output_format {
            write!(writer, "]").unwrap();
        }

        writer.flush().unwrap();

        sender_clone.send(WorkerMessage::Log(format!("Finished old method. Total primes found: {}",found_count))).ok();
        sender_clone.send(WorkerMessage::Done).ok();
    });

    segments.into_par_iter().for_each(|(low, high)| {
        if stop_flag_clone.load(Ordering::SeqCst) {
            return;
        }
        let primes_in_segment = segmented_sieve(&small_primes, low, high);
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
    let mut is_prime: BitVec = BitVec::repeat(true, (limit as usize) + 1);
    is_prime.set(0, false);
    if limit >= 1 {
        is_prime.set(1, false);
    }

    let lim_sqrt = integer_sqrt(limit);
    for i in 2..=lim_sqrt as usize {
        if is_prime[i] {
            for j in ((i*i)..=limit as usize).step_by(i) {
                is_prime.set(j, false);
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

pub fn segmented_sieve(small_primes:&[u64], low:u64, high:u64)->Vec<u64> {
    let size=(high - low +1) as usize;
    let mut is_prime: BitVec = BitVec::repeat(true, size);

    // Handle low edge cases
    if low == 0 {
        if size > 0 {
            is_prime.set(0, false);
        }
        if size > 1 {
            is_prime.set(1, false);
        }
    } else if low == 1 {
        is_prime.set(0, false);
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
            is_prime.set((j - low) as usize, false);
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
