//! The local AI runs in the `thrasher-ask` worker process; this module starts it and
//! passes its answer on as it is written.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use thrasher_core::ai::Job;

/// The worker sits next to the app, and only exists in builds with local AI.
pub fn worker_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let worker = exe.with_file_name("thrasher-ask.exe");
    worker.exists().then_some(worker)
}

/// Runs `job`. `on_progress` gets 0..1 while the earlier steps run, `on_text` the
/// answer so far while the last step writes it. Returns the whole answer.
pub fn run(
    worker: &Path,
    job: &Job,
    on_progress: &mut dyn FnMut(f32),
    on_text: &mut dyn FnMut(&str),
) -> Result<String, String> {
    let mut cmd = Command::new(worker);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console window flashes up.
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let input = serde_json::to_string(job).map_err(|e| e.to_string())?;
    {
        let mut stdin = child.stdin.take().ok_or("no stdin")?;
        stdin
            .write_all(input.as_bytes())
            .map_err(|e| e.to_string())?;
    }
    let mut stderr = child.stderr.take().ok_or("no stderr")?;
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let (mut text, mut done) = (String::new(), false);
    for line in BufReader::new(child.stdout.take().ok_or("no stdout")?).lines() {
        let line = line.map_err(|e| e.to_string())?;
        if let Some(piece) = line
            .strip_prefix("token ")
            .and_then(|t| serde_json::from_str::<String>(t).ok())
        {
            text.push_str(&piece);
            on_text(&text);
        } else if let Some(p) = line
            .strip_prefix("progress ")
            .and_then(|p| p.parse::<f32>().ok())
        {
            on_progress(p);
        } else if line == "done" {
            done = true;
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let err = err_reader.join().unwrap_or_default();
    if status.success() && done {
        Ok(text)
    } else {
        let last = err
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("the AI worker stopped");
        Err(last.to_string())
    }
}
