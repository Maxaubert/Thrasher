//! Records the default mic and system audio for a few seconds into a folder.
//! Usage: cargo run -p thrasher-core --example record -- <folder> [seconds] [--no-system]

#[cfg(windows)]
fn main() {
    use thrasher_core::capture::{LoopbackMode, LoopbackSource, MicSource, Source};
    use thrasher_core::library::Library;
    use thrasher_core::recorder::{RecorderOptions, Recording};
    use thrasher_core::sidecar::TrackKind;

    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(1).expect("folder");
    let secs: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3.0);
    let system = !args.iter().any(|a| a == "--no-system");

    for d in thrasher_core::wasapi_io::input_devices().unwrap() {
        println!(
            "input: {}{}",
            d.name,
            if d.is_default { " (default)" } else { "" }
        );
    }
    let lib = Library::open(dir).unwrap();
    let mut sources: Vec<(TrackKind, Box<dyn Source>)> =
        vec![(TrackKind::Mic, Box::new(MicSource::new(None)))];
    if system {
        sources.push((
            TrackKind::System,
            Box::new(LoopbackSource {
                mode: LoopbackMode::ExcludeSelf,
            }),
        ));
    }
    let name = "test_clip";
    let rec = Recording::start(&lib, name, sources, RecorderOptions::default()).unwrap();
    for t in rec.tracks() {
        println!("{:?}: {:?}", t.kind, t.format);
    }
    let start = std::time::Instant::now();
    while start.elapsed().as_secs_f64() < secs {
        std::thread::sleep(std::time::Duration::from_millis(500));
        println!(
            "{:.1}s {:?} err={:?}",
            rec.elapsed(),
            rec.levels(),
            rec.error()
        );
    }
    let done = rec.stop().unwrap();
    println!("done: {done:?}");
    for c in lib.list().unwrap() {
        println!("{} {:.3}s system={}", c.name, c.duration, c.has_system);
    }
}

#[cfg(not(windows))]
fn main() {}
