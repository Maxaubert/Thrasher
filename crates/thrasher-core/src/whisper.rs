//! Whisper through whisper.cpp (whisper-rs), with DTW word timestamps.
//!
//! whisper.cpp's own voice detection is left off: its timestamps are not mapped back
//! to the original timeline in the bundled version. Instead the audio is cut into
//! chunks of at most 28 s here, each chunk is decoded on its own, and its offset is
//! added to the token times.

use crate::models::Heads;
use crate::transcribe::{self, Options, Token, Transcriber, Transcript};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use whisper_rs::{
    DtwMode, DtwModelPreset, DtwParameters, FullParams, SamplingStrategy, WhisperContext,
    WhisperContextParameters,
};

/// whisper.cpp's DTW times come out late. Measured on a 17-minute Norwegian clip
/// against faster-whisper (which Max's video tooling uses and whose timing he is happy
/// with): words started 0.2 s later in the median, and moving them 0.15 s earlier cut
/// the median difference from 0.21 s to 0.08 s. See docs/transcription-benchmark.md.
const DTW_LEAD_SECS: f64 = 0.15;

pub struct WhisperTranscriber {
    ctx: WhisperContext,
}

impl WhisperTranscriber {
    /// Loads a GGML model. `use_gpu` falls back to the CPU when no GPU backend was built.
    /// `heads` must match the model's architecture, or word times come out wrong.
    pub fn load(model: &Path, heads: Heads, use_gpu: bool) -> Result<Self, String> {
        let mut params = WhisperContextParameters::default();
        params.use_gpu(use_gpu);
        // The NB-Whisper models are fine-tunes and share the original models' heads.
        let model_preset = match heads {
            Heads::Tiny => DtwModelPreset::Tiny,
            Heads::TinyEn => DtwModelPreset::TinyEn,
            Heads::Base => DtwModelPreset::Base,
            Heads::BaseEn => DtwModelPreset::BaseEn,
            Heads::Small => DtwModelPreset::Small,
            Heads::SmallEn => DtwModelPreset::SmallEn,
            Heads::Medium => DtwModelPreset::Medium,
            Heads::MediumEn => DtwModelPreset::MediumEn,
            Heads::LargeV2 => DtwModelPreset::LargeV2,
            Heads::LargeV3 => DtwModelPreset::LargeV3,
            Heads::LargeV3Turbo => DtwModelPreset::LargeV3Turbo,
        };
        params.dtw_parameters(DtwParameters {
            mode: DtwMode::ModelPreset { model_preset },
            ..Default::default()
        });
        let ctx = WhisperContext::new_with_params(model, params)
            .map_err(|e| format!("cannot load {}: {e}", model.display()))?;
        Ok(Self { ctx })
    }
}

impl Transcriber for WhisperTranscriber {
    fn transcribe(
        &mut self,
        audio: &[f32],
        options: &Options,
        progress: &mut dyn FnMut(f32),
        cancel: &AtomicBool,
    ) -> Result<Transcript, String> {
        let mut state = self.ctx.create_state().map_err(|e| e.to_string())?;
        let eot = self.ctx.token_eot();
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8)) as i32;
        let chunks = transcribe::chunks(audio);
        let total = audio.len().max(1) as f32;
        let mut words = Vec::new();
        let mut language = options.language.clone();
        for range in chunks {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let mut params = FullParams::new(SamplingStrategy::BeamSearch {
                beam_size: options.beam_size,
                patience: -1.0,
            });
            params.set_language(language.as_deref());
            params.set_n_threads(threads);
            params.set_token_timestamps(true);
            // Each chunk stands alone: this keeps a bad chunk from looping into the next.
            params.set_no_context(true);
            params.set_suppress_blank(true);
            params.set_print_special(false);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);
            state
                .full(params, &audio[range.clone()])
                .map_err(|e| e.to_string())?;
            if language.is_none() {
                language =
                    whisper_rs::get_lang_str(state.full_lang_id_from_state()).map(str::to_string);
            }
            let offset = range.start as f64 / transcribe::RATE as f64;
            for seg in state.as_iter() {
                let seg_end = seg.end_timestamp() as f64 / 100.0;
                let mut tokens = Vec::new();
                for i in 0..seg.n_tokens() {
                    let Some(tok) = seg.get_token(i) else {
                        continue;
                    };
                    if tok.token_id() >= eot {
                        continue;
                    }
                    let data = tok.token_data();
                    let cs = if data.t_dtw >= 0 { data.t_dtw } else { data.t0 };
                    let lead = if data.t_dtw >= 0 { DTW_LEAD_SECS } else { 0.0 };
                    tokens.push(Token {
                        bytes: tok.to_bytes().map(<[u8]>::to_vec).unwrap_or_default(),
                        time: (cs as f64 / 100.0 - lead).max(0.0),
                    });
                }
                words.extend(transcribe::words_from_tokens(&tokens, offset, seg_end));
            }
            progress(range.end as f32 / total);
        }
        // Keep word order monotonic; DTW can place a word a little before its neighbour.
        let mut last = 0.0f64;
        for w in &mut words {
            w.1 = w.1.max(last);
            w.2 = w.2.max(w.1 + 0.02);
            last = w.1;
        }
        Ok(Transcript {
            language,
            words: transcribe::number_words(words),
        })
    }
}
