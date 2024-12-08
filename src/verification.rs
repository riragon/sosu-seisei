use std::sync::{mpsc,Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::io::{BufReader, BufRead};
use std::fs::File;
use std::path::Path;
use std::time::Instant;
use crate::miller_rabin::is_bpsw_prime_check;
use crate::app::WorkerMessage;

pub fn verify_primes_bpsw_all_composites(sender:mpsc::Sender<WorkerMessage>,stop_flag:Arc<AtomicBool>) -> Result<(),Box<dyn std::error::Error>> {
    let path=Path::new("primes.txt");
    if !path.exists() {
        sender.send(WorkerMessage::Log("No primes.txt found for verification.\n".to_string())).ok();
        sender.send(WorkerMessage::VerificationDone("No file to verify".to_string())).ok();
        return Ok(())
    }

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
    let mut composites=Vec::new();

    for line in reader.lines() {
        if stop_flag.load(Ordering::SeqCst) {
            sender.send(WorkerMessage::Log("Verification stopped by user.\n".to_string())).ok();
            break;
        }
        let l=line?;
        let n:u64=match l.trim().parse() {
            Ok(v)=>v,
            Err(_)=> {
                composites.push(l.trim().to_string());
                count+=1;
                continue;
            }
        };

        if !is_bpsw_prime_check(n) {
            composites.push(n.to_string());
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
    if composites.is_empty() {
        sender.send(WorkerMessage::VerificationDone("All primes verified as correct".to_string())).ok();
    } else {
        let composite_list = composites.join(", ");
        sender.send(WorkerMessage::VerificationDone(format!("Found composites: {}", composite_list))).ok();
    }

    Ok(())
}
