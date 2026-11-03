//! Copies text through the terminal with OSC 52, so it also works over SSH.

use std::io::{self, Write};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Asks the terminal to put `text` on the system clipboard; unsupported terminals ignore it.
pub fn copy(text: &str) -> io::Result<()> {
    let mut stdout = io::stdout();
    stdout.write_all(osc52(text).as_bytes())?;
    stdout.flush()
}

fn osc52(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64(text.as_bytes()))
}

fn base64(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let group = chunk.iter().enumerate().fold(0u32, |group, (index, byte)| {
            group | u32::from(*byte) << (16 - 8 * index)
        });
        // A chunk of n bytes fills n + 1 characters; the rest are padding.
        for index in 0..4 {
            encoded.push(if index <= chunk.len() {
                ALPHABET[(group >> (18 - 6 * index) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    encoded
}

#[cfg(test)]
#[path = "tests/clipboard.rs"]
mod tests;
