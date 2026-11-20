# ADR-0251 — Expressive-media fuzz CPU-hour budget and decoded-pixel and frame budgets

- **Status:** Accepted by delegation (2026-11-20), additive. The operator delegated the figures that the `expressive_media` build plan left open as Q1 and Q3. This ADR never amends audited text: it adds numbers where the design names a budget and gives no figure, and it never lowers a ceiling the design already states.
- **Class:** Engineering parameters for scope `expressive_media` (ADR-0250). No wire change, no new primitive.
- **Answers:** build-plan questions Q1 (fuzz CPU-hour budget) and Q3 (decoded-pixel and frame budgets).
- **Leaves open:** Q4 and Q5 (see "Not answered").

## Rulings

### 1. Fuzz budget (Q1)

The "agreed CPU-hour budget" of V-012 pre-engineering criterion 1 is:

- **24 CPU-hours per target**, per release candidate, on a current commit of the parser. Targets are the parsers OM itself ships (not the platform decoders): (a) the WebP RIFF header and chunk walker with the profile and budget validator, (b) the `ExpressiveAssetRef` and expressive attachment-ref decoder, (c) the `ContentPack` manifest and `ContentPackRelease` parser, (d) the `ContentPackControlState` parser and lineage walker. At least four targets, so **at least 96 CPU-hours**. A new target adds 24.
- **Two tools.** A coverage-guided fuzzer in the engineer's normal toolchain (libFuzzer through `cargo-fuzz`) and a second, independently implemented fuzzer (AFL++) over the same targets, each running the full per-target budget. This is the "independent tool" the gate's independence field asks for.
- **Corpus.** Seeded from the positive and negative wire vectors, the image-bomb corpus of build-plan slice 1 (every over-ceiling case, one over and exactly at), and truncated, overlapping and reordered chunk variants; the corpus is committed under `verification/`.
- **"No unresolved crash"** means no crash, hang over 1 second per input, out-of-memory or sanitizer finding is open; each finding is fixed and its input kept as a regression vector.
- Platform and browser decoders are not fuzzed by OM; they are reached only after the header validator and the budgets below pass, behind a worker with the memory ceiling and time limit below.
- The budget is for slice 1 close and for every release candidate that offers expressive media. Continuous fuzzing of shipped parsers (pre-release criterion) is unchanged. A lower budget needs a new ADR; a higher one may be set locally at any time.

### 2. Decoded-pixel and frame budgets (Q3)

Budget identifier `om.expressive.budget-v1`, versioned with the profile and enforced from the header before any decode (reject on the first violation; no partial render):

| Profile | Max canvas | Max frames | Max decoded pixels per frame | Max total decoded pixels |
|---|---|---|---|---|
| `sticker.v1` | 512 x 512 | 90 | 262,144 | 23,592,960 |
| `custom-emoji.v1` | 256 x 256 | 90 | 65,536 | 5,898,240 |
| `loop-animation.v1` | 1024 x 1024 | 300 | 1,048,576 | 314,572,800 |

- **Derivation, not invention.** The frame cap is the design ceiling (30 fps x 3 s, 30 fps x 3 s, 30 fps x 10 s); pixels per frame is the canvas ceiling; the total is their product. The budget therefore never rejects an asset the design profiles allow, and rejects every asset that the encoded-size ceilings alone would admit but that is larger in decoded terms (an image bomb).
- Every frame rectangle must lie inside the canvas, and the sum of declared frame areas must not exceed the total. A frame count, canvas size or duration the header cannot state, or a value that overflows 32 bits in the multiplication, is rejected.
- **Resident decode ceiling: 64 MiB** of decoded pixel memory per asset (RGBA, including the compositing canvas and the previous frame), measured in the decode worker; decoding is streamed frame by frame and never holds all frames.
- **Decode time limit: 5 seconds** of worker time per asset; on expiry the worker is terminated and the asset shows its inert placeholder.
- A budget change that rejects more is a new `om.expressive.budget-v2`; one that admits more needs an ADR, because it changes fallback validity (C-44).

## Not answered (left open)

- **Q4**, how a device advertises inline type 4, `ReactionSet` key 3 and each media profile. The constitution digest gives only constraints (C-23 negotiation authenticated and bound into the transcript, C-41 and C-43 public immutable registry IDs, C-44 a new ID for anything that changes fallback validity); it does not name a capability ID or the carriage. Open for slice 0.
- **Q5**, the signed bytes and domain-separation label for `ContentPackRelease` and `ContentPackControlState`, and the controller and threshold encoding. The design says only "OM's durable-signature profile"; the constitution gives no label (C-83 and R-108 only say what a signature must not claim). Open for slice 0.
- Q2 and Q6 to Q11 are untouched.

## Consequences

- Slices 0, 1 and 7 can say done against these figures. Release evidence in ADR-0250 point 4 uses the same budget.
- Nothing here is design-bound text; where an audited document later states a different figure, that document wins and this ADR is superseded by an ADR that says so.
