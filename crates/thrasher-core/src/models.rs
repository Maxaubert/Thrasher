//! The transcription models Thrasher knows about, where they live on disk and how to
//! download them. Downloads resume from a `.part` file and are checked by size and hash.

use std::path::Path;
use std::path::PathBuf;
#[cfg(feature = "download")]
use std::sync::atomic::{AtomicBool, Ordering};

/// Which set of alignment heads a model uses for word timestamps: the OpenAI model
/// it was trained from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heads {
    Tiny,
    TinyEn,
    Base,
    BaseEn,
    Small,
    SmallEn,
    Medium,
    MediumEn,
    LargeV2,
    LargeV3,
    LargeV3Turbo,
}

/// The families the model list is grouped by, in the order shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Norwegian,
    NorwegianVerbatim,
    AllLanguages,
    English,
    Swedish,
}

impl Group {
    pub const ALL: [Group; 5] = [
        Group::Norwegian,
        Group::NorwegianVerbatim,
        Group::AllLanguages,
        Group::English,
        Group::Swedish,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Group::Norwegian => "NORWEGIAN",
            Group::NorwegianVerbatim => "NORWEGIAN, WORD FOR WORD",
            Group::AllLanguages => "ALL LANGUAGES",
            Group::English => "ENGLISH ONLY",
            Group::Swedish => "SWEDISH",
        }
    }

    pub fn by(self) -> &'static str {
        match self {
            Group::Norwegian | Group::NorwegianVerbatim => {
                "NB-Whisper · National Library of Norway"
            }
            Group::AllLanguages | Group::English => "Whisper · OpenAI",
            Group::Swedish => "KB-Whisper · National Library of Sweden",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub group: Group,
    pub url: &'static str,
    pub bytes: u64,
    /// SHA-256 published by Hugging Face (the LFS object id).
    pub sha256: &'static str,
    pub heads: Heads,
    /// The language it is made for; empty for all languages.
    pub language: &'static str,
    /// Rough guides from 1 to 5, for the model list.
    pub quality: u8,
    pub speed: u8,
    /// A quantised copy: about a third of the size, nearly the same quality.
    pub compressed: bool,
}

/// Every ggml model the NB-Whisper, KB-Whisper and whisper.cpp repositories publish
/// (large-v1 left out: large-v2 and v3 replace it). Sizes and hashes from the
/// Hugging Face API on 2026-10-24.
pub const CATALOG: &[ModelInfo] = &[
    ModelInfo {
        id: "nb-whisper-tiny",
        name: "NB-Whisper Tiny",
        group: Group::Norwegian,
        url: "https://huggingface.co/NbAiLab/nb-whisper-tiny/resolve/main/ggml-model.bin",
        bytes: 77_691_730,
        sha256: "2f9dd799ee36b6a9c8d642e9b1df8ecf2135efdd5a91b1d9ca0b3c0decda535f",
        heads: Heads::Tiny,
        language: "no",
        quality: 1,
        speed: 5,
        compressed: false,
    },
    ModelInfo {
        id: "nb-whisper-tiny-q5",
        name: "NB-Whisper Tiny, compressed",
        group: Group::Norwegian,
        url: "https://huggingface.co/NbAiLab/nb-whisper-tiny/resolve/main/ggml-model-q5_0.bin",
        bytes: 29_875_738,
        sha256: "e5fb42192cdf31bea624a524d035e8895030b2bb4b31d4ea2a1ebf0ea8f57237",
        heads: Heads::Tiny,
        language: "no",
        quality: 1,
        speed: 5,
        compressed: true,
    },
    ModelInfo {
        id: "nb-whisper-base",
        name: "NB-Whisper Base",
        group: Group::Norwegian,
        url: "https://huggingface.co/NbAiLab/nb-whisper-base/resolve/main/ggml-model.bin",
        bytes: 147_951_482,
        sha256: "05fc04a7f16b634b99e0cbbe5e4e78a00469b834219e83df39ab4f8bca5c4b97",
        heads: Heads::Base,
        language: "no",
        quality: 2,
        speed: 5,
        compressed: false,
    },
    ModelInfo {
        id: "nb-whisper-base-q5",
        name: "NB-Whisper Base, compressed",
        group: Group::Norwegian,
        url: "https://huggingface.co/NbAiLab/nb-whisper-base/resolve/main/ggml-model-q5_0.bin",
        bytes: 55_295_450,
        sha256: "dcb9f3ab963cd288974c826c1519ff73b78b2372e80d388a6ce94f29c6a5b40f",
        heads: Heads::Base,
        language: "no",
        quality: 2,
        speed: 5,
        compressed: true,
    },
    ModelInfo {
        id: "nb-whisper-small",
        name: "NB-Whisper Small",
        group: Group::Norwegian,
        url: "https://huggingface.co/NbAiLab/nb-whisper-small/resolve/main/ggml-model.bin",
        bytes: 487_601_984,
        sha256: "a0fc1555f5bd51044b0ea88bb9b7891e1ee331a8087eddb0afc3a05839db48ab",
        heads: Heads::Small,
        language: "no",
        quality: 3,
        speed: 4,
        compressed: false,
    },
    ModelInfo {
        id: "nb-whisper-small-q5",
        name: "NB-Whisper Small, compressed",
        group: Group::Norwegian,
        url: "https://huggingface.co/NbAiLab/nb-whisper-small/resolve/main/ggml-model-q5_0.bin",
        bytes: 175_209_680,
        sha256: "2a9025afb6e825fc4ae6a46671e0cb2f43e62f1dec87270deea6fe61b5285a20",
        heads: Heads::Small,
        language: "no",
        quality: 3,
        speed: 4,
        compressed: true,
    },
    ModelInfo {
        id: "nb-whisper-medium",
        name: "NB-Whisper Medium",
        group: Group::Norwegian,
        url: "https://huggingface.co/NbAiLab/nb-whisper-medium/resolve/main/ggml-model.bin",
        bytes: 1_533_763_076,
        sha256: "f73141401d203ee77fc7ddf7bf97926a8a85fe85faa6066a6920e4815f48a73d",
        heads: Heads::Medium,
        language: "no",
        quality: 4,
        speed: 3,
        compressed: false,
    },
    ModelInfo {
        id: "nb-whisper-medium-q5",
        name: "NB-Whisper Medium, compressed",
        group: Group::Norwegian,
        url: "https://huggingface.co/NbAiLab/nb-whisper-medium/resolve/main/ggml-model-q5_0.bin",
        bytes: 539_212_484,
        sha256: "18733de634af639a43b0f8c5f5a2ea0920de4c5b32a5570ec130981581c0e5e7",
        heads: Heads::Medium,
        language: "no",
        quality: 4,
        speed: 3,
        compressed: true,
    },
    ModelInfo {
        id: "nb-whisper-large",
        name: "NB-Whisper Large",
        group: Group::Norwegian,
        url: "https://huggingface.co/NbAiLab/nb-whisper-large/resolve/main/ggml-model.bin",
        bytes: 3_095_033_483,
        sha256: "0f2f66f22e11a7c7da3c582d8e5c89cb2c0011753ba9c7c9731e320a4ba33e76",
        heads: Heads::LargeV3,
        language: "no",
        quality: 5,
        speed: 1,
        compressed: false,
    },
    ModelInfo {
        id: "nb-whisper-large-q5",
        name: "NB-Whisper Large, compressed",
        group: Group::Norwegian,
        url: "https://huggingface.co/NbAiLab/nb-whisper-large/resolve/main/ggml-model-q5_0.bin",
        bytes: 1_081_140_203,
        sha256: "feb5951ae694a62cfeb81fb501f6cfa8cc50d96bcddb1e4e8215f7006bac23a2",
        heads: Heads::LargeV3,
        language: "no",
        quality: 5,
        speed: 1,
        compressed: true,
    },
    ModelInfo {
        id: "nb-whisper-tiny-verbatim",
        name: "NB-Whisper Tiny Verbatim",
        group: Group::NorwegianVerbatim,
        url: "https://huggingface.co/NbAiLab/nb-whisper-tiny-verbatim/resolve/main/ggml-model.bin",
        bytes: 77_691_730,
        sha256: "c285a5e2af1943ac7015eef4312403ecf83e1f44eff1e5703f48254b32d25a43",
        heads: Heads::Tiny,
        language: "no",
        quality: 1,
        speed: 5,
        compressed: false,
    },
    ModelInfo {
        id: "nb-whisper-tiny-verbatim-q5",
        name: "NB-Whisper Tiny Verbatim, compressed",
        group: Group::NorwegianVerbatim,
        url: "https://huggingface.co/NbAiLab/nb-whisper-tiny-verbatim/resolve/main/ggml-model-q5_0.bin",
        bytes: 29_875_738,
        sha256: "b791d9a49a3c144db2d25a2f1419f265f8306316c1cd8a6f313ee795d07139a9",
        heads: Heads::Tiny,
        language: "no",
        quality: 1,
        speed: 5,
        compressed: true,
    },
    ModelInfo {
        id: "nb-whisper-base-verbatim",
        name: "NB-Whisper Base Verbatim",
        group: Group::NorwegianVerbatim,
        url: "https://huggingface.co/NbAiLab/nb-whisper-base-verbatim/resolve/main/ggml-model.bin",
        bytes: 147_951_482,
        sha256: "b3418e3ef22b8d88148dac4f431da9e9e56c01493e1f55edf56585acd59d80db",
        heads: Heads::Base,
        language: "no",
        quality: 2,
        speed: 5,
        compressed: false,
    },
    ModelInfo {
        id: "nb-whisper-base-verbatim-q5",
        name: "NB-Whisper Base Verbatim, compressed",
        group: Group::NorwegianVerbatim,
        url: "https://huggingface.co/NbAiLab/nb-whisper-base-verbatim/resolve/main/ggml-model-q5_0.bin",
        bytes: 55_295_450,
        sha256: "eef8a758a1d8641f1cae87f91422110b696ff1a89259d74c8df5055519556224",
        heads: Heads::Base,
        language: "no",
        quality: 2,
        speed: 5,
        compressed: true,
    },
    ModelInfo {
        id: "nb-whisper-small-verbatim",
        name: "NB-Whisper Small Verbatim",
        group: Group::NorwegianVerbatim,
        url: "https://huggingface.co/NbAiLab/nb-whisper-small-verbatim/resolve/main/ggml-model.bin",
        bytes: 487_601_984,
        sha256: "f80a415709dbd56e13cd7ee8b961f881bf81082c84247109e29ae2cd171afe72",
        heads: Heads::Small,
        language: "no",
        quality: 3,
        speed: 4,
        compressed: false,
    },
    ModelInfo {
        id: "nb-whisper-small-verbatim-q5",
        name: "NB-Whisper Small Verbatim, compressed",
        group: Group::NorwegianVerbatim,
        url: "https://huggingface.co/NbAiLab/nb-whisper-small-verbatim/resolve/main/ggml-model-q5_0.bin",
        bytes: 175_209_680,
        sha256: "8982b4a4e365f3f64a60ca12abb7382132e59b2a1040d52ab8ddd577948a5c96",
        heads: Heads::Small,
        language: "no",
        quality: 3,
        speed: 4,
        compressed: true,
    },
    ModelInfo {
        id: "nb-whisper-medium-verbatim",
        name: "NB-Whisper Medium Verbatim",
        group: Group::NorwegianVerbatim,
        url: "https://huggingface.co/NbAiLab/nb-whisper-medium-verbatim/resolve/main/ggml-model.bin",
        bytes: 1_533_763_076,
        sha256: "5d6c56c7ccaeee20b8ef65f0188ba9f6a06c2f0d35797b86b851e77adc451770",
        heads: Heads::Medium,
        language: "no",
        quality: 4,
        speed: 3,
        compressed: false,
    },
    ModelInfo {
        id: "nb-whisper-medium-verbatim-q5",
        name: "NB-Whisper Medium Verbatim, compressed",
        group: Group::NorwegianVerbatim,
        url: "https://huggingface.co/NbAiLab/nb-whisper-medium-verbatim/resolve/main/ggml-model-q5_0.bin",
        bytes: 539_212_484,
        sha256: "600aff00e417bf070364da36d6d4a6ed5f28a3deac11c5919b62733ee17cecd1",
        heads: Heads::Medium,
        language: "no",
        quality: 4,
        speed: 3,
        compressed: true,
    },
    ModelInfo {
        id: "nb-whisper-large-verbatim",
        name: "NB-Whisper Large Verbatim",
        group: Group::NorwegianVerbatim,
        url: "https://huggingface.co/NbAiLab/nb-whisper-large-verbatim/resolve/main/ggml-model.bin",
        bytes: 3_095_033_483,
        sha256: "0227866ef47189f79d04347a080c73fcd8267c8e2255da5f77803506ab4ffc7d",
        heads: Heads::LargeV3,
        language: "no",
        quality: 5,
        speed: 1,
        compressed: false,
    },
    ModelInfo {
        id: "nb-whisper-large-verbatim-q5",
        name: "NB-Whisper Large Verbatim, compressed",
        group: Group::NorwegianVerbatim,
        url: "https://huggingface.co/NbAiLab/nb-whisper-large-verbatim/resolve/main/ggml-model-q5_0.bin",
        bytes: 1_081_140_203,
        sha256: "344f6cbda0fb44b20d19d02dd423bb87723a6fc13fe02d45dff3d7505a12f919",
        heads: Heads::LargeV3,
        language: "no",
        quality: 5,
        speed: 1,
        compressed: true,
    },
    ModelInfo {
        id: "whisper-large-v3-turbo",
        name: "Whisper Large v3 Turbo",
        group: Group::AllLanguages,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
        bytes: 1_624_555_275,
        sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
        heads: Heads::LargeV3Turbo,
        language: "",
        quality: 4,
        speed: 4,
        compressed: false,
    },
    ModelInfo {
        id: "whisper-large-v3-turbo-q5",
        name: "Whisper Large v3 Turbo, compressed",
        group: Group::AllLanguages,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
        bytes: 574_041_195,
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        heads: Heads::LargeV3Turbo,
        language: "",
        quality: 4,
        speed: 4,
        compressed: true,
    },
    ModelInfo {
        id: "whisper-large-v3",
        name: "Whisper Large v3",
        group: Group::AllLanguages,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin",
        bytes: 3_095_033_483,
        sha256: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
        heads: Heads::LargeV3,
        language: "",
        quality: 5,
        speed: 1,
        compressed: false,
    },
    ModelInfo {
        id: "whisper-large-v3-q5",
        name: "Whisper Large v3, compressed",
        group: Group::AllLanguages,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-q5_0.bin",
        bytes: 1_081_140_203,
        sha256: "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1",
        heads: Heads::LargeV3,
        language: "",
        quality: 5,
        speed: 1,
        compressed: true,
    },
    ModelInfo {
        id: "whisper-large-v2",
        name: "Whisper Large v2",
        group: Group::AllLanguages,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v2.bin",
        bytes: 3_094_623_691,
        sha256: "9a423fe4d40c82774b6af34115b8b935f34152246eb19e80e376071d3f999487",
        heads: Heads::LargeV2,
        language: "",
        quality: 4,
        speed: 1,
        compressed: false,
    },
    ModelInfo {
        id: "whisper-large-v2-q5",
        name: "Whisper Large v2, compressed",
        group: Group::AllLanguages,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v2-q5_0.bin",
        bytes: 1_080_732_091,
        sha256: "3a214837221e4530dbc1fe8d734f302af393eb30bd0ed046042ebf4baf70f6f2",
        heads: Heads::LargeV2,
        language: "",
        quality: 4,
        speed: 1,
        compressed: true,
    },
    ModelInfo {
        id: "whisper-medium",
        name: "Whisper Medium",
        group: Group::AllLanguages,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
        bytes: 1_533_763_059,
        sha256: "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
        heads: Heads::Medium,
        language: "",
        quality: 4,
        speed: 3,
        compressed: false,
    },
    ModelInfo {
        id: "whisper-medium-q5",
        name: "Whisper Medium, compressed",
        group: Group::AllLanguages,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium-q5_0.bin",
        bytes: 539_212_467,
        sha256: "19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f",
        heads: Heads::Medium,
        language: "",
        quality: 4,
        speed: 3,
        compressed: true,
    },
    ModelInfo {
        id: "whisper-small",
        name: "Whisper Small",
        group: Group::AllLanguages,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        bytes: 487_601_967,
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        heads: Heads::Small,
        language: "",
        quality: 3,
        speed: 4,
        compressed: false,
    },
    ModelInfo {
        id: "whisper-small-q5",
        name: "Whisper Small, compressed",
        group: Group::AllLanguages,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin",
        bytes: 190_085_487,
        sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
        heads: Heads::Small,
        language: "",
        quality: 3,
        speed: 4,
        compressed: true,
    },
    ModelInfo {
        id: "whisper-base",
        name: "Whisper Base",
        group: Group::AllLanguages,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
        bytes: 147_951_465,
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
        heads: Heads::Base,
        language: "",
        quality: 2,
        speed: 5,
        compressed: false,
    },
    ModelInfo {
        id: "whisper-base-q5",
        name: "Whisper Base, compressed",
        group: Group::AllLanguages,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin",
        bytes: 59_707_625,
        sha256: "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",
        heads: Heads::Base,
        language: "",
        quality: 2,
        speed: 5,
        compressed: true,
    },
    ModelInfo {
        id: "whisper-tiny",
        name: "Whisper Tiny",
        group: Group::AllLanguages,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
        bytes: 77_691_713,
        sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
        heads: Heads::Tiny,
        language: "",
        quality: 1,
        speed: 5,
        compressed: false,
    },
    ModelInfo {
        id: "whisper-tiny-q5",
        name: "Whisper Tiny, compressed",
        group: Group::AllLanguages,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny-q5_1.bin",
        bytes: 32_152_673,
        sha256: "818710568da3ca15689e31a743197b520007872ff9576237bda97bd1b469c3d7",
        heads: Heads::Tiny,
        language: "",
        quality: 1,
        speed: 5,
        compressed: true,
    },
    ModelInfo {
        id: "whisper-medium-en",
        name: "Whisper Medium English",
        group: Group::English,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin",
        bytes: 1_533_774_781,
        sha256: "cc37e93478338ec7700281a7ac30a10128929eb8f427dda2e865faa8f6da4356",
        heads: Heads::MediumEn,
        language: "en",
        quality: 4,
        speed: 3,
        compressed: false,
    },
    ModelInfo {
        id: "whisper-medium-en-q5",
        name: "Whisper Medium English, compressed",
        group: Group::English,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en-q5_0.bin",
        bytes: 539_225_533,
        sha256: "76733e26ad8fe1c7a5bf7531a9d41917b2adc0f20f2e4f5531688a8c6cd88eb0",
        heads: Heads::MediumEn,
        language: "en",
        quality: 4,
        speed: 3,
        compressed: true,
    },
    ModelInfo {
        id: "whisper-small-en",
        name: "Whisper Small English",
        group: Group::English,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
        bytes: 487_614_201,
        sha256: "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d",
        heads: Heads::SmallEn,
        language: "en",
        quality: 3,
        speed: 4,
        compressed: false,
    },
    ModelInfo {
        id: "whisper-small-en-q5",
        name: "Whisper Small English, compressed",
        group: Group::English,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en-q5_1.bin",
        bytes: 190_098_681,
        sha256: "bfdff4894dcb76bbf647d56263ea2a96645423f1669176f4844a1bf8e478ad30",
        heads: Heads::SmallEn,
        language: "en",
        quality: 3,
        speed: 4,
        compressed: true,
    },
    ModelInfo {
        id: "whisper-base-en",
        name: "Whisper Base English",
        group: Group::English,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
        bytes: 147_964_211,
        sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
        heads: Heads::BaseEn,
        language: "en",
        quality: 2,
        speed: 5,
        compressed: false,
    },
    ModelInfo {
        id: "whisper-base-en-q5",
        name: "Whisper Base English, compressed",
        group: Group::English,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en-q5_1.bin",
        bytes: 59_721_011,
        sha256: "4baf70dd0d7c4247ba2b81fafd9c01005ac77c2f9ef064e00dcf195d0e2fdd2f",
        heads: Heads::BaseEn,
        language: "en",
        quality: 2,
        speed: 5,
        compressed: true,
    },
    ModelInfo {
        id: "whisper-tiny-en",
        name: "Whisper Tiny English",
        group: Group::English,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
        bytes: 77_704_715,
        sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
        heads: Heads::TinyEn,
        language: "en",
        quality: 1,
        speed: 5,
        compressed: false,
    },
    ModelInfo {
        id: "whisper-tiny-en-q5",
        name: "Whisper Tiny English, compressed",
        group: Group::English,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en-q5_1.bin",
        bytes: 32_166_155,
        sha256: "c77c5766f1cef09b6b7d47f21b546cbddd4157886b3b5d6d4f709e91e66c7c2b",
        heads: Heads::TinyEn,
        language: "en",
        quality: 1,
        speed: 5,
        compressed: true,
    },
    ModelInfo {
        id: "kb-whisper-tiny",
        name: "KB-Whisper Tiny",
        group: Group::Swedish,
        url: "https://huggingface.co/KBLab/kb-whisper-tiny/resolve/main/ggml-model.bin",
        bytes: 77_691_730,
        sha256: "054187c95948ee0455d428db0c0d6c84d6c6157dab72e86857ced13233118b03",
        heads: Heads::Tiny,
        language: "sv",
        quality: 1,
        speed: 5,
        compressed: false,
    },
    ModelInfo {
        id: "kb-whisper-tiny-q5",
        name: "KB-Whisper Tiny, compressed",
        group: Group::Swedish,
        url: "https://huggingface.co/KBLab/kb-whisper-tiny/resolve/main/ggml-model-q5_0.bin",
        bytes: 29_875_738,
        sha256: "98d46b7d23e5528d006e8a42e29eb0cb39b44bed94e1329f10f57d1fd15c658b",
        heads: Heads::Tiny,
        language: "sv",
        quality: 1,
        speed: 5,
        compressed: true,
    },
    ModelInfo {
        id: "kb-whisper-base",
        name: "KB-Whisper Base",
        group: Group::Swedish,
        url: "https://huggingface.co/KBLab/kb-whisper-base/resolve/main/ggml-model.bin",
        bytes: 147_951_482,
        sha256: "f5e3cdb33e537eedfa2a749b5cae28c4c511873a1b13362f87dffbe07891d3fe",
        heads: Heads::Base,
        language: "sv",
        quality: 2,
        speed: 5,
        compressed: false,
    },
    ModelInfo {
        id: "kb-whisper-base-q5",
        name: "KB-Whisper Base, compressed",
        group: Group::Swedish,
        url: "https://huggingface.co/KBLab/kb-whisper-base/resolve/main/ggml-model-q5_0.bin",
        bytes: 55_295_450,
        sha256: "aead29b356bca8840e72a8dc2286e2d69e6702639751a1e60cb3c8eacefec546",
        heads: Heads::Base,
        language: "sv",
        quality: 2,
        speed: 5,
        compressed: true,
    },
    ModelInfo {
        id: "kb-whisper-small",
        name: "KB-Whisper Small",
        group: Group::Swedish,
        url: "https://huggingface.co/KBLab/kb-whisper-small/resolve/main/ggml-model.bin",
        bytes: 487_601_984,
        sha256: "de6911330cbdc131362f7a955682b65c8a5a2394caba73e7ea821a9822efb8c6",
        heads: Heads::Small,
        language: "sv",
        quality: 3,
        speed: 4,
        compressed: false,
    },
    ModelInfo {
        id: "kb-whisper-small-q5",
        name: "KB-Whisper Small, compressed",
        group: Group::Swedish,
        url: "https://huggingface.co/KBLab/kb-whisper-small/resolve/main/ggml-model-q5_0.bin",
        bytes: 175_209_680,
        sha256: "6768836a51abc902e420c613153e6d418c90ea2774e913274d02ab23170225b7",
        heads: Heads::Small,
        language: "sv",
        quality: 3,
        speed: 4,
        compressed: true,
    },
    ModelInfo {
        id: "kb-whisper-medium",
        name: "KB-Whisper Medium",
        group: Group::Swedish,
        url: "https://huggingface.co/KBLab/kb-whisper-medium/resolve/main/ggml-model.bin",
        bytes: 1_533_763_076,
        sha256: "1b7842bc1c3f79fb3bf043a0a3590961d625a49ef3ccbdceb00e738c5dd8b015",
        heads: Heads::Medium,
        language: "sv",
        quality: 4,
        speed: 3,
        compressed: false,
    },
    ModelInfo {
        id: "kb-whisper-medium-q5",
        name: "KB-Whisper Medium, compressed",
        group: Group::Swedish,
        url: "https://huggingface.co/KBLab/kb-whisper-medium/resolve/main/ggml-model-q5_0.bin",
        bytes: 539_212_484,
        sha256: "7f8762e0ade9e0073674c0d5acae942a0b1ea98add9baa008ee89c94eaba43d0",
        heads: Heads::Medium,
        language: "sv",
        quality: 4,
        speed: 3,
        compressed: true,
    },
    ModelInfo {
        id: "kb-whisper-large",
        name: "KB-Whisper Large",
        group: Group::Swedish,
        url: "https://huggingface.co/KBLab/kb-whisper-large/resolve/main/ggml-model.bin",
        bytes: 3_095_033_483,
        sha256: "b66f2dda369a88f6c03fe37326d7cc37aa216f6f34e6fc1be686e497ba9c2f39",
        heads: Heads::LargeV3,
        language: "sv",
        quality: 5,
        speed: 1,
        compressed: false,
    },
    ModelInfo {
        id: "kb-whisper-large-q5",
        name: "KB-Whisper Large, compressed",
        group: Group::Swedish,
        url: "https://huggingface.co/KBLab/kb-whisper-large/resolve/main/ggml-model-q5_0.bin",
        bytes: 1_081_140_203,
        sha256: "6d2863812d7410322bb7d8647a5c7260761300fa946714c9ed66d22bb30bcb19",
        heads: Heads::LargeV3,
        language: "sv",
        quality: 5,
        speed: 1,
        compressed: true,
    },
];

pub fn find(id: &str) -> Option<&'static ModelInfo> {
    CATALOG.iter().find(|m| m.id == id)
}

/// `%LOCALAPPDATA%\Thrasher\models` on Windows: per user, never a shared folder.
pub fn models_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.data_local_dir().join("Thrasher").join("models"))
}

impl ModelInfo {
    /// Where the model is stored, or `None` if the user has no local data folder.
    pub fn path(&self) -> Option<PathBuf> {
        let file = self.url.rsplit('/').next().unwrap_or("model.bin");
        models_dir().map(|d| d.join(self.id).join(file))
    }

    /// True when the file is there with the right size. The hash is checked once,
    /// when the download finishes.
    pub fn is_downloaded(&self) -> bool {
        self.path()
            .and_then(|p| std::fs::metadata(p).ok())
            .is_some_and(|m| m.len() == self.bytes)
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
    let dest = model
        .path()
        .ok_or("no local app data folder to store models in")?;
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
    verify_and_finish(&part, &dest, model.bytes, model.sha256)
}

#[cfg(any(feature = "download", test))]
fn verify_and_finish(
    part: &Path,
    dest: &Path,
    bytes: u64,
    sha256: &str,
) -> Result<PathBuf, String> {
    let len = std::fs::metadata(part).map_err(|e| e.to_string())?.len();
    if len != bytes {
        return Err(format!(
            "download incomplete: {len} of {bytes} bytes; try again to resume"
        ));
    }
    let actual = sha256_file(part).map_err(|e| e.to_string())?;
    if actual != sha256 {
        let _ = std::fs::remove_file(part);
        return Err("the downloaded file is damaged (checksum mismatch); download it again".into());
    }
    std::fs::rename(part, dest).map_err(|e| e.to_string())?;
    Ok(dest.to_path_buf())
}

/// Hex SHA-256 of a file.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
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
            assert!(
                m.path()
                    .unwrap()
                    .ends_with(m.url.rsplit('/').next().unwrap())
            );
            assert_eq!(m.sha256.len(), 64);
        }
        assert_eq!(find("nb-whisper-large").unwrap().heads, Heads::LargeV3);
    }

    #[test]
    fn earlier_ids_and_files_still_exist() {
        // Saved settings and downloaded files use these.
        for (id, file) in [
            ("nb-whisper-large", "ggml-model.bin"),
            ("nb-whisper-large-q5", "ggml-model-q5_0.bin"),
            ("nb-whisper-large-verbatim", "ggml-model.bin"),
            ("nb-whisper-medium", "ggml-model.bin"),
            ("whisper-large-v3-turbo", "ggml-large-v3-turbo.bin"),
        ] {
            let m = find(id).unwrap();
            assert!(
                m.path()
                    .unwrap()
                    .ends_with(std::path::Path::new(id).join(file))
            );
        }
    }

    #[test]
    fn every_group_has_models_and_english_ones_are_english() {
        for g in Group::ALL {
            assert!(CATALOG.iter().any(|m| m.group == g), "{g:?}");
        }
        for m in CATALOG.iter().filter(|m| m.group == Group::English) {
            assert_eq!(m.language, "en");
            assert!(matches!(
                m.heads,
                Heads::TinyEn | Heads::BaseEn | Heads::SmallEn | Heads::MediumEn
            ));
        }
    }

    #[test]
    fn finish_checks_size() {
        let dir = tempfile::tempdir().unwrap();
        let part = dir.path().join("m.bin.part");
        let dest = dir.path().join("m.bin");
        std::fs::write(&part, b"abc").unwrap();
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify_and_finish(&part, &dest, 4, abc).is_err());
        assert!(verify_and_finish(&part, &dest, 3, &"0".repeat(64)).is_err());
        assert!(!part.exists(), "a damaged download is removed");
        std::fs::write(&part, b"abc").unwrap();
        assert_eq!(verify_and_finish(&part, &dest, 3, abc).unwrap(), dest);
        assert!(dest.exists());
    }
}
