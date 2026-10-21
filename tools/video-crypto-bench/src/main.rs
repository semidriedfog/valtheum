//! Synthetic benchmark only. The prototype format is NOT a shipping format.
//! Uses temporary random keys, never opens a user library or vault.key.
#![allow(dead_code)]
#[path = "../../../src/archive_crypto.rs"]
mod current;
mod prototype;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    time::Instant,
};
use zeroize::Zeroizing;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Counters {
    read_ops: u64,
    write_ops: u64,
    other_ops: u64,
    read: u64,
    write: u64,
    other: u64,
}
fn counters() -> Result<Counters> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn GetProcessIoCounters(handle: *mut std::ffi::c_void, out: *mut Counters) -> i32;
    }
    let mut c = Counters::default();
    ensure!(
        unsafe { GetProcessIoCounters(GetCurrentProcess(), &mut c) } != 0,
        "I/O counters: {}",
        std::io::Error::last_os_error()
    );
    Ok(c)
}
fn measurement(start: Instant, before: Counters) -> Result<Value> {
    let seconds = start.elapsed().as_secs_f64();
    let after = counters()?;
    Ok(
        json!({"seconds":seconds,"read_bytes":after.read-before.read,"write_bytes":after.write-before.write,
        "read_operations":after.read_ops-before.read_ops,"write_operations":after.write_ops-before.write_ops}),
    )
}
fn fixture(path: &Path, size: u64) -> Result<String> {
    let mut out = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut buf = Zeroizing::new(vec![0; 1024 * 1024]);
    let mut sha = Sha256::new();
    let mut left = size;
    while left > 0 {
        let n = left.min(buf.len() as u64) as usize;
        getrandom::fill(&mut buf[..n]).map_err(|e| anyhow::anyhow!("{e}"))?;
        sha.update(&buf[..n]);
        out.write_all(&buf[..n])?;
        left -= n as u64;
    }
    out.sync_all()?;
    Ok(current::hex(&sha.finalize()))
}
fn seek_test(
    path: &Path,
    source: &Path,
    key: &[u8; 32],
    size: u64,
    baseline: bool,
) -> Result<Value> {
    let length = (1024 * 1024u64).min(size);
    let mut values = Vec::new();
    for offset in [0, size / 4, size / 2, 3 * size / 4, size - length] {
        let offset = offset.min(size - length);
        let before = counters()?;
        let start = Instant::now();
        let bytes = if baseline {
            let mut reader = current::Reader::open(path, key)?;
            reader.seek(SeekFrom::Start(offset))?;
            let mut bytes = vec![0; length as usize];
            reader.read_exact(&mut bytes)?;
            bytes
        } else {
            prototype::read_range(path, key, offset, length as usize)?
        };
        let mut m = measurement(start, before)?;
        // Reference read intentionally outside the measured interval.
        let mut reference = File::open(source)?;
        reference.seek(SeekFrom::Start(offset))?;
        let mut expected = vec![0; length as usize];
        reference.read_exact(&mut expected)?;
        ensure!(bytes == expected, "range plaintext mismatch");
        m["offset"] = json!(offset);
        m["returned_bytes"] = json!(length);
        values.push(m);
    }
    Ok(json!(values))
}

fn main() -> Result<()> {
    let size = std::env::args()
        .nth(1)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(1_000_000_000u64);
    let rounds = std::env::args()
        .nth(2)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(3usize);
    ensure!(
        (1..=2_000_000_000).contains(&size) && (1..=5).contains(&rounds),
        "bounded benchmark: up to 2 GB, 1-5 rounds"
    );
    let production = std::env::args().nth(3).as_deref() == Some("--production");
    let base = std::env::current_dir()?.join("target/video-crypto-bench-data");
    fs::create_dir_all(&base)?;
    current::check_space(&base, size * 3 + 64 * 1024 * 1024)?;
    let run = base.join(current::hex(&current::random::<16>()?));
    fs::create_dir(&run)?;
    let source = run.join("synthetic-random.bin");
    let output = run.join("encrypted-test.bin");
    let key = Zeroizing::new(current::random::<32>()?);
    println!(
        "Synthetic benchmark: {} bytes, {} rounds; {}",
        size,
        rounds,
        run.display()
    );
    let start = Instant::now();
    let expected = fixture(&source, size)?;
    let generation_seconds = start.elapsed().as_secs_f64();
    let mut results = Vec::new();
    // Rotate order between rounds to reduce a fixed-order cache/temperature bias.
    for round in 0..rounds {
        for i in 0..if production { 1 } else { 3 } {
            let mode = if production { 3 } else { (i + round) % 3 };
            let name = [
                "current_vta1_64k",
                "onepass_prototype_64k",
                "onepass_prototype_1m",
                "production_vtv2_64k",
            ][mode];
            let before = counters()?;
            let started = Instant::now();
            let mut transitions = Vec::new();
            let (encrypt_seconds, verify_seconds) = if mode == 0 || mode == 3 {
                let mut last = String::new();
                let metadata = current::encrypt(
                    &mut File::open(&source)?,
                    &output,
                    if mode == 3 {
                        "synthetic-video.mp4"
                    } else {
                        "synthetic-video.zip"
                    },
                    &key,
                    |phase, _, _| {
                        if phase != last {
                            transitions.push(
                                json!({"phase":phase,"at_seconds":started.elapsed().as_secs_f64()}),
                            );
                            last = phase.into();
                        }
                        Ok(())
                    },
                )?;
                ensure!(metadata.sha256 == expected, "baseline digest mismatch");
                let at = transitions
                    .iter()
                    .find(|v| v["phase"] == "사본 검증")
                    .context("missing verify phase")?["at_seconds"]
                    .as_f64()
                    .unwrap();
                (at, started.elapsed().as_secs_f64() - at)
            } else {
                let chunk = if mode == 1 { 65536 } else { 1048576 };
                prototype::encrypt(&source, &output, &key, chunk)?;
                let encryption = started.elapsed().as_secs_f64();
                let verification = Instant::now();
                let digest = prototype::verify(&output, &key)?;
                ensure!(digest == expected, "prototype digest mismatch");
                (encryption, verification.elapsed().as_secs_f64())
            };
            let mut measured = measurement(started, before)?;
            measured["mode"] = json!(name);
            measured["round"] = json!(round + 1);
            measured["encryption_including_prehash_seconds"] = json!(encrypt_seconds);
            measured["verification_seconds"] = json!(verify_seconds);
            measured["phase_transitions"] = json!(transitions);
            measured["output_bytes"] = json!(output.metadata()?.len());
            measured["source_mb_per_second"] =
                json!(size as f64 / 1e6 / measured["seconds"].as_f64().unwrap());
            measured["range_reads"] =
                seek_test(&output, &source, &key, size, mode == 0 || mode == 3)?;
            println!("{} round {}: {:.3}s total, {:.3}s prepare/encrypt, {:.3}s verify, read {:.3} GB / write {:.3} GB",name,round+1,measured["seconds"].as_f64().unwrap(),encrypt_seconds,verify_seconds,measured["read_bytes"].as_u64().unwrap()as f64/1e9,measured["write_bytes"].as_u64().unwrap()as f64/1e9);
            results.push(measured);
            // Only the exact synthetic output created by this process is removed.
            fs::remove_file(&output)?;
        }
    }
    let report = json!({"input_bytes":size,"rounds":rounds,"input":"synthetic incompressible random bytes, not a real video or archive","fixture_generation_seconds_excluded":generation_seconds,
        "aes_cpu_support":std::is_x86_feature_detected!("aes"),"pclmul_cpu_support":std::is_x86_feature_detected!("pclmulqdq"),
        "source_directory":run,"cache_policy":"buffered Windows I/O; recently generated source, warm/mixed OS caches; no cache flush; output sync_all included",
        "io_measurement":"GetProcessIoCounters: process I/O transfer bytes, not physical disk/NAND traffic; range reference reads excluded",
        "exclusions":["input creation","password KDF/unlock","database/UI","video decoding/transcoding"],
        "prototype_warning":if production { "production VTV2 implementation; benchmark is not a security review" } else { "one-pass authenticated-footer prototype, NOT the shipped VTA1 format or a production security review" },
        "baseline_stage_note":"phase boundary inferred from first progress callback; VTA1 includes original SHA-256 prepass; production VTV2 hashes while encrypting",
        "results":results});
    fs::write(
        run.join("results.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    fs::remove_file(&source)?;
    println!(
        "Report: {} (generated data files removed)",
        run.join("results.json").display()
    );
    Ok(())
}
