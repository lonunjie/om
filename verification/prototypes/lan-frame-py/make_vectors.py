# SPDX-License-Identifier: MPL-2.0
# Independent second-language codec for LanFrameV1 (spec/cddl/lan.cddl); written from the CDDL alone, reviewed and tested here.
"""Independent wire fixtures; run with Python 3.11 to write vectors.json."""
import json
from pathlib import Path

MAX = (1 << 64) - 1
ID = bytes(range(16))
DIGEST = bytes(range(32))


def h(s):
    return bytes.fromhex(s)


def u(n):
    # Fixture-only construction; intentionally independent of lanframe.py.
    if n <= 23:
        return bytes([n])
    if n <= 0xff:
        return b'\x18' + bytes([n])
    if n <= 0xffff:
        return b'\x19' + n.to_bytes(2, 'big')
    if n <= 0xffffffff:
        return b'\x1a' + n.to_bytes(4, 'big')
    return b'\x1b' + n.to_bytes(8, 'big')


def bs(payload):
    length = len(payload)
    if length < 24:
        return bytes([0x40 + length]) + payload
    if length <= 255:
        return b'\x58' + bytes([length]) + payload
    if length <= 65535:
        return b'\x59' + length.to_bytes(2, 'big') + payload
    return b'\x5a' + length.to_bytes(4, 'big') + payload


def packet(pairs):
    # Pairs contain explicit wire bytes, not logical values. Preserve order,
    # duplicates, nonminimal headers and malformed values exactly as supplied.
    return bytes([0xa0 + len(pairs)]) + b''.join(k + v for k, v in pairs)


def fields(kind, *extra):
    return [(h('00'), h('01')), (h('01'), u(kind)), (h('02'), bs(ID)), *extra]


def pair(key, wire):
    return (u(key), wire)


def changed(pairs, key, wire):
    return [(k, wire if k == u(key) else v) for k, v in pairs]


def valid_cases():
    # Expected logical frames and independently assembled canonical bytes.
    cases = []

    def add(name, kind, identifier, extra):
        frame = {0: 1, 1: kind, 2: identifier, **extra}
        pairs = [(u(k), u(v) if type(v) is int else bs(v))
                 for k, v in sorted(frame.items())]
        cases.append((name, packet(pairs), frame))

    add('begin_min', 1, ID, {3: 1, 4: 0, 5: DIGEST})
    add('begin_max', 1, b'\xff' * 16, {3: 8, 4: MAX, 5: b'\xff' * 32})
    add('chunk_min', 2, ID, {6: 0, 7: b'\x00'})
    add('chunk_max', 2, ID, {6: MAX, 7: bytes(range(256)) * 64})
    add('end_first', 3, ID, {})
    add('end_second', 3, b'\x00' * 16, {})
    add('ack_min', 4, ID, {14: 0})
    add('ack_max', 4, ID, {14: MAX})
    add('async_min', 5, ID, {8: ID[::-1], 9: 1, 10: b'\x80' * 16})
    add('async_max', 5, ID, {8: ID, 9: 65535, 10: b'\xff' * 64})
    add('blob_min', 6, ID, {10: b'\x00' * 16, 11: DIGEST, 12: DIGEST[::-1]})
    add('blob_max', 6, ID, {10: b'\x01' * 64, 11: b'\xff' * 32, 12: DIGEST})
    add('error_min', 7, ID, {13: 1})
    add('error_max', 7, ID, {13: 65535})
    add('receipt_first', 8, ID, {15: DIGEST})
    add('receipt_second', 8, b'\x00' * 16, {15: b'\x00' * 32})
    for offset in (23, 24, 255, 256, 65535, 65536, 4294967295, 4294967296):
        add(f'ack_uint_boundary_{offset}', 4, ID, {14: offset})
    for size in (23, 24, 255, 256):
        add(f'chunk_length_boundary_{size}', 2, ID, {6: 0, 7: b'\xa5' * size})
    return cases


def hostile_cases():
    """Hand-built CBOR inputs with explicit, independently assigned reasons."""
    cases = []

    def add(name, data, reason):
        cases.append((name, data, reason))

    end = fields(3)
    begin = fields(1, pair(3, h('01')), pair(4, h('00')), pair(5, bs(DIGEST)))
    chunk = fields(2, pair(6, h('00')), pair(7, bs(b'x')))
    ack = fields(4, pair(14, h('00')))
    async_key = fields(5, pair(8, bs(ID)), pair(9, h('01')), pair(10, bs(ID)))
    blob = fields(6, pair(10, bs(ID)), pair(11, bs(DIGEST)), pair(12, bs(DIGEST)))
    error = fields(7, pair(13, h('01')))
    receipt = fields(8, pair(15, bs(DIGEST)))
    good_end = packet(end)
    add('empty', b'', 'empty_input')
    add('over_input_limit', good_end + b'\x00' * (131069 - len(good_end)), 'input_too_long')
    add('at_input_limit_trailing', good_end + b'\x00' * (131068 - len(good_end)), 'trailing_bytes')
    add('trailing_uint', good_end + h('00'), 'trailing_bytes')
    add('trailing_frame', good_end + good_end, 'trailing_bytes')
    add('trailing_break', good_end + h('ff'), 'trailing_bytes')
    for name, wire in [('uint', '01'), ('negative', '20'), ('bstr', '40'), ('text', '60'),
                       ('array', '80'), ('tag', 'c000'), ('simple', 'f4'), ('float', 'f90000')]:
        add('top_' + name, h(wire), 'not_map')
    add('indefinite_map', h('bf') + good_end[1:] + h('ff'), 'indefinite_length')
    for wire in ('b803', 'b90003', 'ba00000003', 'bb0000000000000003'):
        add('nonminimal_map_' + wire, h(wire) + good_end[1:], 'non_shortest')
    for wire in ('1800', '190000', '1a00000000', '1b0000000000000000'):
        add('nonminimal_key_' + wire, packet([(h(wire), h('01')), *end[1:]]), 'non_shortest')
    for wire in ('1801', '190001', '1a00000001', '1b0000000000000001'):
        add('nonminimal_uint_' + wire, packet(changed(end, 0, h(wire))), 'non_shortest')
    for wire in ('5810', '590010', '5a00000010', '5b0000000000000010'):
        add('nonminimal_bstr_' + wire, packet(changed(end, 2, h(wire) + ID)), 'non_shortest')
    for name, wire in [('negative', '20'), ('bytes', '40'), ('text', '6130'), ('array', '80'),
                       ('map', 'a0'), ('tag', 'c000'), ('simple', 'f6'), ('float', 'f90000')]:
        add('key_' + name, packet([(h(wire), h('01')), *end[1:]]), 'invalid_key')
    add('indefinite_key', packet([(h('5f40ff'), h('01')), *end[1:]]), 'indefinite_length')
    add('duplicate_version', packet([end[0], end[0], *end[1:]]), 'duplicate_key')
    add('duplicate_id', packet([*end, end[2]]), 'duplicate_key')
    add('duplicate_nonadjacent', packet([*end, end[0]]), 'duplicate_key')
    add('descending_keys', packet([end[1], end[0], end[2]]), 'key_order')
    add('last_key_before_first', packet([end[2], *end[:2]]), 'key_order')
    add('long_key_before_short', packet([*end[:2], pair(24, h('00')), end[2]]), 'key_order')
    add('same_width_long_key_order', packet([*end, pair(25, h('00')), pair(24, h('00'))]), 'key_order')
    for name, wire in [('text', '6161'), ('array', '8100'), ('nested_map', 'a10000'),
                       ('tag', 'c001'), ('bignum_above_u64', 'c249010000000000000000'),
                       ('negative_bignum', 'c349010000000000000000'), ('false', 'f4'), ('true', 'f5'),
                       ('null', 'f6'), ('undefined', 'f7'), ('simple', 'f820'),
                       ('half_float', 'f93c00'), ('single_float', 'fa3f800000'),
                       ('double_float', 'fb3ff0000000000000')]:
        add('value_' + name, packet(changed(end, 2, h(wire))), 'unsupported_type')
    for name, wire in [('bytes', '5f4161ff'), ('text', '7f6161ff'), ('array', '9f00ff'),
                       ('map', 'bf0000ff'), ('break', 'ff'), ('uint', '1f')]:
        add('indefinite_value_' + name, packet(changed(end, 2, h(wire))), 'indefinite_length')
    for name, wire in [('top', 'bc'), ('key', '1c'), ('value', '5e')]:
        data = h(wire) if name == 'top' else (packet([(h(wire), h('01')), *end[1:]]) if name == 'key'
                                             else packet(changed(end, 2, h(wire))))
        add('reserved_head_' + name, data, 'invalid_head')
    for name, wire in [('map_argument', 'b9'), ('key_argument', 'a11a00'), ('value_argument', 'a1001b00'),
                       ('missing_key', 'a1'), ('missing_value', 'a100'), ('bstr_argument', 'a10059')]:
        add('truncated_' + name, h(wire), 'truncated')
    add('truncated_id_payload', good_end[:-1], 'truncated')
    add('truncated_chunk_payload', packet(chunk)[:-1], 'truncated')
    add('huge_map_count', h('bbffffffffffffffff'), 'key_set')
    add('huge_byte_length', packet(changed(chunk, 7, h('5bffffffffffffffff'))), 'byte_string_size')
    add('empty_map', h('a0'), 'key_set')
    for kind, pairs in enumerate((begin, chunk, end, ack, async_key, blob, error, receipt), 1):
        for key, _ in pairs:
            add(f'kind_{kind}_missing_key_{int.from_bytes(key, "big")}',
                packet([(k, v) for k, v in pairs if k != key]), 'key_set')
        # A field of a different frame kind, in ascending order.
        extra = 15 if kind == 4 else 14
        foreign = sorted([*pairs, pair(extra, h('00'))])
        add(f'kind_{kind}_foreign_key', packet(foreign), 'key_set')
    add('receipt_unknown_key_16', packet([*receipt, pair(16, h('00'))]), 'key_set')
    add('unknown_key_23', packet([*end, pair(23, h('00'))]), 'key_set')
    add('unknown_key_24', packet([*end, pair(24, h('00'))]), 'key_set')
    add('unknown_key_max', packet([*end, pair(MAX, h('00'))]), 'key_set')
    add('wrong_version_zero', packet(changed(end, 0, h('00'))), 'invalid_version')
    add('wrong_version_two', packet(changed(end, 0, h('02'))), 'invalid_version')
    add('wrong_version_max', packet(changed(end, 0, u(MAX))), 'invalid_version')
    for kind in (0, 9, 255, MAX):
        add(f'unknown_kind_{kind}', packet(changed(end, 1, u(kind))), 'unknown_kind')
    for name, pairs, key in [('version', end, 0), ('kind', end, 1), ('object_kind', begin, 3),
                             ('total', begin, 4), ('offset', chunk, 6), ('ack_offset', ack, 14),
                             ('profile', async_key, 9), ('error_code', error, 13)]:
        add('bytes_in_' + name, packet(changed(pairs, key, bs(b'x'))), 'value_type')
        add('negative_' + name, packet(changed(pairs, key, h('20'))), 'integer_range')
    for name, pairs, key, sizes in [
        ('id', end, 2, (0, 15, 17)), ('digest', begin, 5, (0, 31, 33)),
        ('chunk', chunk, 7, (0, 16385)), ('device_id', async_key, 8, (0, 15, 17)),
        ('async_cap', async_key, 10, (0, 15, 65)), ('blob_cap', blob, 10, (0, 15, 65)),
        ('blob_id', blob, 11, (0, 31, 33)), ('ciphertext_digest', blob, 12, (0, 31, 33)),
        ('receipt_tag', receipt, 15, (0, 31, 33)),
    ]:
        add('uint_in_' + name, packet(changed(pairs, key, h('00'))), 'value_type')
        for size in sizes:
            add(f'{name}_size_{size}', packet(changed(pairs, key, bs(b'x' * size))), 'byte_string_size')
    for name, pairs, key, nums in [('object_kind', begin, 3, (0, 9, MAX)),
                                  ('profile', async_key, 9, (0, 65536, MAX)),
                                  ('error_code', error, 13, (0, 65536, MAX))]:
        for n in nums:
            add(f'{name}_range_{n}', packet(changed(pairs, key, u(n))), 'integer_range')
    return cases


def json_frame(frame):
    return {str(k): v.hex() if type(v) is bytes else v for k, v in frame.items()}


def vectors():
    return [dict(name=name, hex=wire.hex(), accept=True, reason=None, frame=json_frame(frame))
            for name, wire, frame in valid_cases()] + [
        dict(name=name, hex=wire.hex(), accept=False, reason=reason, frame=None)
        for name, wire, reason in hostile_cases()]


def main():
    # Always write alongside this script, independent of invocation directory.
    Path(__file__).with_name('vectors.json').write_text(
        json.dumps(vectors(), indent=2, ensure_ascii=True) + '\n', encoding='utf-8')


if __name__ == '__main__':
    main()
