use std::io;
use std::path::PathBuf;

use crate::executor::ExecutionResult;

/// Returns the directory where execution logs are stored.
pub fn log_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("saku")
        .join("wrk")
        .join("logs")
}

/// Metadata for a single log entry (parsed from filename).
pub struct LogEntry {
    pub task_number: u64,
    pub timestamp: String,
    pub path: PathBuf,
}

/// Write an execution log to disk.
///
/// Returns the path to the written log file.
pub fn write_log(task_number: u64, result: &ExecutionResult) -> Result<PathBuf, io::Error> {
    let dir = log_dir();
    std::fs::create_dir_all(&dir)?;

    let timestamp = jiff::Zoned::now()
        .strftime("%Y%m%d-%H%M%S")
        .to_string();
    let filename = format!("{}-{}.log", task_number, timestamp);
    let path = dir.join(&filename);

    let mut content = String::new();
    content.push_str(&format!(
        "=== wrk execution log ===\n\
         Task:     #{}\n\
         Duration: {:.1}s\n\
         Exit:     {}\n\
         ===\n\n",
        task_number,
        result.duration.as_secs_f64(),
        result.exit_code,
    ));

    if !result.stdout.is_empty() {
        content.push_str("--- stdout ---\n");
        content.push_str(&result.stdout);
        if !result.stdout.ends_with('\n') {
            content.push('\n');
        }
    }

    if !result.stderr.is_empty() {
        content.push_str("--- stderr ---\n");
        content.push_str(&result.stderr);
        if !result.stderr.ends_with('\n') {
            content.push('\n');
        }
    }

    std::fs::write(&path, &content)?;

    Ok(path)
}

/// Read the most recent log for a given task number.
pub fn read_log(task_number: u64) -> Result<String, io::Error> {
    let path = find_latest_log(task_number)?;
    std::fs::read_to_string(&path)
}

/// List all log entries, sorted most recent first.
pub fn list_logs() -> Result<Vec<LogEntry>, io::Error> {
    let dir = log_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut entries: Vec<LogEntry> = std::fs::read_dir(&dir)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().to_string();
            parse_log_filename(&name, entry.path())
        })
        .collect();

    // Sort by timestamp descending (most recent first)
    entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(entries)
}

fn find_latest_log(task_number: u64) -> Result<PathBuf, io::Error> {
    let dir = log_dir();
    if !dir.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("No logs found for task #{}", task_number),
        ));
    }

    let prefix = format!("{}-", task_number);
    let mut matching: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&prefix) && name.ends_with(".log") {
                Some(entry.path())
            } else {
                None
            }
        })
        .collect();

    matching.sort();

    matching.last().cloned().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("No logs found for task #{}", task_number),
        )
    })
}

fn parse_log_filename(name: &str, path: PathBuf) -> Option<LogEntry> {
    let stem = name.strip_suffix(".log")?;
    let dash_pos = stem.find('-')?;
    let task_number = stem[..dash_pos].parse::<u64>().ok()?;
    let timestamp = stem[dash_pos + 1..].to_string();
    Some(LogEntry {
        task_number,
        timestamp,
        path,
    })
}
