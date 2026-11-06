# object/function.rs — functions (shadings, transfer, tint), against PDF 32000-1:2008 §7.10

Reviewed: 2026-11-06.

## Fixed on this branch

- ~~**`/BitsPerSample` ignored for type 0 functions**~~ — **fixed**. The interpolation indexed the
  raw stream bytes, so it treated every sample as 8 bits: 1-, 2-, 4-, 12- and 16-bit sample arrays
  were read as garbage (and `Size` larger than `Length` bytes silently ran past the grid into
  neighbouring samples). `SampledFunction` now decodes the bit stream once, at construction, into
  `f32` samples mapped through `/Decode` into `/Range` (7.10.2's `Interpolate` + clip).
  Test: `sampled_function_reads_the_configured_bits_per_sample`.
- ~~**`/Encode` used as a scale, not as an interpolation**~~ — **fixed**. The old code took
  `Encode1` as the multiplier and ignored `Encode0` and the domain width, which is only correct
  for a domain of width 1: `/Domain [-1 1] /Encode [0 20]` was stretched 20× from 0 instead of
  10× from 10. The scale is now `(Encode1 - Encode0) / (Domain1 - Domain0)` with the matching
  offset; an inverted `/Domain` (forbidden by 7.10.2) is normalised so the function stays
  monotone. Test: `sampled_function_encodes_over_the_whole_domain`.
- ~~**`/Order 3` (cubic spline) refused**~~ — **fixed**. The error dropped the whole shading.
  Cubic spline interpolation is now implemented as a Catmull–Rom spline on the sample grid
  (uniform, passes through every sample, reduces to linear where a neighbour gap closes), applied
  per input dimension; 7.10.2 leaves the spline itself unspecified. Per 7.10.2, `/Order 3` is
  ignored (linear is used) when any `/Size` entry is below 4. Tests:
  `sampled_function_with_cubic_spline`,
  `sampled_function_ignores_cubic_when_a_size_is_smaller_than_four`.
- ~~**No dimension-generic interpolation**~~ — **fixed**. The old `apply` had a hand-written body
  per input dimension (1, 2, 3) and `/Order` outside 1..3 returned `Err("Order n")`. Interpolation
  is now one separable loop over dimensions and works for any number of inputs; `apply` only
  errors on a dimension mismatch.
- ~~**Short sample stream accepted**~~ — **fixed**. 7.10.2: "the stream data shall be long enough
  to contain the entire sample array". It is checked before decoding, so a truncated stream is a
  parse error instead of silently shifted samples. The decoded array is capped at 4 Mi values
  (16 MiB), because 1-bit samples grow 8× when decoded. Tests:
  `sampled_function_rejects_a_short_stream`,
  `sampled_function_rejects_unsupported_bits_per_sample`.
- Writing a `Function` back out still bails: the evaluated form does not keep `/Domain`,
  `/Range`, `/Encode`, `/Decode` and the raw samples, so a round trip is not possible without
  keeping the original stream.

## Checked, no mismatch

- Type 2 (exponential) and type 3 (stitching) functions: `/C0`, `/C1`, `/N`, `/Bounds` (k−1
  entries), `/Encode` (2k numbers), subfunction output dimensionality.
- Type 4 `PostScript` calculator functions: the operator set parsed by `PsFunc`.
