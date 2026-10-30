# ADR-0025: DIB vs DDB for the paint path

- Status: accepted (Phase V2-J, prompt 020 §3.7 / FUT-42)

Context: the window paints through one reused band DIB (`IDWriteBitmapRenderTarget`, 128 rows × window width × 4 B; DirectWrite GDI interop draws the glyphs, tiny-skia draws shapes and images on the same bits, each band is `BitBlt`-ed). Decoded images live as premultiplied RGBA in the Rust heap (image LRU). The alternative is a device-dependent bitmap (DDB, `CreateCompatibleBitmap`) per image or per band.

Measurements (CI run 41039881133, `alloc-sites` + memprobe, typical.md with 2 images): the image pixels are 2 × 691,200 B = 1,382,400 B of the 1,508,246 B tracked Rust heap; the band DIB is ≈ 0.6 MiB at 1280 px; M1 private WS median 5.35 MiB (bimodal 4.6 / 5.4, see F-41); warm scroll T4 9.0–11.1 ms p99, large 5.3 ms, mermaid 8.0 ms (all ≤ 16 ms).

Decision: keep the DIB band. A DDB cannot be a target for tiny-skia or for the DirectWrite bitmap render target (both need CPU-addressable bits), so a DDB path would add a second copy of every image (RGBA for drawing + DDB for blitting) — more memory, not less — and the band would still need a DIB. The DDB variant was not built; its cost follows from the structure above, not from a measurement (reported as such in the V2-J report).

Consequences: no change in code; image memory stays bounded by the image LRU; F-41 (the extra image-sized region in high M1 runs) is a heap-scratch issue, not a bitmap-kind issue.
