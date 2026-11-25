# SPDX-License-Identifier: MPL-2.0
# Independent second-language codec for LanFrameV1 (spec/cddl/lan.cddl); written from the CDDL alone, reviewed and tested here.
"""Strict, standalone CBOR codec for the closed lan-frame-v1 schema."""

_MAX_UINT = (1 << 64) - 1
_MAX_INPUT = 131068
_MAX_CHUNK = 16384

# Each field rule is (exact Python type, inclusive lower and upper bounds).
# Bounds describe the integer itself or the byte-string length.
_U64 = (int, 0, _MAX_UINT)
_ID16 = (bytes, 16, 16)
_B32 = (bytes, 32, 32)
_CAP = (bytes, 16, 64)
_U16_POS = (int, 1, 65535)
_COMMON = {0: (int, 1, 1), 1: (int, 1, 8), 2: _ID16}
_FIELDS = {
    1: {**_COMMON, 3: (int, 1, 8), 4: _U64, 5: _B32},
    2: {**_COMMON, 6: _U64, 7: (bytes, 1, _MAX_CHUNK)},
    3: {**_COMMON},
    4: {**_COMMON, 14: _U64},
    5: {**_COMMON, 8: _ID16, 9: _U16_POS, 10: _CAP},
    6: {**_COMMON, 10: _CAP, 11: _B32, 12: _B32},
    7: {**_COMMON, 13: _U16_POS},
    8: {**_COMMON, 15: _B32},
}
_MAX_FIELDS = max(map(len, _FIELDS.values()))


def _validate(frame: dict) -> None:
    if type(frame) is not dict:
        raise ValueError('not_map')
    if len(frame) > _MAX_FIELDS:
        raise ValueError('key_set')
    for key in frame:
        if type(key) is not int or not 0 <= key <= _MAX_UINT:
            raise ValueError('invalid_key')
    if 0 not in frame or 1 not in frame:
        raise ValueError('key_set')
    if type(frame[0]) is not int:
        raise ValueError('value_type')
    if frame[0] != 1:
        raise ValueError('invalid_version')
    if type(frame[1]) is not int:
        raise ValueError('value_type')
    if frame[1] not in _FIELDS:
        raise ValueError('unknown_kind')
    fields = _FIELDS[frame[1]]
    if frame.keys() != fields.keys():
        raise ValueError('key_set')
    for key, (expected, lo, hi) in fields.items():
        value = frame[key]
        if type(value) is not expected:
            raise ValueError('value_type')
        if expected is int:
            if not lo <= value <= hi:
                raise ValueError('integer_range')
        elif not lo <= len(value) <= hi:
            raise ValueError('byte_string_size')


def _head(major: int, n: int) -> bytes:
    """Encode a validated nonnegative CBOR argument in its shortest form."""
    prefix = major << 5
    if n < 24:
        return bytes((prefix | n,))
    for ai, width in ((24, 1), (25, 2), (26, 4), (27, 8)):
        if n < 1 << (8 * width):
            return bytes((prefix | ai,)) + n.to_bytes(width, 'big')
    raise ValueError('integer_range')


def encode(frame: dict) -> bytes:
    """Return the canonical encoding of one valid frame, or raise ValueError."""
    _validate(frame)
    # All legal keys are one-byte unsigned integers (0..15). Numeric
    # order therefore equals both encoded-length and bytewise order.
    parts = [_head(5, len(frame))]
    for key in sorted(frame):
        value = frame[key]
        parts.append(_head(0, key))
        if type(value) is int:
            parts.append(_head(0, value))
        else:
            parts.extend((_head(2, len(value)), value))
    return b''.join(parts)


class _Reader:
    def __init__(self, data: bytes):
        self.data = data
        self.pos = 0

    def take(self, count: int) -> bytes:
        if count > len(self.data) - self.pos:
            raise ValueError('truncated')
        start = self.pos
        self.pos += count
        return self.data[start:self.pos]

    def initial(self) -> tuple[int, int]:
        first = self.take(1)[0]
        major, ai = first >> 5, first & 31
        if ai == 31:
            raise ValueError('indefinite_length')
        if 28 <= ai <= 30:
            raise ValueError('invalid_head')
        return major, ai

    def argument(self, ai: int) -> int:
        if ai < 24:
            return ai
        width = 1 << (ai - 24)
        n = int.from_bytes(self.take(width), 'big')
        minimum = (24, 256, 65536, 4294967296)[ai - 24]
        if n < minimum:
            raise ValueError('non_shortest')
        return n

    def value(self) -> int | bytes:
        major, ai = self.initial()
        if major not in (0, 1, 2):
            raise ValueError('unsupported_type')
        n = self.argument(ai)
        if major == 0:
            return n
        if major == 1:
            raise ValueError('integer_range')
        # No byte string in this schema can exceed the chunk bound.
        # Check before slicing, even when a malicious length is enormous.
        if n > _MAX_CHUNK:
            raise ValueError('byte_string_size')
        return self.take(n)


def decode(data: bytes) -> dict:
    """Accept exactly one canonical, schema-valid frame, or raise ValueError."""
    if type(data) is not bytes:
        raise ValueError('input_type')
    if not data:
        raise ValueError('empty_input')
    if len(data) > _MAX_INPUT:
        raise ValueError('input_too_long')
    reader = _Reader(data)
    major, ai = reader.initial()
    if major != 5:
        raise ValueError('not_map')
    count = reader.argument(ai)
    if count > _MAX_FIELDS:
        raise ValueError('key_set')
    frame = {}
    previous = None
    for _ in range(count):
        start = reader.pos
        major, ai = reader.initial()
        if major != 0:
            raise ValueError('invalid_key')
        key = reader.argument(ai)
        encoded = data[start:reader.pos]
        order = (len(encoded), encoded)
        if key in frame:
            raise ValueError('duplicate_key')
        if previous is not None and order <= previous:
            raise ValueError('key_order')
        previous = order
        frame[key] = reader.value()
    if reader.pos != len(data):
        raise ValueError('trailing_bytes')
    _validate(frame)
    return frame
