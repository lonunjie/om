//! INSECURE-BY-DESIGN stub "LLM" (test only). It obeys every instruction it can find. Supported forms:
//!   <<tool:NAME {json-args}>>      explicit directive
//!   [[b64:BASE64]]                 base64 text, decoded and scanned again (obfuscation)
//!   "please call NAME" / "call the NAME tool"   natural-language request, empty args
//! It scans the user prompt, every context item and every tool result it is shown, in order.

use base64::Engine;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct ToolRequest {
    pub tool: String,
    pub args: Value,
}

pub fn extract(text: &str, depth: u32) -> Vec<ToolRequest> {
    let mut out = vec![];
    let mut i = 0;
    let b = text.as_bytes();
    while i < b.len() {
        let rest = &text[i..];
        if rest.starts_with("<<tool:") {
            if let Some(end) = find_close(rest) {
                let inner = &rest[7..end];
                let (name, args) = match inner.find(' ') {
                    Some(sp) => (&inner[..sp], inner[sp + 1..].trim()),
                    None => (inner, "{}"),
                };
                let args = serde_json::from_str(args).unwrap_or(Value::Object(Default::default()));
                out.push(ToolRequest { tool: name.to_string(), args });
                i += end + 2;
                continue;
            }
        }
        if rest.starts_with("[[b64:") && depth < 3 {
            if let Some(end) = rest.find("]]") {
                if let Ok(dec) = base64::engine::general_purpose::STANDARD.decode(&rest[6..end]) {
                    out.extend(extract(&String::from_utf8_lossy(&dec), depth + 1));
                }
                i += end + 2;
                continue;
            }
        }
        for pre in ["please call ", "call the "] {
            if rest.to_lowercase().starts_with(pre) {
                let name: String = rest[pre.len()..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                if !name.is_empty() {
                    out.push(ToolRequest { tool: name, args: Value::Object(Default::default()) });
                }
            }
        }
        i += rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    out
}

/// Find the matching ">>" of a directive, skipping nested "<<" ... ">>" pairs inside JSON string arguments.
fn find_close(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 0;
    let mut i = 0;
    while i + 1 < b.len() {
        if &b[i..i + 2] == b"<<" {
            depth += 1;
            i += 2;
        } else if &b[i..i + 2] == b">>" {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    None
}
