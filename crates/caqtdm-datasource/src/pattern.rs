//! Shared waveform language for the `sim` and `demo` plugins.
//!
//! A channel name is `kind` or `kind:args` with an optional `?key=value`
//! query. Kinds:
//!
//! - `const:3.5` a fixed number
//! - `sine` or `sine:amp:period_ms:offset`
//! - `ramp:min:max:period_ms`
//! - `square:period_ms`
//! - `enum:Off,On,Fault`
//! - `wave:count:period_ms` a moving sine array
//! - `string:hello`
//!
//! `?sev=minor|major|invalid|ok` forces the alarm severity. `?alarm=1` raises
//! minor above 80 and major above 90 on a 0..100 display range.

use std::collections::BTreeMap;

use caqtdm_core::{ChannelData, Limits, PvValue, Severity};

#[derive(Debug, Clone)]
pub struct Pattern {
    pub kind: String,
    pub args: Vec<String>,
    pub query: BTreeMap<String, String>,
}

pub fn parse_pattern(name: &str) -> Pattern {
    let (body, query_src) = name.split_once('?').unwrap_or((name, ""));
    let mut parts = body.split(':');
    let kind = parts.next().unwrap_or("sine").to_ascii_lowercase();
    let args = parts.map(str::to_string).collect();
    let mut query = BTreeMap::new();
    if !query_src.is_empty() {
        for pair in query_src.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                query.insert(k.to_ascii_lowercase(), v.to_string());
            }
        }
    }
    Pattern { kind, args, query }
}

pub fn evaluate(pattern: &Pattern, now_ms: u64, seed: u64) -> ChannelData {
    let mut data = match pattern.kind.as_str() {
        "const" => {
            let value = pattern
                .args
                .first()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            ChannelData::scalar(value)
        }
        "string" => {
            let mut data = ChannelData::scalar(0.0);
            data.value = PvValue::String(pattern.args.join(":"));
            data
        }
        "enum" => enum_value(pattern, now_ms),
        "ramp" => {
            let min = arg_f64(pattern, 0, 0.0);
            let max = arg_f64(pattern, 1, 100.0);
            let period = arg_f64(pattern, 2, 4000.0).max(1.0);
            let phase = (now_ms as f64 % period) / period;
            let mut data = ChannelData::scalar(min + (max - min) * phase);
            data.limits = span(min, max);
            data
        }
        "square" => {
            let period = arg_f64(pattern, 0, 1000.0).max(1.0);
            let on = (now_ms as f64 % period) < period / 2.0;
            ChannelData::scalar(if on { 1.0 } else { 0.0 })
        }
        "wave" => wave(pattern, now_ms, seed),
        _ => sine(pattern, now_ms, seed),
    };
    if let Some(sev) = pattern.query.get("sev") {
        data.severity = match sev.to_ascii_lowercase().as_str() {
            "minor" | "1" => Severity::Minor,
            "major" | "2" => Severity::Major,
            "invalid" | "3" => Severity::Invalid,
            "disconnected" => Severity::NotConnected,
            _ => Severity::NoAlarm,
        };
        if data.severity == Severity::NotConnected {
            data.connected = false;
        }
    }
    if pattern.query.get("alarm").is_some_and(|v| v == "1") {
        if let Some(value) = data.value.as_f64() {
            if value > 90.0 {
                data.severity = Severity::Major;
            } else if value > 80.0 {
                data.severity = Severity::Minor;
            }
        }
    }
    data
}

fn sine(pattern: &Pattern, now_ms: u64, seed: u64) -> ChannelData {
    let amp = arg_f64(pattern, 0, 40.0);
    let period = arg_f64(pattern, 1, 2000.0).max(1.0);
    let offset = arg_f64(pattern, 2, 50.0);
    let phase = (now_ms as f64 / period) * std::f64::consts::TAU;
    let wobble = (seed % 360) as f64 * std::f64::consts::PI / 180.0;
    let mut data = ChannelData::scalar(offset + amp * (phase + wobble).sin());
    data.limits = span(offset - amp, offset + amp);
    data.precision = 3;
    data
}

fn wave(pattern: &Pattern, now_ms: u64, seed: u64) -> ChannelData {
    let count = arg_f64(pattern, 0, 32.0).clamp(1.0, 4096.0) as usize;
    let period = arg_f64(pattern, 1, 2000.0).max(1.0);
    let shift = (now_ms as f64 / period) * std::f64::consts::TAU;
    let samples: Vec<f64> = (0..count)
        .map(|i| {
            let x = shift + (i as f64) * 0.35 + (seed % 16) as f64;
            50.0 + 40.0 * x.sin()
        })
        .collect();
    let mut data = ChannelData::scalar(samples.first().copied().unwrap_or(0.0));
    data.value = PvValue::Doubles(samples);
    data.element_count = count;
    data.limits = span(0.0, 100.0);
    data
}

fn enum_value(pattern: &Pattern, now_ms: u64) -> ChannelData {
    let labels: Vec<String> = pattern
        .args
        .first()
        .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
        .unwrap_or_else(|| vec!["Off".into(), "On".into()]);
    let n = labels.len().max(1);
    let index = ((now_ms / 1000) as usize) % n;
    let mut data = ChannelData::scalar(index as f64);
    data.enum_strings = labels.clone();
    data.value = PvValue::Enum {
        index: index as i32,
        label: labels.get(index).cloned().unwrap_or_default(),
    };
    data
}

fn span(min: f64, max: f64) -> Limits {
    Limits {
        lower_display: min,
        upper_display: max,
        lower_ctrl: min,
        upper_ctrl: max,
        lower_alarm: f64::NAN,
        lower_warning: f64::NAN,
        upper_warning: f64::NAN,
        upper_alarm: f64::NAN,
    }
}

fn arg_f64(pattern: &Pattern, index: usize, default: f64) -> f64 {
    pattern
        .args
        .get(index)
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

pub fn name_seed(name: &str) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in name.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
