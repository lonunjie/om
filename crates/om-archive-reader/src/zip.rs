//! Strict OM-ZIP-v1 container, including fully checked ZIP64 central and local records.
use crate::{ArchiveError, Limits, ZipError};
use std::collections::BTreeSet;

const EOCD: u32 = 0x0605_4b50;
const CEN: u32 = 0x0201_4b50;
const LOC: u32 = 0x0403_4b50;
const ZIP64_END: u32 = 0x0606_4b50;
const ZIP64_LOC: u32 = 0x0706_4b50;

#[derive(Debug)]
pub(crate) struct ZipEntry<'a> {
    pub(crate) name: &'a str,
    pub(crate) data: &'a [u8],
}

fn part(b: &[u8], start: usize, count: usize) -> Result<&[u8], ZipError> {
    let end = start.checked_add(count).ok_or(ZipError::Structure)?;
    b.get(start..end).ok_or(ZipError::Truncated)
}
fn u16at(b: &[u8], start: usize) -> Result<u16, ZipError> {
    let s = part(b, start, 2)?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}
fn u32at(b: &[u8], start: usize) -> Result<u32, ZipError> {
    let s = part(b, start, 4)?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn u64at(b: &[u8], start: usize) -> Result<u64, ZipError> {
    let s = part(b, start, 8)?;
    Ok(u64::from_le_bytes([
        s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
    ]))
}
fn index(v: u64) -> Result<usize, ZipError> {
    usize::try_from(v).map_err(|_| ZipError::Structure)
}
fn add(a: usize, b: usize) -> Result<usize, ZipError> {
    a.checked_add(b).ok_or(ZipError::Structure)
}

pub(crate) fn valid_name(name: &str) -> bool {
    const PREFIX: &str = "om-archive-v1/";
    if name.len() > 255 {
        return false;
    }
    let Some(tail) = name.strip_prefix(PREFIX) else {
        return false;
    };
    let mut count = 0;
    for seg in tail.split('/') {
        count += 1;
        if count > 16 || seg.is_empty() || seg.len() > 128 {
            return false;
        }
        let b = seg.as_bytes();
        if !b[0].is_ascii_lowercase() && !b[0].is_ascii_digit() {
            return false;
        }
        if !b[1..].iter().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(*c, b'.' | b'_' | b'-')
        }) {
            return false;
        }
        if seg == "." || seg == ".." {
            return false;
        }
    }
    count >= 1
}

// No unknown/duplicate/unused extra fields. Return ZIP64 values in specified order.
fn zip64(extra: &[u8], wanted: usize) -> Result<[u64; 3], ZipError> {
    if wanted == 0 {
        if !extra.is_empty() {
            return Err(ZipError::Extra);
        }
        return Ok([0; 3]);
    }
    if extra.len() != 4 + wanted * 8
        || u16at(extra, 0)? != 1
        || usize::from(u16at(extra, 2)?) != wanted * 8
    {
        return Err(ZipError::Extra);
    }
    let mut v = [0u64; 3];
    for (i, slot) in v.iter_mut().enumerate().take(wanted) {
        *slot = u64at(extra, 4 + i * 8)?;
    }
    Ok(v)
}

fn flags_and_method(flags: u16, method: u16) -> Result<(), ZipError> {
    if flags != 0x0800 {
        return Err(ZipError::Flags);
    }
    if method != 0 {
        return Err(ZipError::Compression);
    }
    Ok(())
}

pub(crate) fn parse<'a>(b: &'a [u8], limits: &Limits) -> Result<Vec<ZipEntry<'a>>, ArchiveError> {
    parse_inner(b, limits).map_err(ArchiveError::Zip)
}

fn parse_inner<'a>(b: &'a [u8], limits: &Limits) -> Result<Vec<ZipEntry<'a>>, ZipError> {
    if b.len() < 22 {
        return Err(ZipError::Truncated);
    }
    let end = b.len() - 22;
    if u32at(b, end)? != EOCD {
        return Err(ZipError::Structure);
    }
    if u16at(b, end + 4)? != 0 || u16at(b, end + 6)? != 0 || u16at(b, end + 20)? != 0 {
        return Err(ZipError::Structure);
    }
    let count_disk = u16at(b, end + 8)?;
    let count_eocd = u16at(b, end + 10)?;
    let dir_size_eocd = u32at(b, end + 12)?;
    let dir_offset_eocd = u32at(b, end + 16)?;
    let sentinel = count_eocd == 0xffff
        || count_disk == 0xffff
        || dir_size_eocd == u32::MAX
        || dir_offset_eocd == u32::MAX;
    let locator = end
        .checked_sub(20)
        .filter(|&p| u32at(b, p).ok() == Some(ZIP64_LOC));
    let (count, dir_size, dir_offset, dir_end) = if let Some(loc) = locator {
        if u32at(b, loc + 4)? != 0 || u32at(b, loc + 16)? != 1 {
            return Err(ZipError::Structure);
        }
        let rec = index(u64at(b, loc + 8)?)?;
        if add(rec, 56)? != loc || u32at(b, rec)? != ZIP64_END || u64at(b, rec + 4)? != 44 {
            return Err(ZipError::Structure);
        }
        if u16at(b, rec + 14)? > 45 || u32at(b, rec + 16)? != 0 || u32at(b, rec + 20)? != 0 {
            return Err(ZipError::Structure);
        }
        let count_on_disk = u64at(b, rec + 24)?;
        let total_count = u64at(b, rec + 32)?;
        if count_on_disk != total_count {
            return Err(ZipError::Structure);
        }
        let size = u64at(b, rec + 40)?;
        let offset = u64at(b, rec + 48)?;
        if count_disk != 0xffff && u64::from(count_disk) != total_count
            || count_eocd != 0xffff && u64::from(count_eocd) != total_count
            || dir_size_eocd != u32::MAX && u64::from(dir_size_eocd) != size
            || dir_offset_eocd != u32::MAX && u64::from(dir_offset_eocd) != offset
        {
            return Err(ZipError::Structure);
        }
        (total_count, size, offset, rec)
    } else {
        if sentinel || count_disk != count_eocd {
            return Err(ZipError::Structure);
        }
        (
            u64::from(count_eocd),
            u64::from(dir_size_eocd),
            u64::from(dir_offset_eocd),
            end,
        )
    };
    if count > limits.max_entries {
        return Err(ZipError::CountLimit);
    }
    let central_start = index(dir_offset)?;
    let central_len = index(dir_size)?;
    if add(central_start, central_len)? != dir_end || count > dir_size / 46 {
        return Err(ZipError::Structure);
    }
    // All named fields are bounded by the central directory before looping/allocating.
    let mut cd_pos = central_start;
    let mut local_pos = 0usize;
    let mut total = 0u64;
    let mut names = BTreeSet::new();
    let mut entries = Vec::new();
    for _ in 0..count {
        if add(cd_pos, 46)? > dir_end || u32at(b, cd_pos)? != CEN {
            return Err(ZipError::Structure);
        }
        let made = u16at(b, cd_pos + 4)?;
        let needed = u16at(b, cd_pos + 6)?;
        let flags = u16at(b, cd_pos + 8)?;
        let method = u16at(b, cd_pos + 10)?;
        flags_and_method(flags, method)?;
        if needed > 45 {
            return Err(ZipError::Structure);
        }
        let crc = u32at(b, cd_pos + 16)?;
        let comp32 = u32at(b, cd_pos + 20)?;
        let uncomp32 = u32at(b, cd_pos + 24)?;
        let name_len = usize::from(u16at(b, cd_pos + 28)?);
        let extra_len = usize::from(u16at(b, cd_pos + 30)?);
        let comment_len = usize::from(u16at(b, cd_pos + 32)?);
        if comment_len != 0 || u16at(b, cd_pos + 34)? != 0 || u16at(b, cd_pos + 36)? != 0 {
            return Err(ZipError::Structure);
        }
        let ext = u32at(b, cd_pos + 38)?;
        if made >> 8 == 3 {
            if ext & 0xffff != 0 || ((ext >> 16) & 0o170000) != 0o100000 {
                return Err(ZipError::Mode);
            }
        } else if ext != 0 {
            return Err(ZipError::Mode);
        }
        let off32 = u32at(b, cd_pos + 42)?;
        let name_start = add(cd_pos, 46)?;
        let extra_start = add(name_start, name_len)?;
        let next_cd = add(extra_start, extra_len)?;
        if next_cd > dir_end {
            return Err(ZipError::Truncated);
        }
        let name_bytes = part(b, name_start, name_len)?;
        let name = std::str::from_utf8(name_bytes).map_err(|_| ZipError::Path)?;
        if !valid_name(name) {
            return Err(ZipError::Path);
        }
        let extra = part(b, extra_start, extra_len)?;
        let want_uncomp = uncomp32 == u32::MAX;
        let want_comp = comp32 == u32::MAX;
        let want_offset = off32 == u32::MAX;
        let parts = zip64(
            extra,
            (if want_uncomp { 1 } else { 0 })
                + (if want_comp { 1 } else { 0 })
                + (if want_offset { 1 } else { 0 }),
        )?;
        let mut ix = 0;
        let uncomp = if want_uncomp {
            let x = parts[ix];
            ix += 1;
            x
        } else {
            u64::from(uncomp32)
        };
        let comp = if want_comp {
            let x = parts[ix];
            ix += 1;
            x
        } else {
            u64::from(comp32)
        };
        let off = if want_offset {
            parts[ix]
        } else {
            u64::from(off32)
        };
        if comp != uncomp {
            return Err(ZipError::Compression);
        }
        if uncomp > limits.max_entry_size {
            return Err(ZipError::SizeLimit);
        }
        total = total.checked_add(uncomp).ok_or(ZipError::TotalSizeLimit)?;
        if total > limits.max_total_size {
            return Err(ZipError::TotalSizeLimit);
        }
        if !names.insert(name) {
            return Err(ZipError::Duplicate);
        }
        if index(off)? != local_pos {
            return Err(ZipError::Structure);
        }
        if add(local_pos, 30)? > central_start || u32at(b, local_pos)? != LOC {
            return Err(ZipError::Structure);
        }
        let lneeded = u16at(b, local_pos + 4)?;
        let lflags = u16at(b, local_pos + 6)?;
        let lmethod = u16at(b, local_pos + 8)?;
        if lneeded != needed
            || lflags != flags
            || lmethod != method
            || u32at(b, local_pos + 14)? != crc
        {
            return Err(ZipError::Mismatch);
        }
        let lcomp32 = u32at(b, local_pos + 18)?;
        let luncomp32 = u32at(b, local_pos + 22)?;
        let lname_len = usize::from(u16at(b, local_pos + 26)?);
        let lextra_len = usize::from(u16at(b, local_pos + 28)?);
        let lname_start = add(local_pos, 30)?;
        let lextra_start = add(lname_start, lname_len)?;
        let data_start = add(lextra_start, lextra_len)?;
        if data_start > central_start || part(b, lname_start, lname_len)? != name_bytes {
            return Err(ZipError::Mismatch);
        }
        let local_zip64 = lcomp32 == u32::MAX || luncomp32 == u32::MAX;
        // ZIP64 local headers carry both sizes, never a partial pair.
        if local_zip64 && (lcomp32 != u32::MAX || luncomp32 != u32::MAX) {
            return Err(ZipError::Extra);
        }
        let local_parts = zip64(
            part(b, lextra_start, lextra_len)?,
            if local_zip64 { 2 } else { 0 },
        )?;
        let lcomp = if local_zip64 {
            local_parts[1]
        } else {
            u64::from(lcomp32)
        };
        let luncomp = if local_zip64 {
            local_parts[0]
        } else {
            u64::from(luncomp32)
        };
        if lcomp != comp || luncomp != uncomp {
            return Err(ZipError::Mismatch);
        }
        let data_end = add(data_start, index(uncomp)?)?;
        if data_end > central_start {
            return Err(ZipError::Structure);
        }
        let data = part(b, data_start, index(uncomp)?)?;
        if crc32fast::hash(data) != crc {
            return Err(ZipError::Crc);
        }
        entries.push(ZipEntry { name, data });
        local_pos = data_end;
        cd_pos = next_cd;
    }
    if cd_pos != dir_end || local_pos != central_start {
        return Err(ZipError::Structure);
    }
    Ok(entries)
}
