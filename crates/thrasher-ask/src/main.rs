//! Local AI worker. The app starts it for each job, so the model (a few GB) leaves
//! memory when the job ends, and a crash inside llama.cpp cannot touch a recording.
//!
//! Reads one JSON job from stdin:
//! `{"model": "<gguf path>", "max_tokens": 700, "steps": [[{"role", "content"}, ...], ...]}`
//! Each step is a chat. A `{prev}` in a later step's text is replaced by the answers of
//! all earlier steps, one per line, so a long transcript can be summarised in parts and
//! then as a whole. Prints `progress <0..1>` after each earlier step, the last step's
//! answer as `token <json string>` lines while it is written, then `done`. Errors go to
//! stderr with exit code 1.

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{LlamaChatMessage, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use serde::Deserialize;
use std::io::{Read, Write};
use std::num::NonZeroU32;

#[derive(Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct Job {
    model: String,
    max_tokens: u32,
    steps: Vec<Vec<Message>>,
}

/// Tokens fed to the model per decode call.
const BATCH: usize = 512;

fn answer(
    backend: &LlamaBackend,
    model: &LlamaModel,
    chat: &[Message],
    max_tokens: u32,
    on_piece: &mut dyn FnMut(&str),
) -> Result<String, String> {
    let messages = chat
        .iter()
        .map(|m| LlamaChatMessage::new(m.role.clone(), m.content.clone()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let template = model.chat_template(None).map_err(|e| e.to_string())?;
    let prompt = model
        .apply_chat_template(&template, &messages, true)
        .map_err(|e| format!("chat template: {e}"))?;
    let vocab = model.vocab();
    // The template already holds the start token.
    let tokens = vocab.tokenize(prompt.as_bytes(), false, true);
    let n_ctx = (tokens.len() as u32 + max_tokens + 16).max(2048);
    let params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(n_ctx))
        .with_n_batch(BATCH as u32);
    let mut ctx = model
        .new_context(backend, params)
        .map_err(|e| e.to_string())?;

    let mut batch = LlamaBatch::new(BATCH, 1);
    let mut pos = 0i32;
    for (k, part) in tokens.chunks(BATCH).enumerate() {
        batch.clear();
        let last_part = (k + 1) * BATCH >= tokens.len();
        for (i, t) in part.iter().enumerate() {
            let last = last_part && i + 1 == part.len();
            batch.add(*t, pos, &[0], last).map_err(|e| e.to_string())?;
            pos += 1;
        }
        ctx.decode(&mut batch).map_err(|e| e.to_string())?;
    }

    // Low temperature: summaries and answers should stick to the text.
    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::temp(0.2),
        LlamaSampler::min_p(0.05, 1),
        LlamaSampler::dist(7),
    ]);
    let mut out = String::new();
    let mut bytes: Vec<u8> = Vec::new();
    for _ in 0..max_tokens {
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        sampler.accept(token);
        if vocab.is_eog(token) {
            break;
        }
        bytes.extend(vocab.token_to_piece(token, false, None));
        // Pass on only whole UTF-8 characters; a letter like ø can span two tokens.
        let valid = match std::str::from_utf8(&bytes) {
            Ok(_) => bytes.len(),
            Err(e) => e.valid_up_to(),
        };
        if valid > 0 {
            let piece = String::from_utf8_lossy(&bytes[..valid]).into_owned();
            bytes.drain(..valid);
            on_piece(&piece);
            out.push_str(&piece);
        }
        batch.clear();
        batch
            .add(token, pos, &[0], true)
            .map_err(|e| e.to_string())?;
        pos += 1;
        ctx.decode(&mut batch).map_err(|e| e.to_string())?;
    }
    Ok(out.trim().to_string())
}

fn run() -> Result<(), String> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| e.to_string())?;
    let job: Job = serde_json::from_str(&input).map_err(|e| format!("bad job: {e}"))?;
    if job.steps.is_empty() {
        return Err("nothing to do".into());
    }
    let mut backend = LlamaBackend::init().map_err(|e| e.to_string())?;
    backend.void_logs();
    // Every layer on the GPU when there is one; llama.cpp keeps them on the CPU otherwise.
    let params = LlamaModelParams::default().with_n_gpu_layers(999);
    let model = LlamaModel::load_from_file(&backend, &job.model, &params)
        .map_err(|e| format!("cannot load the model: {e}"))?;

    let mut out = std::io::stdout();
    let mut earlier: Vec<String> = Vec::new();
    let n = job.steps.len();
    for (i, step) in job.steps.iter().enumerate() {
        let last = i + 1 == n;
        let prev = earlier.join("\n\n");
        let chat: Vec<Message> = step
            .iter()
            .map(|m| Message {
                role: m.role.clone(),
                content: m.content.replace("{prev}", &prev),
            })
            .collect();
        let text = answer(&backend, &model, &chat, job.max_tokens, &mut |piece| {
            if last {
                let _ = writeln!(
                    out,
                    "token {}",
                    serde_json::to_string(piece).unwrap_or_default()
                );
                let _ = out.flush();
            }
        })?;
        if !last {
            earlier.push(text);
            let _ = writeln!(out, "progress {}", (i + 1) as f32 / n as f32);
            let _ = out.flush();
        }
    }
    println!("done");
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
