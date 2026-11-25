# SPDX-License-Identifier: MPL-2.0
# Independent second-language codec for LanFrameV1 (spec/cddl/lan.cddl); written from the CDDL alone, reviewed and tested here.
"""Tests of the independent codec, including hand-crafted hostile CBOR."""
import json
import re
import unittest
from pathlib import Path

from lanframe import decode, encode
from make_vectors import MAX, valid_cases, hostile_cases, vectors


class CodecTests(unittest.TestCase):
    def assert_reason(self, reason, function, value):
        with self.assertRaises(ValueError) as caught:
            function(value)
        self.assertEqual(str(caught.exception), reason)
        self.assertIsNotNone(re.fullmatch(r'[a-z]+(?:_[a-z]+)*', reason))

    def test_positive_wire_fixtures_and_round_trips(self):
        counts = {kind: 0 for kind in range(1, 9)}
        for name, wire, frame in valid_cases():
            with self.subTest(name=name):
                counts[frame[1]] += 1
                self.assertEqual(encode(frame), wire)
                decoded = decode(wire)
                self.assertEqual(decoded, frame)
                self.assertEqual(encode(decoded), wire)
                # Canonical output must not depend on insertion order.
                self.assertEqual(encode(dict(reversed(tuple(frame.items())))), wire)
                self.assertEqual(list(decoded), sorted(frame))
        self.assertTrue(all(count >= 2 for count in counts.values()))

    def test_hostile_wire_fixtures(self):
        cases = hostile_cases()
        self.assertGreaterEqual(len(cases), 80)
        for name, wire, reason in cases:
            with self.subTest(name=name):
                self.assert_reason(reason, decode, wire)

    def test_vector_file_is_exact_and_executable(self):
        saved = json.loads(Path(__file__).with_name('vectors.json').read_text(encoding='utf-8'))
        self.assertEqual(saved, vectors())
        self.assertEqual(len(saved), len({v['name'] for v in saved}))
        for v in saved:
            with self.subTest(name=v['name']):
                self.assertEqual(set(v), {'name', 'hex', 'accept', 'reason', 'frame'})
                wire = bytes.fromhex(v['hex'])
                self.assertEqual(v['hex'], wire.hex())
                if v['accept']:
                    self.assertIsNone(v['reason'])
                    frame = {int(k): bytes.fromhex(x) if isinstance(x, str) else x
                             for k, x in v['frame'].items()}
                    self.assertEqual(decode(wire), frame)
                    self.assertEqual(encode(frame), wire)
                else:
                    self.assertIsNone(v['frame'])
                    self.assert_reason(v['reason'], decode, wire)

    def test_encode_rejects_schema_violations(self):
        examples = {frame[1]: frame for _, _, frame in valid_cases()[:16:2]}
        for kind, frame in examples.items():
            for key in frame:
                with self.subTest(kind=kind, missing=key):
                    self.assert_reason('key_set', encode,
                                       {k: v for k, v in frame.items() if k != key})
            with self.subTest(kind=kind, extra=True):
                self.assert_reason('key_set', encode, {**frame, 23: 0})
            for key, value in frame.items():
                with self.subTest(kind=kind, wrong_type=key):
                    self.assert_reason('value_type', encode,
                                       {**frame, key: b'x' if type(value) is int else 0})
                if key not in (0, 1) and type(value) is int:
                    for bad in (-1, MAX + 1):
                        with self.subTest(kind=kind, key=key, bad=bad):
                            self.assert_reason('integer_range', encode, {**frame, key: bad})
        for kind, key, bad_values in (
            (1, 3, (0, 9)), (5, 9, (0, 65536)), (7, 13, (0, 65536)),
        ):
            for value in bad_values:
                with self.subTest(kind=kind, key=key, value=value):
                    self.assert_reason('integer_range', encode, {**examples[kind], key: value})
        for kind, key, sizes in (
            (3, 2, (0, 15, 17)), (1, 5, (31, 33)), (2, 7, (0, 16385)),
            (5, 8, (15, 17)), (5, 10, (15, 65)), (6, 10, (15, 65)),
            (6, 11, (31, 33)), (6, 12, (31, 33)), (8, 15, (31, 33)),
        ):
            for size in sizes:
                with self.subTest(kind=kind, key=key, size=size):
                    self.assert_reason('byte_string_size', encode,
                                       {**examples[kind], key: b'x' * size})
        for version in (0, 2, MAX + 1):
            self.assert_reason('invalid_version', encode, {**examples[3], 0: version})
        for kind in (0, 9, MAX + 1):
            self.assert_reason('unknown_kind', encode, {**examples[3], 1: kind})

    def test_python_type_policy(self):
        frame = valid_cases()[0][2]
        end = valid_cases()[4][2]
        for value in (True, 1.0, '1', None, [], {}, bytearray(b'x'), memoryview(b'x')):
            with self.subTest(value=repr(value)):
                self.assert_reason('value_type', encode, {**frame, 3: value})
        for value in (bytearray(end[2]), memoryview(end[2]), 'x' * 16):
            self.assert_reason('value_type', encode, {**end, 2: value})
        for key in (True, -1, MAX + 1, '0', b'0', 0.0):
            with self.subTest(key=repr(key)):
                self.assert_reason('invalid_key', encode, {key: 1, 1: 3, 2: end[2]})
        for value in ([], None, b'', 3):
            self.assert_reason('not_map', encode, value)
        for value in (bytearray(b'\xa0'), memoryview(b'\xa0'), 'a0', None):
            self.assert_reason('input_type', decode, value)

    def test_every_proper_prefix_of_small_valid_frames(self):
        for name, wire, _ in valid_cases():
            if len(wire) > 300:
                continue
            for length in range(len(wire)):
                with self.subTest(name=name, length=length):
                    self.assert_reason('empty_input' if length == 0 else 'truncated',
                                       decode, wire[:length])


if __name__ == '__main__':
    unittest.main()
