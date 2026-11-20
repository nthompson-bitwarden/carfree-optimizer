//! E14-S06 bounded persistent batch transport. Rust owns every transition;
//! Python sends one frame per batch, never one request per traveler.
//!
//! Malformed frames are rejected before any slot executes. Runtime failures are
//! per-slot; successful slots are not rolled back. See docs/RL-ENVIRONMENT.md.
use optimizer_env::wire::{BatchEnvironment, WireTransition};
use std::collections::BTreeSet;
use std::io::{self, BufRead, Write};

pub const VERSION: u32 = 1;
pub const MAX_SLOTS: usize = 64;
pub const MAX_FRAME_BYTES: usize = 65_536;
pub const MAX_FEATURES: usize = 4096;
pub const MAX_ACTIONS: usize = 1024;
pub const MAX_OUTPUT_BYTES: usize = MAX_SLOTS * (MAX_FEATURES * 32 + MAX_ACTIONS + 256);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    Reset { seed: u64 },
    Step { state: u64, action: usize },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub slot: usize,
    pub operation: Operation,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub slot: usize,
    pub result: Result<WireTransition, String>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn number<T: std::str::FromStr>(s: &str) -> io::Result<T> {
    s.parse().map_err(|_| invalid("invalid integer field"))
}
/// Allocation-bounded LF reader. Oversized/malformed frames are fatal, not
/// resynchronized: an arbitrary suffix must never be interpreted as a request.
fn line(reader: &mut impl BufRead, budget: &mut usize) -> io::Result<Option<String>> {
    let mut bytes = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err(invalid("unterminated frame"))
            };
        }
        let count = available
            .iter()
            .position(|b| *b == b'\n')
            .map_or(available.len(), |i| i + 1);
        if count > *budget {
            return Err(invalid("frame exceeds byte limit"));
        }
        let done = available[count - 1] == b'\n';
        bytes.extend_from_slice(&available[..count]);
        reader.consume(count);
        *budget -= count;
        if done {
            bytes.pop();
            return String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| invalid("frame is not UTF-8"));
        }
    }
}

pub struct Batch<E> {
    slots: Vec<E>,
}
impl<E: BatchEnvironment> Batch<E> {
    pub fn new(slots: Vec<E>) -> io::Result<Self> {
        if slots.is_empty() || slots.len() > MAX_SLOTS {
            return Err(invalid("slot count outside 1..=64"));
        }
        Ok(Self { slots })
    }
    pub fn len(&self) -> usize {
        self.slots.len()
    }
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
    /// Validate the complete request vector before executing its first operation.
    /// Repeated slots are forbidden, preserving one transition per batch slot.
    pub fn execute(&mut self, requests: &[Request]) -> io::Result<Vec<Response>> {
        if requests.is_empty() || requests.len() > MAX_SLOTS {
            return Err(invalid("invalid batch length"));
        }
        let mut seen = BTreeSet::new();
        for request in requests {
            if request.slot >= self.slots.len() || !seen.insert(request.slot) {
                return Err(invalid("unknown or duplicate slot"));
            }
        }
        Ok(requests
            .iter()
            .map(|request| {
                let env = &mut self.slots[request.slot];
                let result = match request.operation {
                    Operation::Reset { seed } => env.reset(seed),
                    Operation::Step { state, action } => env.step(action, state),
                };
                Response {
                    slot: request.slot,
                    result,
                }
            })
            .collect())
    }
    /// Persistent framed protocol. IO/protocol/invalid-output failures close the
    /// stream: clients must discard state and reset on a new connection.
    pub fn serve(&mut self, mut reader: impl BufRead, mut writer: impl Write) -> io::Result<()> {
        loop {
            let mut budget = MAX_FRAME_BYTES;
            let Some(header) = line(&mut reader, &mut budget)? else {
                return Ok(());
            };
            let parts = header.split('\t').collect::<Vec<_>>();
            if parts.len() != 3 || parts[0] != "E14" || parts[1] != "1" {
                return Err(invalid("unsupported frame schema"));
            }
            let count: usize = number(parts[2])?;
            if count == 0 || count > MAX_SLOTS {
                return Err(invalid("invalid batch length"));
            }
            let mut requests = Vec::with_capacity(count);
            for _ in 0..count {
                let row =
                    line(&mut reader, &mut budget)?.ok_or_else(|| invalid("incomplete frame"))?;
                let fields = row.split('\t').collect::<Vec<_>>();
                let request = match fields.as_slice() {
                    ["reset", slot, seed] => Request {
                        slot: number(slot)?,
                        operation: Operation::Reset {
                            seed: number(seed)?,
                        },
                    },
                    ["step", slot, state, action] => Request {
                        slot: number(slot)?,
                        operation: Operation::Step {
                            state: number(state)?,
                            action: number(action)?,
                        },
                    },
                    _ => return Err(invalid("invalid operation/field count")),
                };
                requests.push(request);
            }
            let responses = self.execute(&requests)?;
            let output = encode(&responses)?;
            writer.write_all(output.as_bytes())?;
            writer.flush()?;
        }
    }
}
fn numeric_text(value: f64) -> String {
    let text = value.to_string();
    if text.len() > 31 {
        format!("{value:.17e}")
    } else {
        text
    }
}
fn encode(responses: &[Response]) -> io::Result<String> {
    let mut out = format!("E14\t{VERSION}\t{}\n", responses.len());
    for response in responses {
        match &response.result {
            Ok(t) => {
                if t.version != VERSION
                    || !t.reward.is_finite()
                    || t.observation.len() > MAX_FEATURES
                    || t.action_mask.is_empty()
                    || t.action_mask.len() > MAX_ACTIONS
                    || t.observation.iter().any(|v| !v.is_finite())
                    || (t.terminated && t.truncated)
                {
                    return Err(invalid(
                        "invalid environment output; connection state uncertain",
                    ));
                }
                let observation = t
                    .observation
                    .iter()
                    .map(|v| numeric_text(*v))
                    .collect::<Vec<_>>()
                    .join(",");
                let mask = t
                    .action_mask
                    .iter()
                    .map(|v| if *v { '1' } else { '0' })
                    .collect::<String>();
                out.push_str(&format!(
                    "ok\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                    response.slot,
                    t.version,
                    t.state,
                    numeric_text(t.reward),
                    u8::from(t.terminated),
                    u8::from(t.truncated),
                    observation,
                    mask
                ));
            }
            Err(error) => {
                // Bounded ASCII diagnostic, with controls removed. Diagnostics
                // are not authoritative evidence; rich logs remain in Rust.
                let message = error
                    .chars()
                    .filter(|c| c.is_ascii() && !c.is_control())
                    .take(512)
                    .collect::<String>();
                out.push_str(&format!(
                    "error\t{}\t{}\n",
                    response.slot,
                    if message.is_empty() {
                        "environment rejected operation"
                    } else {
                        &message
                    }
                ));
            }
        }
        // Bound output before write. No partially serialized response is sent.
        if out.len() > MAX_OUTPUT_BYTES {
            return Err(invalid(
                "output exceeds byte limit; connection state uncertain",
            ));
        }
    }
    Ok(out)
}
