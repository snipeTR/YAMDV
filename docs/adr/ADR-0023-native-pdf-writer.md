# ADR-0023: Native PDF writer

- Status: accepted (Phase V2-H, prompt 018 §2.1; FUT-39, R-EXP-01, S-EXP-03, T10)

Context: v1 exported PDF through the GDI band path on "Microsoft Print to PDF": raster text, no bookmarks, no links,
and a dependency on an optional Windows printer. R-EXP-01 asks for vector, selectable text, an outline and links.

Decision — crates (checked with `cargo deny check licenses`, all inside the S-SUP-04 allowlist):
- `pdf-writer` 0.15 (MIT OR Apache-2.0; deps `bitflags`, `itoa`, `memchr`, `ryu`): object/stream writer only.
- `subsetter` 0.2 with `default-features = false` (MIT OR Apache-2.0; dep `rustc-hash`): TrueType/CFF subsetting; the
  `variable-fonts` feature (skrifa, write-fonts, kurbo) stays off for size.
- Deflate: no deflate encoder was in the product graph (encoding_rs, chardetng, tiny-skia without `png-format`), so
  `miniz_oxide` 0.9 (MIT OR Zlib OR Apache-2.0, already in Cargo.lock through the xtask PNG tooling, `flate2`) compresses the
  content, font and image streams (FlateDecode, level 6).

Decision — drawing (backend seam): the pagination engine (`yamdv_export::paginate`) is reused unchanged. Each page is
drawn by the same `draw_doc` code into a `Band` in **record mode**: instead of rasterising, every primitive (fill,
stroke, polyline, Bézier path, circle, image, clip push/pop, glyph run, decoration) is appended to a page command list
in page DIPs. The band renderer (print, screen) and the PDF writer are the two consumers of that list. Link rectangles
come from the laid-out blocks' hot rectangles (`HotKind::Link`, `FootRef`, `FootBack`), headings from the index.

Decision — fonts: glyph runs keep their `IDWriteFontFace`. The font bytes come from DirectWrite
(`IDWriteFontFace::GetFiles` → `IDWriteFontFileLoader::CreateStreamFromKey` → `ReadFileFragment`; the TTC face index
from `GetIndex`), are subset with `subsetter` and embedded as **Type0** with a CIDFontType2 (`FontFile2`) or
CIDFontType0 (`FontFile3`/`OpenType`) descendant, `Identity-H`, the subset's glyph ids as CIDs, a `W` widths array
from the design metrics and a **ToUnicode** CMap built from the run's cluster map, so text is selectable and
searchable. A face whose OS/2 `fsType` forbids embedding (restricted 0x0002 without preview/editable bits, or
bitmap-only 0x0200), a variable-font instance off its default location, or a failed subset is drawn as filled
outlines (`GetGlyphRunOutline`). COLR emoji (`TranslateColorGlyphRun`) become one filled outline path per colour layer.

Decision — images: a JPEG source is passed through as `DCTDecode` (its bytes, not re-encoded); everything else is
written as deflated 8-bit RGB with an 8-bit deflated `SMask` when it has alpha (premultiplied pixels are
un-premultiplied first).

Decision — security (S-EXP-03): only `GoTo` (internal: headings, anchors, footnotes, TOC) and `URI` actions for
`http`, `https` and `mailto`; no JavaScript, Launch, embedded files, forms or XFA. Metadata: Title, Producer
`YAMDV <version>`, CreationDate; nothing personal (S-PRIV-06).

Decision — process: the writer runs in the existing worker process (ADR-0018, R-EXP-07) with the progress card,
Cancel and atomic write (temp file + `MoveFileEx` replace; Cancel deletes the temp file), so the main process memory
stays flat. Print… keeps its GDI band path. Pages are recorded and written one at a time; only the font glyph sets
and the page object ids live across pages.

Consequences: Export PDF no longer needs "Microsoft Print to PDF". Size cost is paid by the §2.2 size room. T10 and
the transient-memory bound are measured in 018a.
