# V-005 encoding prototype

**Feasibility prototype. Not product code. Not for production.** It exists to
produce evidence for gate V-005 (canonical encoding of W0 signed/hashed
objects) and codec-cost numbers for V-022. Evidence is not gate passage. The W0 numeric/CDDL/domain registries are frozen by ADR-0119 and amended/re-frozen by ADR-0140, ADR-0141 and ADR-0142; runtime agreement must still be re-established after a frozen wire change.

## What it checks

| Input | Location |
|---|---|
| CDDL for the 23 W0 objects (including ADR-0126 first-contact and ADR-0142 sealed-delivery objects) | `spec/cddl/*.cddl` |
| Registries (numeric fields, object types, domain separation, crypto profiles, time fields, CBOR profile) | `spec/registries/*.json` |
| 735 object vectors (71 positive, 664 negative) | `verification/vectors/v005-w0-object-vectors.json.gz` |

The runner compares regenerated canonical JSON with the decompressed committed
corpus. It does not compare gzip container bytes, which can differ between
gzip implementations even when the vector contents are identical.

1. **Two codecs agree.** The Rust strict codec (`rust/`) and the Node strict
   harness ([`verification/cbor/node_strict_harness.mjs`](../../cbor/node_strict_harness.mjs)) decode, validate and
   re-encode every vector. For each one, `gen/compare.py` checks the verdict,
   the exact error class, byte-identical re-encoding and the derived
   signature/hash inputs. Rust also re-encodes the 14 existing
   [`verification/cbor/fixtures.json`](../../cbor/fixtures.json) fixtures.
2. **Two CDDL tools agree.** `gen/cddl_check.py` validates each vector with
   the Rust `cddl` crate 0.10.7 and the Ruby `cddl` gem 0.12.14. It also feeds
   the Ruby tool's generated examples to the Rust codec in lenient mode.
3. **Codec cost (V-022).** `v005 bench` times decode, decode+validate, encode
   and signature/hash-input derivation for one representative object per
   type, natively and as wasm32-wasip1 under wasmtime.

Positive vector bytes come from the existing, unmodified
[`verification/cbor/python_canonical.py`](../../cbor/python_canonical.py) encoder. The generator only
hand-emits bytes for deliberately non-canonical negatives.

## Run

```sh
./run.sh               # everything, writes verification/results/v005-encoding-latest.json
./run.sh --no-bench    # skip V-022 timings
./run.sh --update-vectors   # accept regenerated vectors (normally they must match the .gz)
```

Requirements: python3, node, rustup (the toolchain pinned in
`rust/rust-toolchain.toml`, with target `wasm32-wasip1`), wasmtime and ruby.
Install the two CDDL tools into the git-ignored `.tools/` directory:

```sh
cargo install cddl --version 0.10.7 --locked --root .tools
GEM_HOME=.tools/gems gem install json_pure -v 2.6.3   # only on Ruby < 2.7 (macOS system ruby)
GEM_HOME=.tools/gems gem install cddl -v 0.12.14
```

## Known tool quirks (handled in the scripts)

- The Rust `cddl` 0.10.7 CLI exits 0 even when validation fails, so the
  verdict is read from its log.
- The same tool never matches a `KEY => VALUE` map member whose key is a
  named rule or has a control operator (`ext-id`, `tstr .size (1..128)`).
  Only the Rust tool validates against a copy with the `extensions` key
  inlined as `tstr`. The 1..128 bound is still checked by the Ruby tool and
  by both codecs.
- On macOS, rustup's `rust-lld` for 1.98.1 cannot find `libLLVM.dylib`.
  `run.sh` sets `DYLD_FALLBACK_LIBRARY_PATH` for the wasm link.

## 2026-09-25 additions (ADR-0131/0135)

- 677 vectors. New families: the MessageRequest actually sealed with the pinned draft-ietf-hpke-pq-05 suite (taken
  from the V-001 vectors); profile-pinned size negatives (the enc and public-key lengths come from
  [`om-crypto-profiles.json`](../../../spec/registries/om-crypto-profiles.json) `message_request_hpke`, enforced by both codecs; CDDL cannot express them); RequestKey
  negatives inside ContactDescriptor key 15 and IntroductionTicket key 11; `om.acs.fork_ack` negatives; and R-237
  attachment pairs (a W1 field inside the body is rejected, the same field in a non-critical extension is accepted).
- Both codecs now validate a **registered** extension's value against its registered struct and criticality. Before
  this, a malformed `om.acs.fork_ack` was accepted by both.
- R-280: 16 reciprocal IntroIdentityBinding sequences (`intro_binding_sequences`) are checked by
  `gen/intro_binding.py` and the independently written [`node_intro_binding.mjs`](../../cbor/node_intro_binding.mjs).


## 2026-09-27 ADR-0140 additions (Run 1)

- AsyncKeyBundle v1 no longer carries keys 3/4 (`acs_generation`, `acs_state_hash`); both are reserved/retired and explicit negative vectors reject them.
- one-time-prekey derived signature input binds `subject_device_id`, CryptoProfile, bundle generation, `provider_pool_id`, kind, key id and public key.
- Run 1 produced 679 vectors (68 positive, 611 negative). The Python generator and Node strict harness agreed on that regenerated corpus.
- this runner has no Rust toolchain, so the Rust strict-codec/CDDL-tool half of the V-005 rerun was **not** executed. V-005 therefore remained open at pre-engineering pending a tool-complete rerun.

## 2026-09-27 ADR-0141 additions (Run 2)

- `om.acs.legacy_policy` is now a registered **critical** extension; a positive vector carries the critical bit and a negative vector rejects the same extension marked non-critical.
- the canonical corpus is now **681 vectors (69 positive, 612 negative)**. The Python generator and Node strict harness agree on all 681 expected verdict/error classes in this checkpoint.
- the Rust strict-codec/CDDL-tool half still could not be executed because this runner has no Rust toolchain. The two-codec criterion therefore remains intentionally open rather than inheriting stale passage.

## Limits of this evidence

- One agent wrote both codecs, the registries and the generator in a single
  session. Their agreement shows internal consistency, not independent review
  (historical limitation of this run; ADR-0117 was later accepted and governs independence).
- This result predates the later closure of A-023..A-028 and the OM-Crypto-v1 pinning in ADR-0131/ADR-0135; it must not be read as current protocol status.
  The vectors in this historical prototype use the insecure test crypto profile (65535).
- Timings come from one machine (see the results JSON for host and
  toolchain). They show order of magnitude, not a budget.

### 2026-09-27 Run 3 / ADR-0142 sealed delivery

- Added `DeliveryKeyMaterial`, signed `SealedDeliveryKeyRecord` (type 22), unsigned `SealedDeliveryEnvelope` (type 23), and required `MessageRequestPayload.reply_delivery_key`.
- OM-Crypto-v1 exact sealed-HPKE public-key/encapsulation sizes and the five ciphertext bucket+tag lengths are enforced by the Python-generated corpus and Node strict codec; the Rust strict codec source has the same registry-driven checks ready for a Rust-capable rerun.
- The canonical corpus is now **735 vectors (71 positive, 664 negative)**. Python generation and Node strict decoding agree on **all 735** expected verdict/error classes.
- Rust/CDDL execution remains unavailable in this environment, so the two-codec/tool criterion is deliberately **not** marked complete.

### 2026-09-27 Rust/CDDL verification

- The regenerated 735-vector corpus (71 positive, 664 negative) matched the
  committed corpus byte-for-byte after decompression: 1,204,364 bytes,
  SHA-256 `d5de1deaca350942b286a55d09de2748c234b37c26f77cad315d6c72bb46644a`.
- Rust 1.98.1 and Node v24.14.0 each processed all 735 vectors with zero
  mismatches; Rust also passed all 14 existing fixtures. Python and Node each
  passed all 16 R-280 sequences with zero mismatches (5 reached ACTIVE).
- Rust `cddl` 0.10.7 and Ruby `cddl` 0.12.14 agreed on all 399 applicable
  vector checks. Ruby generated 276 examples across the 23 object types;
  the Rust strict codec reported zero unexpected rejections.
- The complete `./run.sh` run, including native and wasm32-wasip1 benchmarks,
  passed. The result records Rust, Cargo, Node, Ruby, Python, CDDL versions,
  vector counts and the canonical corpus hash.
- The runner's corpus comparison now decompresses the committed gzip before
  comparing bytes. No test vectors or protocol rules changed. The V-005
  pre-engineering criteria are satisfied; overall V-005 remains open for its
  pre-release criterion.
