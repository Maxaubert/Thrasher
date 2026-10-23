//! The transcription models Thrasher knows about, where they live on disk and how to
//! download them. Downloads resume from a `.part` file and are checked by size.

#[cfg(any(feature = "download", test))]
use std::path::Path;
use std::path::PathBuf;
#[cfg(feature = "download")]
use std::sync::atomic::{AtomicBool, Ordering};

/// Which set of alignment heads a model uses for word timestamps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heads {
    Medium,
    LargeV3,
    LargeV3Turbo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub url: &'static str,
    pub bytes: u64,
    pub heads: Heads,
    pub language: &'static str,
}

/// Sizes checked against Hugging Face on 2026-10-23.
pub const CATALOG: &[ModelInfo] = &[
    ModelInfo {
        id: "nb-whisper-large",
        name: "NB-Whisper Large (Norwegian)",
        description: "Best Norwegian quality. 3.1 GB, needs a GPU to be quick.",
        url: "https://huggingface.co/NbAiLab/nb-whisper-large/resolve/main/ggml-model.bin",
        bytes: 3_095_033_483,
        heads: Heads::LargeV3,
        language: "no",
    },
    ModelInfo {
        id: "nb-whisper-large-q5",
        name: "NB-Whisper Large, compressed (Norwegian)",
        description: "Almost the same quality at 1.1 GB.",
        url: "https://huggingface.co/NbAiLab/nb-whisper-large/resolve/main/ggml-model-q5_0.bin",
        bytes: 1_081_140_203,
        heads: Heads::LargeV3,
        language: "no",
    },
    ModelInfo {
        id: "nb-whisper-large-verbatim",
        name: "NB-Whisper Large Verbatim (Norwegian)",
        description: "Keeps filler words and repetitions, for exact transcripts. 3.1 GB.",
        url: "https://huggingface.co/NbAiLab/nb-whisper-large-verbatim/resolve/main/ggml-model.bin",
        bytes: 3_095_033_483,
        heads: Heads::LargeV3,
        language: "no",
    },
    ModelInfo {
        id: "nb-whisper-medium",
        name: "NB-Whisper Medium (Norwegian)",
        description: "Faster and smaller, a little less accurate. 1.5 GB.",
        url: "https://huggingface.co/NbAiLab/nb-whisper-medium/resolve/main/ggml-model.bin",
        bytes: 1_533_763_076,
        heads: Heads::Medium,
        language: "no",
    },
    ModelInfo {
        id: "whisper-large-v3-turbo",
        name: "Whisper Large v3 Turbo (all languages)",
        description: "OpenAI's fast multilingual model. 1.6 GB.",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
        bytes: 1_624_555_275,
        heads: Heads::LargeV3Turbo,
        language: "",
    },
];

pub fn find(id: &str) -> Option<&'static ModelInfo> {
    CATALOG.iter().find(|m| m.id == id)
}

/// `%LOCALAPPDATA%\Thrasher\models` on Windows.
pub fn models_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Thrasher")
        .join("models")
}

impl ModelInfo {
    pub fn path(&self) -> PathBuf {
        let file = self.url.rsplit('/').next().unwrap_or("model.bin");
        models_dir().join(self.id).join(file)
    }

    pub fn is_downloaded(&self) -> bool {
        std::fs::metadata(self.path()).is_ok_and(|m| m.len() == self.bytes)
    }
}

/// Downloads a model, resuming a partial download. `progress` receives bytes done and
/// total. Setting `cancel` stops the download and keeps the partial file.
#[cfg(feature = "download")]
pub fn download(
    model: &ModelInfo,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    use std::io::{Read, Write};
    let dest = model.path();
    if model.is_downloaded() {
        return Ok(dest);
    }
    let dir = dest.parent().unwrap();
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let part = dest.with_extension("bin.part");
    let mut done = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if done > model.bytes {
        std::fs::remove_file(&part).map_err(|e| e.to_string())?;
        done = 0;
    }
    if done < model.bytes {
        let mut req = ureq::get(model.url);
        if done > 0 {
            req = req.header("Range", &format!("bytes={done}-"));
        }
        let mut resp = req.call().map_err(|e| format!("download failed: {e}"))?;
        if done > 0 && resp.status() != 206 {
            // The server ignored the range: start over.
            done = 0;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(done > 0)
            .truncate(done == 0)
            .open(&part)
            .map_err(|e| e.to_string())?;
        let mut reader = resp.body_mut().as_reader();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let n = reader
                .read(&mut buf)
                .map_err(|e| format!("download failed: {e}"))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            done += n as u64;
            progress(done, model.bytes);
        }
        file.sync_all().map_err(|e| e.to_string())?;
    }
    verify_and_finish(&part, &dest, model.bytes)
}

#[cfg(any(feature = "download", test))]
fn verify_and_finish(part: &Path, dest: &Path, bytes: u64) -> Result<PathBuf, String> {
    let len = std::fs::metadata(part).map_err(|e| e.to_string())?.len();
    if len != bytes {
        return Err(format!(
            "download incomplete: {len} of {bytes} bytes; try again to resume"
        ));
    }
    std::fs::rename(part, dest).map_err(|e| e.to_string())?;
    Ok(dest.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_consistent() {
        let mut ids: Vec<_> = CATALOG.iter().map(|m| m.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len());
        for m in CATALOG {
            assert!(m.url.starts_with("https://huggingface.co/"));
            assert!(m.path().ends_with(m.url.rsplit('/').next().unwrap()));
        }
        assert_eq!(find("nb-whisper-large").unwrap().heads, Heads::LargeV3);
    }

    #[test]
    fn finish_checks_size() {
        let dir = tempfile::tempdir().unwrap();
        let part = dir.path().join("m.bin.part");
        let dest = dir.path().join("m.bin");
        std::fs::write(&part, [0u8; 10]).unwrap();
        assert!(verify_and_finish(&part, &dest, 11).is_err());
        assert_eq!(verify_and_finish(&part, &dest, 10).unwrap(), dest);
        assert!(dest.exists());
    }
}
