#![cfg(feature = "loadtest")]

use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(name = "kv-loadtest", about = "Load test for saku KV sync endpoints")]
struct Args {
    /// Server base URL (e.g. http://localhost:8080)
    #[arg(long)]
    url: String,

    /// Login email
    #[arg(long)]
    email: String,

    /// Login password
    #[arg(long)]
    password: String,

    /// Number of concurrent clients
    #[arg(long, default_value = "10")]
    clients: usize,

    /// Test duration in seconds
    #[arg(long, default_value = "30")]
    duration: u64,

    /// Bytes per entry payload
    #[arg(long, default_value = "256")]
    payload_size: usize,

    /// Entries per batch PUT
    #[arg(long, default_value = "10")]
    batch_size: usize,

    /// Workload type: write-only, read-only, mixed
    #[arg(long, default_value = "mixed")]
    workload: String,
}

#[derive(Serialize)]
struct LoginRequest {
    email: String,
    password: String,
    device_id: String,
    device_name: String,
}

#[derive(Deserialize)]
struct LoginResponse {
    access_token: String,
}

#[derive(Serialize)]
struct BatchPutRequest {
    entries: Vec<BatchPutEntry>,
}

#[derive(Serialize)]
struct BatchPutEntry {
    key: String,
    blob: String,
}

#[derive(Deserialize)]
struct GetEntriesResponse {
    cookie: String,
    has_more: bool,
    entries: Vec<serde_json::Value>,
}

struct WorkerResult {
    write_latencies: Vec<Duration>,
    read_latencies: Vec<Duration>,
    write_errors: u64,
    read_errors: u64,
    bytes_written: u64,
    bytes_read: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    println!("=== KV Load Test ===");
    println!("URL:          {}", args.url);
    println!("Clients:      {}", args.clients);
    println!("Duration:     {}s", args.duration);
    println!("Payload:      {} bytes", args.payload_size);
    println!("Batch size:   {}", args.batch_size);
    println!("Workload:     {}", args.workload);
    println!();

    // Login once per client
    let http = reqwest::Client::new();
    let mut tokens = Vec::with_capacity(args.clients);

    println!("Logging in {} clients...", args.clients);
    for i in 0..args.clients {
        let device_id = format!("loadtest-{i}-{}", uuid::Uuid::new_v4());
        let resp = http
            .post(format!("{}/api/v1/auth/login", args.url))
            .json(&LoginRequest {
                email: args.email.clone(),
                password: args.password.clone(),
                device_id,
                device_name: format!("loadtest-client-{i}"),
            })
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await?;
            anyhow::bail!("Login failed for client {i}: {status} {body}");
        }

        let login: LoginResponse = resp.json().await?;
        tokens.push(login.access_token);
    }
    println!("All clients logged in.\n");

    let shared = Arc::new(SharedConfig {
        url: args.url.clone(),
        payload_size: args.payload_size,
        batch_size: args.batch_size,
        workload: args.workload.clone(),
        duration: Duration::from_secs(args.duration),
    });

    // Spawn workers
    let mut handles = Vec::with_capacity(args.clients);
    let start = Instant::now();

    for (i, token) in tokens.into_iter().enumerate() {
        let shared = shared.clone();
        let http = http.clone();
        handles.push(tokio::spawn(async move {
            run_worker(i, &http, &token, &shared, start).await
        }));
    }

    // Collect results
    let mut all_write_latencies = Vec::new();
    let mut all_read_latencies = Vec::new();
    let mut total_write_errors = 0u64;
    let mut total_read_errors = 0u64;
    let mut total_bytes_written = 0u64;
    let mut total_bytes_read = 0u64;

    for handle in handles {
        let result = handle.await?;
        all_write_latencies.extend(result.write_latencies);
        all_read_latencies.extend(result.read_latencies);
        total_write_errors += result.write_errors;
        total_read_errors += result.read_errors;
        total_bytes_written += result.bytes_written;
        total_bytes_read += result.bytes_read;
    }

    let elapsed = start.elapsed();

    // Print report
    println!("=== Results ({:.1}s elapsed) ===\n", elapsed.as_secs_f64());

    let total_requests = all_write_latencies.len() + all_read_latencies.len();
    let total_errors = total_write_errors + total_read_errors;
    println!(
        "Total requests: {} ({} success, {} errors)",
        total_requests as u64 + total_errors,
        total_requests,
        total_errors
    );
    println!(
        "Throughput:     {:.1} req/s",
        total_requests as f64 / elapsed.as_secs_f64()
    );
    println!(
        "Data:           {:.1} KB written, {:.1} KB read",
        total_bytes_written as f64 / 1024.0,
        total_bytes_read as f64 / 1024.0,
    );
    println!();

    if !all_write_latencies.is_empty() {
        all_write_latencies.sort();
        println!("Write latency ({} requests):", all_write_latencies.len());
        print_percentiles(&all_write_latencies);
    }

    if !all_read_latencies.is_empty() {
        all_read_latencies.sort();
        println!("Read latency ({} requests):", all_read_latencies.len());
        print_percentiles(&all_read_latencies);
    }

    Ok(())
}

struct SharedConfig {
    url: String,
    payload_size: usize,
    batch_size: usize,
    workload: String,
    duration: Duration,
}

async fn run_worker(
    id: usize,
    http: &reqwest::Client,
    token: &str,
    config: &SharedConfig,
    start: Instant,
) -> WorkerResult {
    let mut result = WorkerResult {
        write_latencies: Vec::new(),
        read_latencies: Vec::new(),
        write_errors: 0,
        read_errors: 0,
        bytes_written: 0,
        bytes_read: 0,
    };

    let tool = "tdo";
    let mut cookie = "0".to_string();
    let mut write_counter = 0u64;

    // Generate a reusable payload
    let payload_bytes: Vec<u8> = (0..config.payload_size)
        .map(|i| (i % 256) as u8)
        .collect();
    let payload_b64 = BASE64.encode(&payload_bytes);
    let batch_payload_size = config.payload_size * config.batch_size;

    while start.elapsed() < config.duration {
        let do_write = match config.workload.as_str() {
            "write-only" => true,
            "read-only" => false,
            _ => {
                // mixed: 70% write, 30% read
                write_counter % 10 < 7
            }
        };

        if do_write {
            let entries: Vec<BatchPutEntry> = (0..config.batch_size)
                .map(|j| BatchPutEntry {
                    key: format!("w{id}-{write_counter}-{j}"),
                    blob: payload_b64.clone(),
                })
                .collect();

            let req_start = Instant::now();
            let resp = http
                .put(format!("{}/api/v1/kv/{tool}", config.url))
                .header("authorization", format!("Bearer {token}"))
                .json(&BatchPutRequest { entries })
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    result.write_latencies.push(req_start.elapsed());
                    result.bytes_written += batch_payload_size as u64;
                }
                _ => {
                    result.write_errors += 1;
                }
            }
            write_counter += 1;
        } else {
            let req_start = Instant::now();
            let resp = http
                .get(format!(
                    "{}/api/v1/kv/{tool}?cookie={}&limit=100",
                    config.url, cookie
                ))
                .header("authorization", format!("Bearer {token}"))
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    result.read_latencies.push(req_start.elapsed());
                    if let Ok(body) = r.json::<GetEntriesResponse>().await {
                        result.bytes_read += body
                            .entries
                            .iter()
                            .filter_map(|e| e["blob"].as_str())
                            .map(|b| b.len() as u64)
                            .sum::<u64>();
                        if !body.has_more {
                            cookie = body.cookie;
                        }
                    }
                }
                _ => {
                    result.read_errors += 1;
                }
            }
            write_counter += 1;
        }
    }

    result
}

fn print_percentiles(latencies: &[Duration]) {
    let p = |pct: f64| -> Duration {
        let idx = ((latencies.len() as f64 * pct / 100.0) as usize).min(latencies.len() - 1);
        latencies[idx]
    };

    println!("  p50:  {:>8.1}ms", p(50.0).as_secs_f64() * 1000.0);
    println!("  p90:  {:>8.1}ms", p(90.0).as_secs_f64() * 1000.0);
    println!("  p95:  {:>8.1}ms", p(95.0).as_secs_f64() * 1000.0);
    println!("  p99:  {:>8.1}ms", p(99.0).as_secs_f64() * 1000.0);
    println!("  max:  {:>8.1}ms", latencies.last().unwrap().as_secs_f64() * 1000.0);
    println!();
}
