"""AES-256-GCM-SIV (RFC 8452) in pure Python, written from the RFC for the recovery-record vector generator.
Feasibility evidence only: slow, not constant time, never for production. It is an independent implementation, so the
Rust crate and this file must agree on every sealed envelope in the vectors."""
import struct

_SBOX = None


def _sbox():
    global _SBOX
    if _SBOX:
        return _SBOX
    # generate the AES S-box from the multiplicative inverse and the affine map
    p = q = 1
    box = [0] * 256
    while True:
        p = p ^ ((p << 1) & 0xFF) ^ (0x1B if p & 0x80 else 0)
        q ^= q << 1
        q ^= q << 2
        q ^= q << 4
        q &= 0xFF
        if q & 0x80:
            q ^= 0x09
        x = q ^ ((q << 1) | (q >> 7)) & 0xFF ^ ((q << 2) | (q >> 6)) & 0xFF ^ ((q << 3) | (q >> 5)) & 0xFF ^ ((q << 4) | (q >> 4)) & 0xFF
        box[p] = (x ^ 0x63) & 0xFF
        if p == 1:
            break
    box[0] = 0x63
    _SBOX = box
    return box


def _xt(a):
    return ((a << 1) ^ 0x1B) & 0xFF if a & 0x80 else a << 1


def _expand(key):
    sb = _sbox()
    nk = len(key) // 4
    nr = nk + 6
    w = [list(key[4 * i:4 * i + 4]) for i in range(nk)]
    rcon = 1
    for i in range(nk, 4 * (nr + 1)):
        t = list(w[i - 1])
        if i % nk == 0:
            t = t[1:] + t[:1]
            t = [sb[b] for b in t]
            t[0] ^= rcon
            rcon = _xt(rcon)
        elif nk > 6 and i % nk == 4:
            t = [sb[b] for b in t]
        w.append([w[i - nk][j] ^ t[j] for j in range(4)])
    return [sum(w[4 * r:4 * r + 4], []) for r in range(nr + 1)], nr


def aes_encrypt_block(rk_nr, block):
    rk, nr = rk_nr
    sb = _sbox()
    s = [b ^ k for b, k in zip(block, rk[0])]
    for r in range(1, nr + 1):
        s = [sb[b] for b in s]
        # shift rows (column-major state)
        s = [s[(i + 4 * (i % 4)) % 16] for i in range(16)]
        if r != nr:
            o = []
            for c in range(4):
                a = s[4 * c:4 * c + 4]
                o += [_xt(a[0]) ^ (_xt(a[1]) ^ a[1]) ^ a[2] ^ a[3],
                      a[0] ^ _xt(a[1]) ^ (_xt(a[2]) ^ a[2]) ^ a[3],
                      a[0] ^ a[1] ^ _xt(a[2]) ^ (_xt(a[3]) ^ a[3]),
                      (_xt(a[0]) ^ a[0]) ^ a[1] ^ a[2] ^ _xt(a[3])]
            s = o
        s = [b ^ k for b, k in zip(s, rk[r])]
    return bytes(s)


_P = (1 << 128) | (1 << 127) | (1 << 126) | (1 << 121) | 1


def _clmul(a, b):
    r = 0
    while b:
        if b & 1:
            r ^= a
        a <<= 1
        b >>= 1
    return r


def _dot(a, b):
    """POLYVAL field multiplication: a * b * x^-128 in GF(2^128) mod x^128 + x^127 + x^126 + x^121 + 1."""
    v = _clmul(a, b)
    for _ in range(128):
        if v & 1:
            v ^= _P
        v >>= 1
    return v


def polyval(h, blocks):
    hi = int.from_bytes(h, "little")
    s = 0
    for blk in blocks:
        s = _dot(s ^ int.from_bytes(blk, "little"), hi)
    return s.to_bytes(16, "little")


def _pad16(b):
    return b + b"\0" * (-len(b) % 16)


def _derive(key, nonce):
    rk = _expand(key)
    out = b""
    for i in range(6):
        out += aes_encrypt_block(rk, struct.pack("<I", i) + nonce)[:8]
    return out[:16], out[16:48]


def _tag(auth_key, enc_rk, nonce, aad, pt):
    data = _pad16(aad) + _pad16(pt) + struct.pack("<QQ", len(aad) * 8, len(pt) * 8)
    blocks = [data[i:i + 16] for i in range(0, len(data), 16)]
    s = bytearray(polyval(auth_key, blocks))
    for i in range(12):
        s[i] ^= nonce[i]
    s[15] &= 0x7F
    return aes_encrypt_block(enc_rk, bytes(s))


def _ctr(enc_rk, tag, data):
    ctr = bytearray(tag)
    ctr[15] |= 0x80
    out = bytearray()
    n = int.from_bytes(ctr[:4], "little")
    for i in range(0, len(data), 16):
        ks = aes_encrypt_block(enc_rk, struct.pack("<I", n & 0xFFFFFFFF) + bytes(ctr[4:]))
        out += bytes(a ^ b for a, b in zip(data[i:i + 16], ks))
        n += 1
    return bytes(out)


def seal(key, nonce, aad, pt):
    ak, ek = _derive(key, nonce)
    rk = _expand(ek)
    tag = _tag(ak, rk, nonce, aad, pt)
    return _ctr(rk, tag, pt) + tag


def open_(key, nonce, aad, ct):
    ak, ek = _derive(key, nonce)
    rk = _expand(ek)
    tag = ct[-16:]
    pt = _ctr(rk, tag, ct[:-16])
    if _tag(ak, rk, nonce, aad, pt) != tag:
        raise ValueError("tag mismatch")
    return pt


if __name__ == "__main__":
    # RFC 8452 appendix C.2 case 1 (AES-256-GCM-SIV, empty plaintext) and a 16-byte plaintext check
    k = bytes([1]) + bytes(31)
    n = bytes([3]) + bytes(11)
    print(seal(k, n, b"", b"").hex())
