//! Transcribes a clip and writes words and silences into its sidecar.
//! Usage: cargo run --release -p thrasher-core --features whisper-cuda --example transcribe -- <model.bin> <clip.wav> [language|auto]

#[cfg(feature = "whisper")]
fn main() {
    use std::path::Path;
    use std::sync::atomic::AtomicBool;
    use std::time::Instant;
    use thrasher_core::sidecar::Sidecar;
    use thrasher_core::transcribe::{self, Options, Transcriber};
    use thrasher_core::wav;
    use thrasher_core::whisper::WhisperTranscriber;

    let args: Vec<String> = std::env::args().collect();
    let model = Path::new(&args[1]);
    let clip = Path::new(&args[2]);
    let language = match args.get(3).map(String::as_str) {
        Some("auto") => None,
        Some(l) => Some(l.to_string()),
        None => Some("no".to_string()),
    };

    let t = Instant::now();
    let (spec, samples) = wav::read_f32(clip).expect("read wav");
    let mono = transcribe::to_mono(&samples, spec.channels);
    let audio = transcribe::resample(&mono, spec.sample_rate, transcribe::RATE);
    let duration = audio.len() as f64 / transcribe::RATE as f64;
    println!(
        "audio {duration:.1}s, prepared in {:.1}s",
        t.elapsed().as_secs_f64()
    );

    let t = Instant::now();
    let mut engine = WhisperTranscriber::load(model, thrasher_core::models::Heads::LargeV3, true)
        .expect("load model");
    println!("model loaded in {:.1}s", t.elapsed().as_secs_f64());

    let t = Instant::now();
    let options = Options {
        language,
        ..Default::default()
    };
    let mut last = 0.0;
    let result = engine
        .transcribe(
            &audio,
            &options,
            &mut |p| {
                if p - last >= 0.1 || p >= 1.0 {
                    last = p;
                    println!("{:.0}%", p * 100.0);
                }
            },
            &AtomicBool::new(false),
        )
        .expect("transcribe");
    let secs = t.elapsed().as_secs_f64();
    println!(
        "transcribed in {secs:.1}s ({:.0}x realtime), {} words, language {:?}",
        duration / secs,
        result.words.len(),
        result.language
    );

    let silences = transcribe::silences(&result.words, duration, transcribe::DEFAULT_MIN_SILENCE);
    let text = transcribe::render_text(&result.words, &silences);
    println!("{}", text.chars().take(600).collect::<String>());

    let sc_path = clip.with_extension("json");
    let mut sc = Sidecar::load(&sc_path).unwrap_or_else(|_| Sidecar {
        file: clip.file_name().unwrap().to_string_lossy().into_owned(),
        duration,
        ..Default::default()
    });
    sc.language = result.language;
    sc.words = result.words;
    sc.silences = silences;
    let out = std::env::var("THRASHER_OUT")
        .map(std::path::PathBuf::from)
        .unwrap_or(sc_path);
    sc.save(&out).expect("save sidecar");
    println!("wrote {}", out.display());
}

#[cfg(not(feature = "whisper"))]
fn main() {
    eprintln!("build with --features whisper or whisper-cuda");
}
