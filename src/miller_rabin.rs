use std::sync::{mpsc,Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::io::{BufWriter, Write};
use std::fs::OpenOptions;
use std::time::Instant;
use num_bigint::BigUint;
use num_traits::{Zero, ToPrimitive, One};
use crate::config::Config;
use crate::app::WorkerMessage;

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

    for &a in &MR_BASES_64 {
        if a==0 || a>=n {continue;}
        if !miller_rabin_check(n,a,d,r) {
            return false;
        }
    }
    true
}

fn jacobi(mut a: i64, mut n: i64) -> i32 {
    if n <= 0 || n % 2 == 0 {
        return 0;
    }
    let mut result = 1;
    a = a % n;
    while a != 0 {
        while a % 2 == 0 {
            a /= 2;
            let r = n % 8;
            if r == 3 || r == 5 {
                result = -result;
            }
        }
        let temp = a;
        a = n;
        n = temp;
        if a % 4 == 3 && n % 4 == 3 {
            result = -result;
        }
        a = a % n;
    }
    if n == 1 { result } else { 0 }
}

fn lucas_pp_test(n:&BigUint)->bool {
    if n < &BigUint::from(2u64) {
        return false;
    }

    let n_u64 = match n.to_u64_digits().get(0) {
        Some(&x)=>x,
        None=>return false,
    };

    if n_u64 > i64::MAX as u64 {
        return false;
    }
    let n_i64 = n_u64 as i64;

    let mut d=5i64;
    loop {
        let j=jacobi(d,n_i64);
        if j==-1 {
            break;
        }
        if d>0 {
            d=-(d+2);
        } else {
            d=-(d-2);
        }
    }

    true
}

fn is_bpsw_prime(n:&BigUint)->bool {
    if n<&BigUint::from(2u64) {return false;}
    if n==&BigUint::from(2u64) {return true;}
    let two=BigUint::from(2u64);
    if n%&two==BigUint::zero() {
        return false;
    }

    let n_u64 = match n.to_u64_digits().get(0) {
        Some(&x)=>x,
        None=> {
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

pub fn is_bpsw_prime_check(n:u64)->bool {
    if n<2 {return false;}
    if n==2 {return true;}
    if n%2==0 {return false;}
    let big = BigUint::from(n);
    is_bpsw_prime(&big)
}

pub fn run_program_new(config: Config, sender:mpsc::Sender<WorkerMessage>, stop_flag:Arc<AtomicBool>) -> Result<(),Box<dyn std::error::Error>> {
    sender.send(WorkerMessage::Log("Running new method (Miller-Rabin)".to_string())).ok();

    let prime_min = config.prime_min.parse::<BigUint>()?;
    let prime_max = config.prime_max.parse::<BigUint>()?;

    let file = OpenOptions::new().create(true).truncate(true).write(true).open("primes.txt")?;
    let mut writer = BufWriter::with_capacity(config.writer_buffer_size,file);

    let one=BigUint::one();
    let mut current=prime_min.clone();
    let mut found_count=0u64;

    let range_opt = (&prime_max - &prime_min).to_f64();
    let start_time=Instant::now();

    let histogram_interval=50_000_u64;
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
            if &current % &two==BigUint::zero() {
                false
            } else {
                is_bpsw_prime(&current)
            }
        };

        if is_actually_prime {
            writeln!(writer,"{}",current)?;
            found_count+=1;
            current_interval_count+=1;
            sender.send(WorkerMessage::FoundPrimeIndex(current_u64,found_count)).ok();
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

                let processed_bi = BigUint::from(processed);
                if processed_bi>=next_histogram_mark || &current==&prime_max {
                    let _=sender.send(WorkerMessage::HistogramUpdate {
                        histogram: vec![(processed,current_interval_count)],
                    });
                    current_interval_count=0;
                    next_histogram_mark = &next_histogram_mark+BigUint::from(histogram_interval);
                }
            }
        }

        current=&current+&one;
    }

    writer.flush()?;

    let _=sender.send(WorkerMessage::HistogramUpdate {
        histogram: vec![]
    });

    sender.send(WorkerMessage::Log(format!("Finished new method. Total primes found: {}",found_count))).ok();
    sender.send(WorkerMessage::Done).ok();
    Ok(())
}
