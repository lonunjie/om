// SPDX-License-Identifier: MPL-2.0
//! Borrowed RIFF and WebP headers only. O(1) scratch space, at most two chunk levels,
//! with animation work bounded by the per-kind frame count. Pixel payloads stay opaque.

use super::{Error, Kind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub duration_ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Facts {
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub duration_ms: Option<u32>,
    pub encoded_size: usize,
    pub total_pixels: u32,
}

struct Budget {
    kind: Kind,
    facts: Facts,
    animated: bool,
}

// The only accumulated state is this fixed-size header summary.
const _: () = assert!(size_of::<Budget>() <= 64);

impl Budget {
    fn new(
        kind: Kind,
        size: usize,
        width: u32,
        height: u32,
        animated: bool,
    ) -> Result<Self, Error> {
        let limits = kind.limits();
        if size == 0 || size > limits.bytes {
            return Err("encoded_size");
        }
        if width == 0 || height == 0 {
            return Err("header_unstated");
        }
        if width > limits.canvas || height > limits.canvas {
            return Err("canvas");
        }
        Ok(Self {
            kind,
            animated,
            facts: Facts {
                width,
                height,
                frames: 0,
                duration_ms: animated.then_some(0),
                encoded_size: size,
                total_pixels: 0,
            },
        })
    }

    fn add(&mut self, frame: Frame) -> Result<(), Error> {
        let f = &mut self.facts;
        let limits = self.kind.limits();
        if f.frames >= limits.frames {
            return Err("frames");
        }
        if frame.width == 0
            || frame.height == 0
            || frame.x.checked_add(frame.width).is_none_or(|x| x > f.width)
            || frame
                .y
                .checked_add(frame.height)
                .is_none_or(|y| y > f.height)
        {
            return Err("frame_rect");
        }
        let pixels = frame.width.checked_mul(frame.height).ok_or("overflow")?;
        let max_pixels = limits.canvas * limits.canvas;
        if pixels > max_pixels {
            return Err("pixels_per_frame");
        }
        f.total_pixels = f.total_pixels.checked_add(pixels).ok_or("overflow")?;
        if f.total_pixels > max_pixels * limits.frames {
            return Err("total_pixels");
        }
        f.frames += 1;
        if let Some(previous) = f.duration_ms {
            let duration = previous.checked_add(frame.duration_ms).ok_or("overflow")?;
            if duration > limits.duration {
                return Err("duration");
            }
            f.duration_ms = Some(duration);
        } else if frame.duration_ms != 0 {
            return Err("duration");
        }
        Ok(())
    }

    fn finish(self) -> Result<Facts, Error> {
        let f = self.facts;
        if f.frames == 0 {
            return Err("header_unstated");
        }
        if (!self.animated && f.frames != 1) || (self.kind == Kind::LoopAnimation && f.frames < 2) {
            return Err("animation_required");
        }
        if let Some(duration) = f.duration_ms
            && f.frames > 1
            && f.frames * 1000 > duration * 30
        {
            return Err("frame_rate");
        }
        Ok(f)
    }
}

/// Validate independently supplied header facts against the same budget as the RIFF walker.
pub fn validate_facts(
    kind: Kind,
    size: usize,
    width: u32,
    height: u32,
    animated: bool,
    frames: &[Frame],
) -> Result<Facts, Error> {
    if width == 0 || height == 0 || frames.is_empty() {
        return Err("header_unstated");
    }
    if size == 0 || size > kind.limits().bytes {
        return Err("encoded_size");
    }
    let count = u32::try_from(frames.len()).map_err(|_| "overflow")?;
    width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(count))
        .ok_or("overflow")?;
    let mut budget = Budget::new(kind, size, width, height, animated)?;
    for frame in frames {
        budget.add(*frame)?;
    }
    budget.finish()
}

fn le24(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0])
}

fn chunk<'a>(input: &mut &'a [u8]) -> Result<(&'a [u8], &'a [u8]), Error> {
    let (head, rest) = input.split_at_checked(8).ok_or("truncated_chunk")?;
    let n = u32::from_le_bytes(head[4..8].try_into().unwrap()) as usize;
    let (payload, mut tail) = rest.split_at_checked(n).ok_or("chunk_size")?;
    if n % 2 != 0 {
        let (padding, after) = tail.split_first().ok_or("truncated_chunk")?;
        if *padding != 0 {
            return Err("chunk_padding");
        }
        tail = after;
    }
    *input = tail;
    Ok((&head[..4], payload))
}

fn dimensions(tag: &[u8], bytes: &[u8]) -> Result<(u32, u32), Error> {
    match tag {
        b"VP8 " if bytes.len() > 10 && bytes[0] & 1 == 0 && &bytes[3..6] == b"\x9d\x01\x2a" => {
            let frame_tag = le24(&bytes[..3]);
            let partition_size = (frame_tag >> 5) as usize;
            if partition_size == 0 || partition_size > bytes.len() - 10 {
                return Err("image_header");
            }
            let width = u16::from_le_bytes([bytes[6], bytes[7]]) & 0x3fff;
            let height = u16::from_le_bytes([bytes[8], bytes[9]]) & 0x3fff;
            Ok((width as u32, height as u32))
        }
        b"VP8L" if bytes.len() > 5 && bytes[0] == 0x2f => {
            let header = u32::from_le_bytes(bytes[1..5].try_into().unwrap());
            if header >> 29 != 0 {
                return Err("image_header");
            }
            Ok(((header & 0x3fff) + 1, ((header >> 14) & 0x3fff) + 1))
        }
        _ => Err("image_header"),
    }
}

fn image(tag: &[u8], bytes: &[u8], mut tail: &[u8], rect: Frame) -> Result<(), Error> {
    let (tag, bytes) = if tag == b"ALPH" && bytes.len() > 1 {
        let (tag, bytes) = chunk(&mut tail)?;
        if tag != b"VP8 " {
            return Err("chunk_order");
        }
        (tag, bytes)
    } else {
        (tag, bytes)
    };
    if !tail.is_empty() || dimensions(tag, bytes)? != (rect.width, rect.height) {
        return Err("frame_dimensions");
    }
    Ok(())
}

fn still(width: u32, height: u32) -> Frame {
    Frame {
        x: 0,
        y: 0,
        width,
        height,
        duration_ms: 0,
    }
}

/// Read the fixed-size container and codec dimensions. Accepting headers only authorizes
/// handing the bytes to the bounded platform decoder.
pub fn walk_webp(kind: Kind, input: &[u8]) -> Result<Facts, Error> {
    if input.len() > kind.limits().bytes {
        return Err("encoded_size");
    }
    let (head, mut tail) = input.split_at_checked(12).ok_or("riff_header")?;
    if &head[..4] != b"RIFF"
        || &head[8..] != b"WEBP"
        || u32::from_le_bytes(head[4..8].try_into().unwrap()) as usize != input.len() - 8
    {
        return Err("riff_header");
    }
    let (tag, payload) = chunk(&mut tail)?;
    if tag != b"VP8X" {
        let (width, height) = dimensions(tag, payload)?;
        if !tail.is_empty() {
            return Err("chunk_order");
        }
        let mut budget = Budget::new(kind, input.len(), width, height, false)?;
        budget.add(still(width, height))?;
        return budget.finish();
    }
    if payload.len() != 10 || payload[0] & 0xc1 != 0 || payload[1..4] != [0; 3] {
        return Err("extended_header");
    }
    let flags = payload[0];
    let animated = flags & 2 != 0;
    let (width, height) = (le24(&payload[4..7]) + 1, le24(&payload[7..10]) + 1);
    let mut budget = Budget::new(kind, input.len(), width, height, animated)?;
    if flags & 0x20 != 0 && chunk(&mut tail)?.0 != b"ICCP" {
        return Err("chunk_order");
    }
    if animated {
        let (tag, anim) = chunk(&mut tail)?;
        if tag != b"ANIM" || anim.len() != 6 {
            return Err("animation_header");
        }
        while tail.starts_with(b"ANMF") {
            let (_, frame) = chunk(&mut tail)?;
            let (head, mut nested) = frame.split_at_checked(16).ok_or("frame_header")?;
            if head[15] & 0xfc != 0 {
                return Err("frame_header");
            }
            let rect = Frame {
                x: le24(&head[..3]) * 2,
                y: le24(&head[3..6]) * 2,
                width: le24(&head[6..9]) + 1,
                height: le24(&head[9..12]) + 1,
                duration_ms: le24(&head[12..15]),
            };
            budget.add(rect)?;
            let (tag, payload) = chunk(&mut nested)?;
            if tag == b"ALPH" && flags & 0x10 == 0 {
                return Err("chunk_order");
            }
            image(tag, payload, nested, rect)?;
        }
    } else {
        let (tag, payload) = chunk(&mut tail)?;
        let rect = still(width, height);
        if tag == b"ALPH" {
            if flags & 0x10 == 0 || payload.len() <= 1 {
                return Err("chunk_order");
            }
            let (tag, payload) = chunk(&mut tail)?;
            if tag != b"VP8 " {
                return Err("chunk_order");
            }
            image(tag, payload, &[], rect)?;
        } else {
            image(tag, payload, &[], rect)?;
        }
        budget.add(rect)?;
    }
    for (flag, name) in [(8, b"EXIF"), (4, b"XMP ")] {
        if flags & flag != 0 && chunk(&mut tail)?.0 != name {
            return Err("chunk_order");
        }
    }
    if !tail.is_empty() {
        return Err("chunk_order");
    }
    budget.finish()
}
