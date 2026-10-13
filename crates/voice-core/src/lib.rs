//! Motor de DSP em tempo real para transformar a voz em algo sobrenatural.
//!
//! Sem dependências de hardware: recebe amostras `f32` mono e devolve amostras
//! processadas. A camada de áudio (WASAPI/cpal) fica no crate `voice-app`.

pub mod biquad;
pub mod dynamics;
pub mod engine;
pub mod period;
pub mod pitch;
pub mod reverb;
pub mod settings;
pub mod util;

pub use engine::{Engine, Meters};
pub use settings::{Settings, builtin_presets};
