use std::sync::{mpsc,Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::io::{BufWriter, Write};
use std::fs::OpenOptions;
use std::time::Instant;
use crate::config::Config;
use crate::app::WorkerMessage;

pub fn run_program_old(config: Config, sender:mpsc::Sender<WorkerMessage>, stop_flag:Arc<AtomicBool>) -> Result<(),Box<dyn std::error::Error>> {
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

    let histogram_interval = 50_000u64; // update interval
    let mut next_histogram_mark = prime_min + histogram_interval;
    let mut current_interval_count = 0u64;

    let start_time = Instant::now();
    let mut processed = 0u64;
    let total_range = prime_max - prime_min + 1;

    let segment_size = config.segment_size as usize;
    let mut low = prime_min;
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
            found_count+=1;
            sender.send(WorkerMessage::FoundPrimeIndex(p,found_count)).ok();
            current_interval_count+=1;
        }

        let segment_range = high - low +1;
        processed += segment_range;
        let progress = processed as f64 / total_range as f64;

        let elapsed = start_time.elapsed().as_secs_f64();
        let eta= if progress>0.0 {
            let total_time=elapsed/progress;
            let remaining= total_time - elapsed;
            format!("ETA: {:.2} sec",remaining)
        } else {
            "Calculating...".to_string()
        };

        sender.send(WorkerMessage::Progress { current:processed, total:total_range}).ok();
        sender.send(WorkerMessage::Eta(eta)).ok();

        if processed >= next_histogram_mark || high == prime_max {
            let _=sender.send(WorkerMessage::HistogramUpdate {
                histogram: vec![(processed,current_interval_count)],
            });
            current_interval_count=0;
            next_histogram_mark = processed + histogram_interval;
        }

        low=high+1;
    }

    writer.flush()?;

    let _=sender.send(WorkerMessage::HistogramUpdate {
        histogram: vec![]
    });

    sender.send(WorkerMessage::Log(format!("Finished old method. Total primes found: {}",found_count))).ok();
    sender.send(WorkerMessage::Done).ok();
    Ok(())
}

pub fn simple_sieve(limit:u64)->Vec<u64>{
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

pub fn segmented_sieve(small_primes:&[u64], low:u64, high:u64)->Vec<u64> {
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
