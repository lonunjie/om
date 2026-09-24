//! OM-ZIP-v1: the strict, stored-only ZIP profile used as the om-archive-v1 container (OM_ARCHIVE_V1_SPEC.md §3).
//! Based on PKWARE APPNOTE 6.3.10 (local/central headers, ZIP64 extensions). The reader accepts exactly one
//! canonical layout: entries contiguous from offset 0, central directory directly after the last entry, (ZIP64
//! EOCD + locator,) EOCD with an empty comment as the final bytes. Feasibility prototype, not product code.

use std::collections::HashSet;

pub const PREFIX: &str = "om-archive-v1/";
const SIG_LOCAL: u32 = 0x0403_4b50;
const SIG_CENTRAL: u32 = 0x0201_4b50;
const SIG_EOCD: u32 = 0x0605_4b50;
const SIG_Z64_EOCD: u32 = 0x0606_4b50;
const SIG_Z64_LOC: u32 = 0x0706_4b50;
const FLAG_UTF8: u16 = 0x0800;
const DOS_DATE_1980_01_01: u16 = 0x0021;
const UNIX_REGULAR_0644: u32 = 0o100644;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZipError {
    NotZip,
    Truncated,
    TrailingOrPrependedData,
    MultiDisk,
    TooManyEntries,
    EntryTooLarge,
    TotalTooLarge,
    UnsupportedMethod,
    UnsupportedFlags,
    DataDescriptor,
    Encrypted,
    BadName,
    DuplicateName,
    NotRegularFile,
    ExtraField,
    CommentPresent,
    HeaderMismatch,
    NonContiguous,
    Crc,
    Zip64Inconsistent,
}

#[derive(Clone, Debug)]
pub struct Limits {
    pub max_entries: u64,
    pub max_entry_size: u64,
    pub max_total_size: u64,
}

impl Default for Limits {
    fn default() -> Self {
        // Spec §3.6 minimum values every conforming reader must support.
        Limits { max_entries: 1_000_000, max_entry_size: 64 << 30, max_total_size: 1 << 40 }
    }
}

/// Spec §3.3 entry-name grammar: "om-archive-v1/" then 1..=16 segments of [a-z0-9][a-z0-9._-]{0,127}, total <= 255
/// bytes. This alone excludes absolute paths, "..", ".", backslashes, drive letters, NUL, non-ASCII look-alikes,
/// case-folding duplicates and directory entries.
pub fn valid_name(name: &[u8]) -> bool {
    if name.len() > 255 || !name.starts_with(PREFIX.as_bytes()) {
        return false;
    }
    let rest = &name[PREFIX.len()..];
    let segs: Vec<&[u8]> = rest.split(|&c| c == b'/').collect();
    if segs.is_empty() || segs.len() > 16 {
        return false;
    }
    segs.iter().all(|s| {
        !s.is_empty()
            && s.len() <= 128
            && (s[0].is_ascii_lowercase() || s[0].is_ascii_digit())
            && s.iter().all(|&c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.' || c == b'_' || c == b'-')
    })
}

fn p16(o: &mut Vec<u8>, v: u16) {
    o.extend_from_slice(&v.to_le_bytes())
}
fn p32(o: &mut Vec<u8>, v: u32) {
    o.extend_from_slice(&v.to_le_bytes())
}
fn p64(o: &mut Vec<u8>, v: u64) {
    o.extend_from_slice(&v.to_le_bytes())
}

/// Canonical writer. ZIP64 records are emitted iff required (or when `force_zip64` is set, which exists only to
/// exercise the reader's ZIP64 path in tests; spec §3.5 says readers MUST accept both).
pub fn write(entries: &[(String, Vec<u8>)], force_zip64: bool) -> Vec<u8> {
    for (name, _) in entries {
        assert!(valid_name(name.as_bytes()), "writer given invalid name {name}");
    }
    write_unchecked(entries, force_zip64)
}

/// TEST-ONLY: same layout as `write` but without name validation, for building hostile-corpus cases.
pub fn write_unchecked(entries: &[(String, Vec<u8>)], force_zip64: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    let big = |n: u64| n >= 0xffff_ffff;
    let mut any64 = force_zip64 || entries.len() >= 0xffff;
    for (name, data) in entries {
        let off = out.len() as u64;
        let size = data.len() as u64;
        let z64 = force_zip64 || big(size) || big(off);
        any64 |= z64;
        let crc = crc32fast::hash(data);
        let need: u16 = if z64 { 45 } else { 20 };
        // local header
        p32(&mut out, SIG_LOCAL);
        p16(&mut out, need);
        p16(&mut out, FLAG_UTF8);
        p16(&mut out, 0);
        p16(&mut out, 0);
        p16(&mut out, DOS_DATE_1980_01_01);
        p32(&mut out, crc);
        let s32 = if z64 { 0xffff_ffff } else { size as u32 };
        p32(&mut out, s32);
        p32(&mut out, s32);
        p16(&mut out, name.len() as u16);
        p16(&mut out, if z64 { 20 } else { 0 });
        out.extend_from_slice(name.as_bytes());
        if z64 {
            p16(&mut out, 1);
            p16(&mut out, 16);
            p64(&mut out, size);
            p64(&mut out, size);
        }
        out.extend_from_slice(data);
        // central header
        p32(&mut central, SIG_CENTRAL);
        p16(&mut central, 0x0300 | need); // made by: Unix
        p16(&mut central, need);
        p16(&mut central, FLAG_UTF8);
        p16(&mut central, 0);
        p16(&mut central, 0);
        p16(&mut central, DOS_DATE_1980_01_01);
        p32(&mut central, crc);
        p32(&mut central, s32);
        p32(&mut central, s32);
        p16(&mut central, name.len() as u16);
        p16(&mut central, if z64 { 28 } else { 0 });
        p16(&mut central, 0); // comment
        p16(&mut central, 0); // disk
        p16(&mut central, 0); // internal attrs
        p32(&mut central, UNIX_REGULAR_0644 << 16);
        p32(&mut central, if z64 { 0xffff_ffff } else { off as u32 });
        central.extend_from_slice(name.as_bytes());
        if z64 {
            p16(&mut central, 1);
            p16(&mut central, 24);
            p64(&mut central, size);
            p64(&mut central, size);
            p64(&mut central, off);
        }
    }
    let cd_off = out.len() as u64;
    let cd_size = central.len() as u64;
    out.extend_from_slice(&central);
    any64 |= big(cd_off) || big(cd_size);
    let n = entries.len() as u64;
    if any64 {
        let z_off = out.len() as u64;
        p32(&mut out, SIG_Z64_EOCD);
        p64(&mut out, 44);
        p16(&mut out, 0x0300 | 45);
        p16(&mut out, 45);
        p32(&mut out, 0);
        p32(&mut out, 0);
        p64(&mut out, n);
        p64(&mut out, n);
        p64(&mut out, cd_size);
        p64(&mut out, cd_off);
        p32(&mut out, SIG_Z64_LOC);
        p32(&mut out, 0);
        p64(&mut out, z_off);
        p32(&mut out, 1);
    }
    p32(&mut out, SIG_EOCD);
    p16(&mut out, 0);
    p16(&mut out, 0);
    let n16 = if any64 { 0xffff } else { n as u16 };
    p16(&mut out, n16);
    p16(&mut out, n16);
    p32(&mut out, if any64 { 0xffff_ffff } else { cd_size as u32 });
    p32(&mut out, if any64 { 0xffff_ffff } else { cd_off as u32 });
    p16(&mut out, 0);
    out
}

struct R<'a> {
    b: &'a [u8],
}
impl<'a> R<'a> {
    fn u16(&self, at: u64) -> Result<u16, ZipError> {
        let a = usize::try_from(at).map_err(|_| ZipError::Truncated)?;
        let e = a.checked_add(2).ok_or(ZipError::Truncated)?;
        self.b.get(a..e).map(|s| u16::from_le_bytes(s.try_into().unwrap())).ok_or(ZipError::Truncated)
    }
    fn u32(&self, at: u64) -> Result<u32, ZipError> {
        let a = usize::try_from(at).map_err(|_| ZipError::Truncated)?;
        let e = a.checked_add(4).ok_or(ZipError::Truncated)?;
        self.b.get(a..e).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or(ZipError::Truncated)
    }
    fn u64(&self, at: u64) -> Result<u64, ZipError> {
        let a = usize::try_from(at).map_err(|_| ZipError::Truncated)?;
        let e = a.checked_add(8).ok_or(ZipError::Truncated)?;
        self.b.get(a..e).map(|s| u64::from_le_bytes(s.try_into().unwrap())).ok_or(ZipError::Truncated)
    }
    fn slice(&self, at: u64, n: u64) -> Result<&'a [u8], ZipError> {
        let end = at.checked_add(n).ok_or(ZipError::Truncated)?;
        if end > self.b.len() as u64 {
            return Err(ZipError::Truncated);
        }
        Ok(&self.b[at as usize..end as usize])
    }
}

#[derive(Debug, Clone)]
pub struct Entry<'a> {
    pub name: String,
    pub data: &'a [u8],
}

struct Central {
    name: Vec<u8>,
    crc: u32,
    size: u64,
    offset: u64,
    need: u16,
}

/// Parse extra fields: only ZIP64 (0x0001) is allowed, and it must carry exactly the fields marked 0xFFFFFFFF.
fn zip64_extra(extra: &[u8], want: usize) -> Result<Vec<u64>, ZipError> {
    if extra.is_empty() {
        return if want == 0 { Ok(vec![]) } else { Err(ZipError::Zip64Inconsistent) };
    }
    if extra.len() < 4 {
        return Err(ZipError::ExtraField);
    }
    let tag = u16::from_le_bytes([extra[0], extra[1]]);
    let len = u16::from_le_bytes([extra[2], extra[3]]) as usize;
    if tag != 1 || len + 4 != extra.len() {
        return Err(ZipError::ExtraField);
    }
    if len != want * 8 {
        return Err(ZipError::Zip64Inconsistent);
    }
    Ok((0..want).map(|i| u64::from_le_bytes(extra[4 + 8 * i..12 + 8 * i].try_into().unwrap())).collect())
}

pub fn read<'a>(bytes: &'a [u8], lim: &Limits) -> Result<Vec<Entry<'a>>, ZipError> {
    let r = R { b: bytes };
    let len = bytes.len() as u64;
    if len < 22 {
        return Err(ZipError::NotZip);
    }
    let eocd = len - 22;
    if r.u32(eocd)? != SIG_EOCD {
        // a comment or appended bytes move the EOCD away from the end
        return Err(if bytes.windows(4).any(|w| w == SIG_EOCD.to_le_bytes()) { ZipError::TrailingOrPrependedData } else { ZipError::NotZip });
    }
    if r.u16(eocd + 20)? != 0 {
        return Err(ZipError::CommentPresent);
    }
    if r.u16(eocd + 4)? != 0 || r.u16(eocd + 6)? != 0 {
        return Err(ZipError::MultiDisk);
    }
    let (n16a, n16b) = (r.u16(eocd + 8)?, r.u16(eocd + 10)?);
    let (sz32, off32) = (r.u32(eocd + 12)?, r.u32(eocd + 16)?);
    let (count, cd_size, cd_off, cd_end);
    if n16a == 0xffff || n16b == 0xffff || sz32 == 0xffff_ffff || off32 == 0xffff_ffff {
        if !(n16a == 0xffff && n16b == 0xffff && sz32 == 0xffff_ffff && off32 == 0xffff_ffff) {
            return Err(ZipError::Zip64Inconsistent);
        }
        if eocd < 20 + 56 {
            return Err(ZipError::Truncated);
        }
        let loc = eocd - 20;
        if r.u32(loc)? != SIG_Z64_LOC || r.u32(loc + 4)? != 0 || r.u32(loc + 16)? != 1 {
            return Err(ZipError::Zip64Inconsistent);
        }
        let z = r.u64(loc + 8)?;
        if z != loc - 56 || r.u32(z)? != SIG_Z64_EOCD || r.u64(z + 4)? != 44 {
            return Err(ZipError::Zip64Inconsistent);
        }
        if r.u32(z + 16)? != 0 || r.u32(z + 20)? != 0 {
            return Err(ZipError::MultiDisk);
        }
        let (a, b) = (r.u64(z + 24)?, r.u64(z + 32)?);
        if a != b {
            return Err(ZipError::MultiDisk);
        }
        count = a;
        cd_size = r.u64(z + 40)?;
        cd_off = r.u64(z + 48)?;
        cd_end = z;
    } else {
        if n16a != n16b {
            return Err(ZipError::MultiDisk);
        }
        count = n16a as u64;
        cd_size = sz32 as u64;
        cd_off = off32 as u64;
        cd_end = eocd;
    }
    if count > lim.max_entries {
        return Err(ZipError::TooManyEntries);
    }
    if cd_off.checked_add(cd_size) != Some(cd_end) {
        return Err(ZipError::NonContiguous);
    }
    // every central header is at least 46 bytes: reject impossible counts before allocating
    if count.saturating_mul(46) > cd_size {
        return Err(ZipError::Truncated);
    }
    let mut cen = Vec::with_capacity(count as usize);
    let mut p = cd_off;
    let mut names = HashSet::new();
    let mut total: u64 = 0;
    for _ in 0..count {
        if r.u32(p)? != SIG_CENTRAL {
            return Err(ZipError::Truncated);
        }
        let made_by = r.u16(p + 4)?;
        let need = r.u16(p + 6)?;
        let flags = r.u16(p + 8)?;
        let method = r.u16(p + 10)?;
        let crc = r.u32(p + 16)?;
        let (cs, us) = (r.u32(p + 20)?, r.u32(p + 24)?);
        let (nl, xl, cl) = (r.u16(p + 28)? as u64, r.u16(p + 30)? as u64, r.u16(p + 32)?);
        let (disk, int_attr, ext_attr, off) = (r.u16(p + 34)?, r.u16(p + 36)?, r.u32(p + 38)?, r.u32(p + 42)?);
        if method != 0 {
            return Err(ZipError::UnsupportedMethod); // deflate/bzip2/... : decompression bombs never reach a decoder
        }
        if flags & 0x0001 != 0 || flags & 0x0040 != 0 || flags & 0x2000 != 0 {
            return Err(ZipError::Encrypted);
        }
        if flags & 0x0008 != 0 {
            return Err(ZipError::DataDescriptor);
        }
        if flags != FLAG_UTF8 || need > 45 {
            return Err(ZipError::UnsupportedFlags);
        }
        if cl != 0 {
            return Err(ZipError::CommentPresent);
        }
        if disk != 0 {
            return Err(ZipError::MultiDisk);
        }
        let host = made_by >> 8;
        let mode = ext_attr >> 16;
        let regular = if host == 3 { mode & 0o170000 == 0o100000 && ext_attr & 0xffff == 0 } else { ext_attr == 0 };
        if !regular || int_attr != 0 {
            return Err(ZipError::NotRegularFile); // symlinks, directories, devices, DOS dir/volume bits
        }
        let name = r.slice(p + 46, nl)?.to_vec();
        if !valid_name(&name) {
            return Err(ZipError::BadName);
        }
        if !names.insert(name.clone()) {
            return Err(ZipError::DuplicateName);
        }
        let want = [us == 0xffff_ffff, cs == 0xffff_ffff, off == 0xffff_ffff];
        let z = zip64_extra(r.slice(p + 46 + nl, xl)?, want.iter().filter(|x| **x).count())?;
        let mut zi = z.into_iter();
        let usize_ = if want[0] { zi.next().unwrap() } else { us as u64 };
        let csize = if want[1] { zi.next().unwrap() } else { cs as u64 };
        let offset = if want[2] { zi.next().unwrap() } else { off as u64 };
        if usize_ != csize {
            return Err(ZipError::HeaderMismatch);
        }
        if usize_ > lim.max_entry_size {
            return Err(ZipError::EntryTooLarge);
        }
        total = total.saturating_add(usize_);
        if total > lim.max_total_size {
            return Err(ZipError::TotalTooLarge);
        }
        cen.push(Central { name, crc, size: usize_, offset, need });
        p += 46 + nl + xl;
    }
    if p != cd_end {
        return Err(ZipError::NonContiguous);
    }
    // local headers: contiguous from offset 0 in central-directory order, identical fields
    let mut expect = 0u64;
    let mut out = Vec::with_capacity(cen.len());
    for c in &cen {
        if c.offset != expect {
            return Err(if c.offset < expect { ZipError::NonContiguous } else if expect == 0 { ZipError::TrailingOrPrependedData } else { ZipError::NonContiguous });
        }
        let h = c.offset;
        if r.u32(h)? != SIG_LOCAL {
            return Err(ZipError::HeaderMismatch);
        }
        let (need, flags, method) = (r.u16(h + 4)?, r.u16(h + 6)?, r.u16(h + 8)?);
        let crc = r.u32(h + 14)?;
        let (cs, us) = (r.u32(h + 18)?, r.u32(h + 22)?);
        let (nl, xl) = (r.u16(h + 26)? as u64, r.u16(h + 28)? as u64);
        if need != c.need || flags != FLAG_UTF8 || method != 0 || crc != c.crc {
            return Err(ZipError::HeaderMismatch);
        }
        if r.slice(h + 30, nl)? != c.name.as_slice() {
            return Err(ZipError::HeaderMismatch);
        }
        let lz = [us == 0xffff_ffff, cs == 0xffff_ffff];
        if lz[0] != lz[1] {
            return Err(ZipError::Zip64Inconsistent);
        }
        let size = if lz[0] {
            let z = zip64_extra(r.slice(h + 30 + nl, xl)?, 2)?;
            if z[0] != z[1] {
                return Err(ZipError::HeaderMismatch);
            }
            z[0]
        } else {
            if xl != 0 {
                return Err(ZipError::ExtraField);
            }
            us as u64
        };
        if size != c.size || (cs as u64 != c.size && !lz[1]) {
            return Err(ZipError::HeaderMismatch);
        }
        let data = r.slice(h + 30 + nl + xl, size)?;
        if crc32fast::hash(data) != c.crc {
            return Err(ZipError::Crc);
        }
        expect = h + 30 + nl + xl + size;
        out.push(Entry { name: String::from_utf8(c.name.clone()).unwrap(), data });
    }
    if expect != cd_off {
        return Err(if cen.is_empty() && cd_off != 0 { ZipError::TrailingOrPrependedData } else { ZipError::NonContiguous });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names() {
        assert!(valid_name(b"om-archive-v1/e/00000000"));
        for bad in [&b"om-archive-v1/../x"[..], b"/om-archive-v1/x", b"om-archive-v1/A", b"om-archive-v1/x/", b"om-archive-v1/.x",
                    b"om-archive-v1/a\\b", b"om-archive-v1/a\0", b"om-archive-v1/", b"x/om-archive-v1/a", "om-archive-v1/\u{2024}\u{2024}".as_bytes()] {
            assert!(!valid_name(bad), "{:?}", String::from_utf8_lossy(bad));
        }
    }
    #[test]
    fn roundtrip_plain_and_zip64() {
        let e = vec![("om-archive-v1/format".to_string(), b"x".to_vec()), ("om-archive-v1/b".to_string(), vec![])];
        for f in [false, true] {
            let z = write(&e, f);
            let got = read(&z, &Limits::default()).unwrap();
            assert_eq!(got.len(), 2);
            assert_eq!(got[0].data, b"x");
        }
    }
}
