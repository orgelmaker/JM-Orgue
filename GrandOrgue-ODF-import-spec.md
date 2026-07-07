# GrandOrgue Organ Definition File (ODF) — Import Specification

**Purpose.** This document specifies, at implementation level, how [GrandOrgue](https://github.com/GrandOrgue/grandorgue) parses its organ definition files (`.organ`) and loads an organ, so that a compatible importer can be built in other software without access to the GrandOrgue source code. It was produced by analyzing the GrandOrgue source tree at commit `7ea81c2` (July 2026). Source file references (e.g. `GOConfigReader.cpp`) point into that tree and are informational only.

**Scope.** Everything needed to (a) parse a `.organ` file byte-for-byte the way GrandOrgue does, (b) build the playable organ model (manuals, stops, ranks, pipes, couplers, switches, tremulants, windchests, enclosures, combinations), (c) load and interpret the audio samples (WAV/WavPack, attacks, releases, loops, pitch, amplitude), and (d) optionally render the visual console (panels). User-specific settings files (`.cmb`) are described only as far as needed to ignore them safely.

## Document map

| Part | Contents |
|---|---|
| **Part 1** | Low-level file format: encoding, gzip, INI dialect, typed value parsing (booleans, ints, floats, enums, colours, sizes), `.orgue` package archives |
| **Part 2** | Top-level load flow, path resolution, the complete `[Organ]` section, windchests, reversible pistons, divisional couplers, generals, organ identity, `.cmb` files |
| **Part 3** | The playable model: manuals, stops, ranks, pipes (`DUMMY`/`REF:`/sample), drawstop switch logic, couplers, tremulants, enclosures, divisionals, pipe-config inheritance with formulas |
| **Part 4** | Sample loading: WAV/WavPack parsing, `smpl`/`cue` chunks, attack/release assembly, loops, crossfades, pitch and amplitude math |
| **Part 5** | Visual console: panels (old & new format), display metrics, layout engine pseudocode, buttons/labels/enclosures/manuals/images, built-in bitmap inventory |
| **Part 6** | Appendix: importer implementation checklist, known quirks, minimal example ODF |

## Quick orientation: what a GrandOrgue sample set looks like

A sample set is either a **directory** containing one or more `.organ` files (INI-style text) plus sample/image files in subdirectories, or a **`.orgue` package** (a restricted ZIP archive, see Part 1 §7). The `.organ` file is the single entry point: it declares the whole organ and references every sample and image by relative path (backslash-separated).

## Glossary (GrandOrgue object model)

- **Manual** — a keyboard (the pedalboard is manual 000). Has *logical keys* (the internal key range stops operate on) and *accessible keys* (the playable window mapped to MIDI notes).
- **Stop** — a register drawn by the player; maps a range of a manual's logical keys onto pipes, either via references to shared **ranks** or via an *internal rank* defined inline in the stop section.
- **Rank** — a row of **pipes** (one per note). Each pipe is a sample file, a `REF:` to another pipe, or `DUMMY` (silent).
- **Pipe** — one playable note of a rank: one main attack sample plus optional extra attacks (velocity/history-dependent) and releases (key-press-time-dependent), with loops and crossfades.
- **Windchest group** — the acoustic group a rank sits on; links pipes to **enclosures** (volume pedals) and **tremulants**.
- **Enclosure** — a swell box / expression pedal attenuating all windchests that reference it.
- **Tremulant** — either synthesized amplitude modulation (`Synth`) or a switch between alternative "tremmed" sample sets (`Wave`).
- **Coupler** — plays another manual's stops from this manual, optionally octave-shifted, including unison-off, bass and melody couplers.
- **Switch** — a named boolean control; drawstops can compute their state from switches via logic functions (`And`/`Or`/`Not`/…).
- **Divisional / General** — combination pistons recalling a registration for one manual / the whole organ.
- **Panel** — a rendered console page: background woods, drawstop jambs, keyboards, pistons, labels, images.

## Conventions used throughout

- Sections and numbered keys use 1-based, zero-padded 3-digit numbering: `[Stop001]`, `Pipe012`, `Attack001` (`%03d`). The only 0-based section is `[Manual000]` (the pedal, present only when `HasPedals=Y`) and `[Panel000]` (the new-format main panel).
- "required" = missing key aborts the organ load; "default X" = key optional.
- `bool` values are `Y`/`N`. Empty values are treated as *missing* for all non-string reads.
- All counts (`NumberOfXxx`) strictly control which numbered sections/keys are read; extra sections are ignored with warnings.
- An importer that only wants the *instrument* (not the user state) can ignore everything marked CMBSetting/`.cmb`.


---

# Part 1 — Low-Level File Format and Parsing

## GrandOrgue ODF Low-Level File Format and Parsing Specification

This section specifies the byte-level and token-level rules GrandOrgue uses to read `.organ` files (ODF — Organ Definition File), `.cmb` files (combination/settings files), `organindex.ini` (package index), and `.orgue` organ packages. It is derived from `src/core/config/GOConfigFileReader.cpp`, `GOConfigReader.cpp/.h`, `GOConfigReaderDB.cpp/.h`, `GOConfigEnum.cpp/.h`, `src/core/GOCompress.cpp`, `src/core/GOLogicalColour.cpp`, `src/core/go_limits.h`, `src/core/archive/*`, and `src/grandorgue/GOOrganController.cpp` of the GrandOrgue source tree.

All four file kinds (`.organ`, `.cmb`, `organindex.ini`, and GO's own config) use the **same INI-like container format** parsed by `GOConfigFileReader`; they differ only in which typed values are read from them.

---

## 1. Physical file reading (GOConfigFileReader)

### 1.1 Loading and optional gzip compression

1. The whole file is read into memory in one pass (`GOConfigFileReader::Read`). There is no streaming; an out-of-memory condition aborts with an error.
2. A SHA-1 hash of the **raw on-disk bytes** (before any decompression) is computed; its 40-character **uppercase** hexadecimal form is the "ODF hash" used to match `.cmb` files against ODFs (`GOHash::getStringHash`, `GOHash.cpp`).
3. If the buffer begins with the gzip signature `0x1F 0x8B` (big-endian 16-bit `0x1f8b`, and the buffer is at least 18 bytes = gzip header + trailer), the entire buffer is **gzip-decompressed** and the decompressed data is parsed instead (`isBufferCompressed` / `uncompressBuffer`, `GOCompress.cpp`, `GOGZipFormat.h`). So a `.organ` or `.cmb` file may be a gzip stream; this is transparent. Decompression failure aborts the load. (When GrandOrgue builds organ packages it always gzips `.organ` files it stores — `GOArchiveCreator::compressData`.)

### 1.2 Character encoding detection

- If the (possibly decompressed) content starts with the UTF-8 BOM `EF BB BF`, the BOM is stripped and the rest is decoded as **UTF-8**. Invalid UTF-8 makes the *entire* decode result empty; if the file was non-empty but decoded to an empty string, loading fails with "Failed to decode file".
- Otherwise the content is decoded as **ISO-8859-1** (Latin-1): every byte `0x00–0xFF` maps 1:1 to Unicode `U+0000–U+00FF`. This decode can never fail.
- There is **no UTF-16 support and no UTF-16 BOM detection**; no other legacy codepage is tried. (Note: this is ISO-8859-1, *not* Windows-1252 — bytes `0x80–0x9F` become C1 control characters, not typographic characters.)

An empty file parses successfully to zero sections.

### 1.3 Line splitting

- Lines are split on `\n` (LF). If a line then ends with `\r` (CR), that single CR is stripped — so both LF and CRLF endings work. A file with only CR line endings (classic Mac) is *not* split and is effectively one line.
- The final line does not need a terminator.
- There is **no maximum line length**, no maximum number of sections, and no maximum number of keys (the whole file is held in memory).

### 1.4 Comments

- On every line, everything from the **first `;` (semicolon)** to the end of the line is removed, **regardless of position** — a semicolon cannot appear inside a section name, key, or value. There is no escape mechanism.
- After stripping a comment, the remainder of the line is **right-trimmed** (trailing whitespace removed). If no `;` was present, the line is **not** trimmed at this stage (trailing whitespace remains part of the value).
- A line that is empty, or becomes empty after comment stripping, is skipped.
- Only `;` starts a comment; `#`, `//` etc. have no special meaning.

### 1.5 Section headers

A line whose **first character is `[`** and whose length is > 1 is treated as a section (group) header:

- The **last character** of the line must be `]`. If it is not, the line is right-trimmed and re-tested:
  - If it now ends with `]`, an error "Invalid section start" is logged but the section is **still accepted** (i.e. whitespace after `]` is tolerated with a logged error).
  - If it still does not end with `]`, an error is logged and the line is skipped.
- The group name is everything strictly between the first `[` and the final `]`, taken **verbatim** (no trimming, no case folding). It may contain spaces, `[`, or even `]` (only the final character is checked); it may be empty (`[]` yields the empty group name).
- Leading whitespace before `[` makes the line *not* a section header (it will be parsed — and fail — as a key=value line). Section headers must start in column 1.
- A **duplicate section name** logs a warning ("Duplicate group") and the sections are **merged**: subsequent keys are added to the already-existing group map.

### 1.6 Key=value lines

Any other non-empty line must be a key=value entry:

- Entries appearing **before any section header** are errors and are skipped.
- The line is split at the **first `=`**. The `=` must exist and must not be the first character (empty keys are invalid); otherwise the line is logged as "Invalid Config entry" and skipped.
- **Key** = all characters before the first `=`, verbatim — **no whitespace trimming**. `Key = value` produces the key `"Key "` (with trailing space), which will *not* match lookups for `"Key"`. Spaces around `=` are therefore significant and effectively forbidden.
- **Value** = all characters after the first `=` to the end of line (after comment stripping), verbatim — may be empty, may contain further `=` characters, leading/trailing whitespace is preserved at this layer (trailing whitespace only survives if the line had no comment, see 1.4).
- A **duplicate key** within a (merged) section logs a warning; the **last occurrence wins**.
- Keys and section names are stored and compared **case-sensitively** at this layer.

### 1.7 Error handling at this layer

Malformed lines are logged and skipped; parsing continues. `GOConfigFileReader::Read` only returns failure for I/O errors, out-of-memory, gzip decompression failure, or UTF-8 decode failure of the whole file.

---

## 2. Setting database and ODF/CMB layering (GOConfigReaderDB)

After file parsing, all entries are loaded into a `GOConfigReaderDB`, which holds **two independent key-value stores**:

- the **ODF store** (filled from the `.organ` file, loaded with type `ODFSetting`), and
- the **CMB store** (filled from the user's `.cmb` combination/settings file, loaded with type `CMBSetting`).

Keys in both stores are flat strings of the form `Group/Key` (section name + `/` + key name).

### 2.1 GOSettingType: ODFSetting vs CMBSetting

`typedef enum { ODFSetting, CMBSetting } GOSettingType;` (`GOConfigReader.h`). Every typed read names which store it reads from:

- `GetString(ODFSetting, …)` looks **only** in the ODF store (with optional case-insensitive fallback, see 2.3).
- `GetString(CMBSetting, …)` looks **only** in the CMB store, always case-sensitively. **There is no automatic fallback from CMB to ODF** in the database.

Layering is done by **callers**: for a user-overridable value, the loading code first reads the ODF value (`ODFSetting`, giving the designed default) and then reads the same or a related key with `CMBSetting, required=false, default=<odf value>` (see e.g. `GOPipeConfig::LoadFromCmb`, `src/grandorgue/model/pipe-config/GOPipeConfig.cpp`). **An ODF importer that ignores user settings needs only `ODFSetting` reads**; all `CMBSetting` reads may be treated as "absent, use default".

### 2.2 How the stores are filled when an organ loads (GOOrganController::Load)

1. The `.organ` file is parsed and loaded into the ODF store: `ReadData(odf_ini_file, ODFSetting, false)`.
2. A settings file is chosen, in priority order:
   1. an explicitly given file;
   2. the per-user settings file `<OrganSettingsPath>/<organHash>-<preset>.cmb`;
   3. a **bundled** `.cmb` next to the ODF: ODF path with its extension replaced by `.cmb` (also searched *inside* the organ package when packages are used).
3. If a settings file was found, it is parsed with the same parser and loaded into the CMB store (`ReadData(…, CMBSetting, false)`). If its `[Organ] ODFHash` entry is present and differs from the SHA-1 of the ODF, the user is asked whether to discard the CMB data.
4. If **no** settings file exists, the ODF itself is scanned for legacy "GO 0.2 style" embedded settings: `ReadData(odf_ini_file, CMBSetting, true)` with `handle_prefix=true` processes **only sections whose name starts with `_`** (underscore), strips the underscore, and loads their entries into the CMB store. (These `_Section` entries are otherwise reported as "Old GO 0.2 styled setting in ODF".)

### 2.3 Case sensitivity

`GOConfigReaderDB(bool case_sensitive)`:

- The organ loader constructs it with `case_sensitive = StrictODFCheck` (a user option, **default false** — `GOConfig.cpp`).
- When **not** case-sensitive (the default), a second lowercase index of the ODF store is built (full `Group/Key` string lowercased). Lookup order for `ODFSetting`: exact match first; if that fails, lowercase match — a hit there logs a warning "Incorrect case for section '…' entry '…'" and returns the value. So **ODF section and key names are effectively case-insensitive by default (exact case preferred), and case-sensitive when the user enables strict ODF checking.** (Implementation note: the miss-test of the lowercase lookup compares against the wrong map's `end()` iterator — `GOConfigReaderDB.cpp` `GetString`; a conforming reimplementation should simply treat "not found in lowercase index" as not found.)
- CMB lookups are always exact/case-sensitive.

### 2.4 Duplicates and unused-entry reporting

- When entries are inserted into a store (`AddEntry`), an already-present key logs "Duplicate entry: Group/Key" and is overwritten (last wins). With case-insensitive mode, two ODF keys differing only in case also collide in the lowercase index (warning; last one wins there).
- Every entry is tracked as used/unused. After the organ is fully loaded, `ReportUnused` logs a warning for every ODF/CMB entry that was never read ("Unused ODF entry '…'"), capped at 3000 ODF warnings (`UNUSED_REPORT_LIMIT`, `GOConfigReaderDB.cpp`). This is diagnostic only.
- `GOConfigReader::MarkGroupInUse(group)` is called once per consumed object section; a second use of the same section name throws a fatal error "Section %s already in use".

---

## 3. Typed read API (GOConfigReader)

All typed reads funnel through a private `Read(type, group, key, required, isToTrim, value)`:

- **Lookup**: as per section 2 (store + case rules).
- **required exemptions**: even when a read is `required`, it is silently downgraded to optional if the group name starts with `Setter`, or starts with `Panel` and has `Setter` at character offset 8 (i.e. `PanelNNNSetter…`), or starts with `FrameGeneral` (`GOConfigReader::Read`). These are combination-related sections.
- **Trimming** (`isToTrim=true`, used by *all* reads except the plain string reads): the found value is trimmed of leading **and** trailing whitespace. If trimming changed the value and strict mode is on, a warning is logged. If the value is **empty after trimming, it is treated as missing** (falls back to default, or triggers the "required" error).
- **Missing required value**: throws a fatal error (`wxString` exception) "Missing required value section '…' entry '…'" — organ loading aborts.
- **Missing optional value**: the method's default is returned.

`GOConfigReader` is constructed with `strict` (= `StrictODFCheck`, default false — enables extra whitespace/empty warnings only) and `hw1Check` (= `ODFHw1Check`, defaults to `StrictODFCheck` — enables filename portability warnings).

Throughout the following, "fatal" means an exception is thrown and the organ fails to load; "warning"/"error log" means a message is logged and parsing continues.

### 3.1 ReadString

- No trimming at all; the raw stored value (which itself never contains `;` and may lack trailing whitespace due to comment stripping) is returned.
- An **empty value counts as present** (`Key=` satisfies a required ReadString and returns `""`).
- Missing optional → default (`""` if none given).

### 3.2 ReadStringTrim

- Calls ReadString, then: **only if the last character is a space (U+0020)**, all trailing whitespace is removed (with a warning in strict mode). A value ending in a tab is *not* trimmed. Leading whitespace is never removed.

### 3.3 ReadStringNotEmpty

- Same as ReadString, but logs a warning (strict mode only) if the value is empty/whitespace; still returns it.

### 3.4 ReadFileName

- `ReadStringTrim`; additionally, when hw1Check is on, warns if the name contains `/` ("non-portable directory separator"). The **portable directory separator in ODF file references is backslash `\`** (converted to the host separator at open time, `GOLoaderFilename::generateFullPath`). See section 6.

### 3.5 ReadBoolean / ReadBooleanTriple

Value is trimmed; empty ⇒ missing.

- Exactly `Y` or `y` → true. Exactly `N` or `n` → false.
- Otherwise the value is uppercased (ASCII-only, locale-independent) and a warning "Strange boolean value" is logged; if it then **starts with** `Y` → true, starts with `N` → false (so `Yes`, `yes`, `No`, `n0`… are accepted with a warning).
- Anything else (e.g. `1`, `true`) → **fatal error** "Invalid boolean value".
- Missing optional → the caller's default (`false` when none supplied). `ReadBooleanTriple` returns a three-state value (default/false/true) for absent entries.
- Note: GrandOrgue itself writes `Y`/`N`.

### 3.6 ReadInteger / ReadLong

`ReadLong` is an exact alias of `ReadInteger` (both return C `int`). Value is trimmed; empty ⇒ missing ⇒ default.

- Parsed with C++ `std::stol` (base 10): optional leading whitespace, optional `+`/`-` sign, decimal digits. **No hex/octal**, no locale digit grouping, no decimal point.
- Not a number at all (e.g. `abc`, or just `-`) → **fatal error** "Invalid integer value".
- Magnitude exceeding `long` → fatal error.
- **Trailing garbage after the parsed prefix** (e.g. `12abc`, `3.5` → parses `3`): an error is **logged** but the parsed prefix value is **used**.
- **Range check** against the caller-supplied `[nmin, nmax]`:
  - For `ODFSetting`: out of range → **fatal error** "Out of range value".
  - For `CMBSetting`: out of range → error logged, the **default value is substituted**.
- Negative values are accepted wherever `nmin < 0`.

### 3.7 ReadFloat

Value is trimmed; empty ⇒ missing ⇒ default.

- If the value contains a comma, the **first** comma is replaced by `.` with a warning ("locale dependent floating point"). (Only the first one.)
- Parsed with a C++ input stream imbued with the **"C" locale**: optional sign, decimal digits, optional `.` fraction, optional `e`/`E` exponent. The decimal separator is always `.`; grouping separators are not recognized.
- **Parse failure is silent**: the stream does not throw, and the result becomes `0.0` (the catch branch in `GOConfigReader::ReadFloat` is effectively dead code). Trailing garbage after a valid prefix is silently ignored (`1.5abc` → 1.5).
- **Range check** on `[nmin, nmax]`: same policy as integers — fatal for `ODFSetting`, log-and-use-default for `CMBSetting`.

### 3.8 ReadEnum

Value is trimmed; empty ⇒ missing ⇒ default (when no explicit default: the **first entry** of the enum, `GOConfigEnum::GetFirstValue`).

- The trimmed value is matched **exactly (case-sensitively)** against the enum's name list (`GOConfigEnum::GetValue`).
- No match → **fatal error** "Invalid enum value".

### 3.9 ReadColor

Value is trimmed; empty ⇒ missing ⇒ default (default default: `"BLACK"`). The value is then **uppercased (ASCII-only)**, so input is case-insensitive.

Accepted **named colours** (exact strings after uppercasing; note the embedded space in the "DARK …" names) and their RGB values (`GOLogicalColour.cpp`):

| Name | RGB | Name | RGB |
|---|---|---|---|
| BLACK | 00 00 00 | DARK RED | 80 00 00 |
| BLUE | 00 00 FF | MAGENTA | FF 00 FF |
| DARK BLUE | 00 00 80 | DARK MAGENTA | 80 00 80 |
| GREEN | 00 FF 00 | YELLOW | FF FF 00 |
| DARK GREEN | 00 80 00 | DARK YELLOW | 80 80 00 |
| CYAN | 00 FF FF | LIGHT GREY | C0 C0 C0 |
| DARK CYAN | 00 80 80 | DARK GREY | 80 80 80 |
| RED | FF 00 00 | WHITE | FF FF FF |
| BROWN | A5 2A 2A | | |

Otherwise a **`#RRGGBB`** form is accepted: exactly 7 characters, `#` followed by six characters each in `0-9` or `A-F` (lowercase already uppercased). **Beware — GrandOrgue's implementation is buggy** (`parseColor`, `GOConfigReader.cpp`): each two-digit component is computed as `first_digit*10 + second_digit` (decimal weighting of hex digits, digits A–F = 10–15) instead of `first_digit*16 + second_digit`. E.g. `#FF0000` yields red = 165, not 255; `#808080` yields 80,80,80 (decimal). A byte-exact reimplementation must reproduce this; an intentionally *correct* importer should treat it as hex but know GrandOrgue renders it differently.

Anything else → **fatal error** "Invalid color".

### 3.10 ReadSize (panel dimensions)

Value trimmed; empty ⇒ missing ⇒ default (default default: `"SMALL"`); uppercased (ASCII). `size_type` selects the dimension: 0 = horizontal, 1 = vertical.

| Value | size_type 0 (width) | size_type 1 (height) |
|---|---|---|
| SMALL | 800 | 500 |
| MEDIUM | 1007 | 663 |
| MEDIUM LARGE | 1263 | 855 |
| LARGE | 1583 | 1095 |

Otherwise parsed with `std::stol` (base 10, trailing garbage silently ignored). Parse failure → fatal error. The documented range is [100, 32000], but the range check is dead code (`size < 100 && size > 32000` can never be true — `GOConfigReader::ReadSize`), so **any integer is accepted in practice**.

### 3.11 ReadFontSize

Value trimmed; empty ⇒ missing ⇒ default (default default: `"NORMAL"`); uppercased (ASCII).

- `SMALL` → 6, `NORMAL` → 7, `LARGE` → 10 (points).
- Otherwise `std::stol` as above; parse failure → fatal. The [1, 50] range check is likewise dead code (`size < 1 && size > 50`), so any integer passes.

---

## 4. Number formats — summary

- **Integers**: base-10 only, optional `+`/`-`, `std::stol` semantics; locale-independent; trailing garbage tolerated with a logged error (parsed prefix used); non-numeric fatal; range violations fatal for ODF values.
- **Floats**: C-locale decimal notation with optional exponent; `.` decimal point; a single `,` is auto-corrected to `.` with a warning; unparsable values silently become `0.0`; trailing garbage silently ignored; range violations fatal for ODF values.
- No thousands separators, no hex, no infinities/NaN keywords.

---

## 5. Limits (go_limits.h)

The parser itself imposes **no** line-length, section-count, or key-count limits. `src/core/go_limits.h` defines semantic limits used as range bounds by higher layers: `MAX_CPU` 256, `MAX_PRESET` 10, `MAX_POLYPHONY` 100000, `MAX_MIDI_DEVICES` 50000, `MAX_SAMPLE_LENGTH` 158760000 (samples), `MAX_TEMPERAMENTS` 999. Diagnostic cap: at most 3000 "unused ODF entry" warnings (`GOConfigReaderDB.cpp`).

---

## 6. Path resolution for files referenced from the ODF

(`GOLoaderFilename.cpp/.h`, `GOFileStore.cpp`)

- File references (samples, images, the InfoFilename) are read with `ReadFileName` and interpreted relative to a virtual "ODF root": the directory containing the `.organ` file, or the set of open package archives.
- Paths use `\` as the portable separator; at open time every `\` is replaced by the host path separator and prefixed with the base directory (`generateFullPath`). Absolute paths in the ODF are used as-is (non-portable). `.`/`..` components are normalized (`wxPATH_NORM_DOTS`); a warning is issued if the stored spelling differs in case from the on-disk name.
- When packages are in use, the (backslash-separated) path is matched **verbatim, case-sensitively** against archive entry names, searching the main archive first, then its declared dependencies in order (`GOFileStore::FindArchiveContaining`). Failure to find the file is fatal at load time, not parse time.

---

## 7. Organ packages (`.orgue`) — archive layer

(`src/core/archive/GOArchiveReader.cpp`, `GOZipFormat.h`, `GOArchive.cpp`, `GOArchiveManager.cpp`)

### 7.1 Container format

A `.orgue` file is a restricted **ZIP archive**:

- **Compression method must be 0 (stored)** for every entry, and compressed size must equal uncompressed size; anything else is rejected ("Only stored compression supported"). (Space saving comes from gzipping the *contents* of `.organ` files and using WavPack-in-`.wv` for samples, not from ZIP compression.)
- Non-split archives only (single disk). **ZIP64 is supported**; `version needed to extract` must be ≤ 45.
- General-purpose flag bits: only bits 1, 2, 4 and 11 may be set (mask `0b1111011111101001` must be zero). Bit 11 = UTF-8 filenames.
- The end-of-central-directory record is located by scanning backwards up to 65535+22 bytes from EOF, requiring the comment length field to match the actual tail length; ZIP64 end locator/record are honored for `0xFFFF`/`0xFFFFFFFF` sentinel fields, and zip64 extra fields (type `0x0001`) in central/local headers supply 64-bit sizes/offsets.
- Local and central headers are cross-checked; mismatches are logged but not fatal. The CRC-32 of each entry's stored bytes is verified at open; a mismatch logs "CRC mismatch in organ package — file corrupted?" but is **not fatal**.
- **Filename encoding**: UTF-8 if flag bit 11 is set, otherwise **code page 437**.
- Entry-name validation (fatal per archive if violated): no `\` in the raw zip name; no absolute paths (`/…` or `X:/…`); no `//`, `/./`, `/../`; no leading `./` or `../`; no trailing `/.` or `/..`. Directory entries (name ending `/`) are skipped (must have size 0). Duplicate file names are fatal.
- After validation, every `/` in an entry name is replaced by `\` — **all in-archive lookups therefore use backslash-separated, case-sensitive names**.

### 7.2 Package identity

The **package ID** is the SHA-1 hash (40 uppercase hex chars) of the entire `.orgue` file bytes (`GOArchiveReader::GenerateFileHash`). Dependencies between packages reference these IDs. (A local `.idx` cache of the parsed directory, keyed by size/mtime, exists under the cache directory — `GOArchiveIndex.cpp` — it is a private optimization, not part of the format.)

### 7.3 The package index: `organindex.ini`

Every package must contain a file named exactly `organindex.ini` at the archive root, in the INI format of section 1 (it is written UTF-8 with BOM by GO). It is read with `CMBSetting`-typed reads (`GOArchiveManager::ReadIndex`):

- `[General]`
  - `Title` (string, required): package display name.
  - `DependencyCount` (integer 0–100, optional, default 0).
  - `OrganCount` (integer 0–100, required).
- `[DependencyNNN]` for NNN = `001` … zero-padded 3 digits, one per dependency:
  - `PackageID` (string, required): SHA-1 ID of the depended-on package. Must not equal the containing package's own ID.
  - `Title` (string, required).
- `[OrganNNN]` for NNN = `001` …:
  - `Filename` (string, required): path of the `.organ` file **inside** the archive, backslash-separated (matched verbatim against the normalized entry names).
  - `ChurchName` (string, required).
  - `OrganBuilder`, `RecordingDetails` (strings, optional).

### 7.4 Loading an organ from a package

- The archive plus all dependency archives are opened; the ODF named by the chosen `Organ` entry's `Filename` is opened from whichever archive contains it and parsed exactly as in sections 1–3 (the entry content is typically a gzip stream, handled by 1.1).
- A bundled combination file is looked up as `<odf path without extension>.cmb` inside the archive containing the ODF.

---

## 8. What GrandOrgue itself writes (for round-trip reference)

`GOConfigFileWriter.cpp` emits: UTF-8 with BOM, `[Group]` then `Name=Value` lines, LF (`\n`) line endings, groups and keys in lexicographic order, no comments, no spaces around `=`; `.cmb` files are optionally gzip-compressed. An importer that also exports should follow the same conventions.

---

## 9. Fatal vs non-fatal — quick reference

| Condition | Behavior |
|---|---|
| I/O failure, OOM, gzip failure, invalid UTF-8 (with BOM) | file read fails |
| Malformed line / entry before section / bad section header | logged, line skipped |
| Duplicate section / duplicate key | warning, merge / last wins |
| Missing required value (outside Setter/PanelNNNSetter/FrameGeneral groups) | fatal |
| Empty trimmed value on a non-string read | treated as missing |
| Boolean not starting with Y/N; invalid enum; invalid color; unparsable int/size/fontsize | fatal |
| Unparsable float | silently 0.0 |
| Int/float out of caller's range, ODFSetting | fatal |
| Int/float out of caller's range, CMBSetting | logged, default used |
| Integer trailing garbage | logged, prefix value used |
| Float/size/fontsize trailing garbage | silently ignored |
| Wrong case in ODF group/key (default settings) | warning, value used |
| Same section consumed by two objects (`MarkGroupInUse`) | fatal |


---

# Part 2 — Top-Level Organ Loading Flow and the [Organ] Section

Source basis: GrandOrgue source at `src/grandorgue/GOOrganController.cpp/.h`, `src/grandorgue/model/GOOrganModel.cpp/.h`, `src/grandorgue/loader/GOFileStore.cpp`, `src/grandorgue/loader/GOLoaderFilename.cpp`, `src/core/config/GOConfigFileReader.cpp`, `src/core/config/GOConfigReader.cpp`, `src/core/config/GOConfigReaderDB.cpp`, `src/core/GOOrgan.cpp`, `src/core/go_path.cpp`, `src/core/archive/GOArchive.cpp`, `src/core/archive/GOArchiveReader.cpp`, `src/grandorgue/model/pipe-config/GOPipeConfig.cpp`, `src/grandorgue/model/GOWindchest.cpp`, `src/grandorgue/control/GOPistonControl.cpp`, `src/grandorgue/control/GOButtonControl.cpp`, `src/grandorgue/model/GODrawstop.cpp`, `src/grandorgue/model/GODivisionalCoupler.cpp`, `src/grandorgue/combinations/control/GOGeneralButtonControl.cpp`, `src/grandorgue/combinations/model/GOGeneralCombination.cpp`, `src/grandorgue/combinations/model/GOCombination.cpp`.

## 1. ODF file container format (INI dialect)

An ODF (`*.organ`) is a line-based INI-style text file (`GOConfigFileReader::Read`):

- **Compression**: the whole file may be gzip-compressed (checked by gzip magic `0x1F 0x8B`); it is transparently decompressed before parsing.
- **Encoding**: if the file starts with a UTF-8 BOM (`EF BB BF`) it is decoded as UTF-8; otherwise as **ISO-8859-1** (Latin-1). There is no other encoding detection.
- **Line endings**: lines split on `\n`; a trailing `\r` is stripped (CRLF and LF both work).
- **Comments**: everything from the **first `;` anywhere in a line** to end of line is discarded, then the remainder is right-trimmed. Consequence: values may not contain a semicolon. Blank (or comment-only) lines are ignored.
- **Sections**: a line starting with `[` and ending with `]` (trailing whitespace tolerated with an error message) opens group `Name` (text between brackets, case-sensitive). A duplicate `[Group]` produces a warning and its entries are merged into the existing group.
- **Entries**: `Key=Value`, split at the **first** `=`; the key must be non-empty. No whitespace stripping is done around key or value at this stage (a key of `Foo ` is distinct from `Foo`). A duplicate key within a group warns; the **later value wins**. Entries before any section header are errors and are skipped.
- A SHA-based hash of the raw (still-compressed) file bytes is remembered as the *ODF hash* (used to match `.cmb` files).

### 1.1 The two key namespaces (ODF vs CMB)

All parsed entries are stored in a database keyed by `Group/Key` (`GOConfigReaderDB`). There are two independent namespaces:

- **ODFSetting** – entries from the `.organ` file.
- **CMBSetting** – entries from a companion settings/combination file (`.cmb`), or, for legacy "GrandOrgue 0.2" ODFs, from sections inside the ODF whose names begin with `_` (the `_` is stripped; the user is asked whether to import them).

A read always names its namespace; there is no fallback between them. Many keys are read twice, once per namespace (CMB overrides ODF, e.g. `DefaultToEngaged`).

Key lookup is **case-sensitive** when the "ODF check" (strict) option is on. When strict mode is off, an additional lower-cased index provides a case-insensitive fallback for ODF entries (with a "Incorrect case" warning). An importer should treat section and key names case-insensitively but warn on mismatched case.

After loading, every never-read ODF entry produces an "Unused ODF entry" warning (capped at 3000). So *extra* keys or whole extra sections are non-fatal; **missing required keys are fatal** (`Missing required value section '%s' entry '%s'` is thrown and the load aborts). Exception: keys in groups whose name starts with `Setter`, `Panel###Setter…`, or `FrameGeneral` are never treated as required (`GOConfigReader::Read`).

### 1.2 Value types (`GOConfigReader`)

- **String** (`ReadString`): returned verbatim (no trim). Empty value counts as *present*. `ReadStringTrim` additionally strips trailing spaces (warns in strict mode); `ReadStringNotEmpty` warns if blank. Default when absent and optional: `""` unless stated.
- **Boolean** (`ReadBoolean`): value is whitespace-trimmed on both sides; empty ⇒ treated as absent. `Y`/`y` ⇒ true, `N`/`n` ⇒ false; otherwise upper-cased first letter `Y`/`N` is accepted with a "Strange boolean value" warning; anything else is a fatal error.
- **Integer** (`ReadInteger`): trimmed; parsed with `std::stol`; trailing garbage after the number logs an error but the parsed prefix is used. Value outside `[min,max]` in the **ODF namespace is fatal**; in the CMB namespace it logs an error and substitutes the default.
- **Float** (`ReadFloat`): trimmed; a first `,` is replaced by `.` (warning); parsed in the C locale; same range rules as integers.
- **Enum**: exact, case-sensitive match against the allowed names; anything else fatal.
- **Filename** (`ReadFileName`): `ReadStringTrim`; when the optional "HW1 compatibility check" is on, a `/` in the name only produces a portability warning.

Number formatting of multi-instance keys/sections is always `%03d` (three digits, zero-padded, 1-based unless stated): `Enclosure001`, `WindchestGroup001`, `Switch001`, `Tremulant001`, `Rank001`, `General001`, `ReversiblePiston001`, `DivisionalCoupler001`, `Panel001`, `Manual000`…

## 2. Locating the ODF and its referenced files

### 2.1 Plain `.organ` file (directory mode)

`GOOrganController::Load` normalizes the ODF path to an absolute path (`go_normalize_path` = `wxFileName::MakeAbsolute` + `GetLongPath`) and sets the **file store directory** to the ODF's parent directory (`GOFileStore::SetDirectory`). Every relative filename in the ODF is then resolved (`GOLoaderFilename`, root kind *ODF*) as:

1. Normalize the string with `wxFileName::Normalize(wxPATH_NORM_DOTS)` (collapses `.`/`..`). If the result is an **absolute path**, it is used as-is against the host filesystem (bypassing the ODF directory) — legal but non-portable.
2. Replace every `\` with the platform path separator (`GOLoaderFilename::generateFullPath`). ODF paths are canonically written with backslashes (`Images\stop.png`, `..\CommonSamples\a.wav` — `..` is allowed and may escape the organ directory).
3. Prepend `<odf-directory>` + separator.
4. Open the file; if it does not exist the load of that object fails ("File '%s' does not exist"). **There is no case-insensitive fallback**: on case-sensitive filesystems the ODF path must match the on-disk case exactly. On Windows, GrandOrgue compares the path against `GetLongPath()` and warns "Filename '%s' not compatible with case sensitive systems" when the case differs.

A third root kind, *Resource*, resolves against GrandOrgue's own resource directory and is never used for ODF references.

### 2.2 `.orgue` package (archive mode)

A `.orgue` file is a ZIP archive. On load (`GOFileStore::LoadArchives`), the main archive and all its **dependency archives** are opened (main first; dependency IDs come from the package metadata; per-archive content indexes are cached as `<hash>.idx` in the cache directory). Rules from `GOArchiveReader`:

- Entry names are read from the ZIP central directory, validated (no `\` inside stored names, no leading `/`, no drive letter, no `//`, `/./`, `/../` etc.), directories are skipped, duplicates are errors; then **every `/` in the entry name is replaced by `\`**.
- File lookup (`GOArchive::containsFile` / `OpenFile`) is an **exact, case-sensitive string comparison** of the ODF path against the normalized entry names, searched in the main archive first, then in each dependency in load order (`GOFileStore::FindArchiveContaining`).

Consequently, inside a package, all ODF file references must use backslash separators and exact case. The "ODF path" of an organ inside a package is itself an entry name (e.g. `myorgan.organ`). Relative sample paths (`SampleFilename`, image paths, etc.) resolve to archive entry names directly — there is no directory prefixing beyond what the ODF string itself contains.

### 2.3 InfoFilename resolution (`GOOrganController::ReadOrganFile`)

- If `InfoFilename` is empty: take the ODF path, change the extension to `.html`; if that file exists (directory mode only), it becomes the organ information page.
- If given: resolve like any ODF-relative filename against the ODF directory (directory mode only; ignored in package mode); it must exist and have extension `.html` or `.htm`, otherwise (with ODF checking on) a warning is issued and it is ignored.

## 3. Top-level load sequence (`GOOrganController::Load`)

1. If the organ comes from a package: load main archive + dependencies; `m_odf` = entry name of the ODF. Else: normalize ODF path, set store directory.
2. Compute the *organ hash* (`GOOrgan::GetOrganHash`): package ⇒ hash(archiveID, odfPath); file ⇒ hash of the fully normalized absolute path (dots, tilde, case, symlinks resolved). This hash names the per-organ settings file `<SettingsDir>/<hash>-<preset>.cmb` and cache `<CacheDir>/<hash>-<preset>.cache` (`GOStdFileName`).
3. Parse the ODF file into the ODFSetting namespace.
4. Select a companion settings file, in priority order: (a) explicit file passed by the caller; (b) `<SettingsDir>/<hash>-<preset>.cmb` if it exists; (c) a **bundled** `.cmb` next to the ODF: `<odf-path-without-extension>.cmb` (in package mode looked up inside the archive that contains the ODF). If one is found, parse it into the CMBSetting namespace; if its `[Organ] ChurchName` (trimmed) differs from the ODF's, warn; if it contains `ODFHash` differing from the current ODF hash, ask the user and possibly discard it. (d) If none found, sections prefixed `_` inside the ODF itself are offered as legacy GO-0.2 settings.
5. Read `[Organ]` and all objects (section 5 below).
6. Report unused entries.
7. Resolve references and load all samples (multi-threaded; optionally served from the binary cache file). Sample loading details are out of scope here.

## 4. `[Organ]` — complete key reference

### 4.1 ODF keys (namespace ODFSetting)

Read in this exact order:

| # | Key | Type | Range | Required | Default | Notes |
|---|-----|------|-------|----------|---------|-------|
| 1 | `HauptwerkOrganFileFormatVersion` | string | – | no | – | Read and discarded (only to suppress the unused-entry warning). |
| 2 | `ChurchAddress` | string | – | **yes** | – | Informational. |
| 3 | `OrganBuilder` | string | – | no | `""` | |
| 4 | `OrganBuildDate` | string | – | no | `""` | |
| 5 | `OrganComments` | string | – | no | `""` | |
| 6 | `RecordingDetails` | string | – | no | `""` | |
| 7 | `InfoFilename` | filename | – | no | `""` | See §2.3. |
| 8 | `NumberOfPanels` | int | 0–100 | no | 0 | Sections `[Panel001]`…`[Panel%03d]`. |
| 9 | `ChurchName` | string (trailing-trimmed) | – | **yes** | – | The **organ name / identity** (see §9). |
| 10 | `DivisionalsStoreIntermanualCouplers` | bool | Y/N | **yes** | – | Combination semantics flag. |
| 11 | `DivisionalsStoreIntramanualCouplers` | bool | Y/N | **yes** | – | |
| 12 | `DivisionalsStoreTremulants` | bool | Y/N | **yes** | – | |
| 13 | `GeneralsStoreDivisionalCouplers` | bool | Y/N | **yes** | – | Also makes `NumberOfDivisionalCouplers` in `[General%03d]` required. |
| 14 | `CombinationsStoreNonDisplayedDrawstops` | bool | Y/N | no | N | Default for drawstops' `StoreInDivisional`/`StoreInGeneral`. |
| 15 | `NumberOfWindchestGroups` | int | 1–999 | **yes** | – | Sections `[WindchestGroup%03d]`. |
| 16 | `AmplitudeLevel` | float | 0–1000 | no | 100 | Root pipe-config (inherited by every windchest/rank/stop/pipe). |
| 17 | `Gain` | float | −120–40 | no | 0 | dB, root pipe-config. |
| 18 | `PitchTuning` | float | −1800–1800 | no | 0 | cents, root pipe-config. |
| 19 | `PitchCorrection` | float | −1800–1800 | no | 0 | cents, root pipe-config. |
| 20 | `TrackerDelay` | int | 0–10000 | no | 0 | ms, root pipe-config. |
| 21 | `Percussive` | bool | Y/N | no | N (tri-state, default false at root) | Root pipe-config. |
| 22 | `HasIndependentRelease` | bool | Y/N | no | N | **Only read if the effective `Percussive` is true** (`GOPipeConfig::Load`). |
| 23 | `NumberOfManuals` | int | 1–16 | **yes** | – | Count of manuals **excluding** the pedal. |
| 24 | `HasPedals` | bool | Y/N | **yes** | – | Determines the first manual index (0 or 1). |
| 25 | `NumberOfEnclosures` | int | 0–999 | **yes** | – | Sections `[Enclosure%03d]`. |
| 26 | `NumberOfSwitches` | int | 0–999 | no | 0 | Sections `[Switch%03d]`. |
| 27 | `NumberOfTremulants` | int | 0–999 | **yes** | – | Sections `[Tremulant%03d]`. |
| 28 | `NumberOfRanks` | int | 0–999 | no | 0 | Sections `[Rank%03d]`. |
| 29 | `NumberOfReversiblePistons` | int | 0–32 | **yes** | – | Sections `[ReversiblePiston%03d]`. |
| 30 | `NumberOfDivisionalCouplers` | int | 0–8 | **yes** | – | Sections `[DivisionalCoupler%03d]`. |
| 31 | `NumberOfGenerals` | int | 0–99 | **yes** | – | Sections `[General%03d]` (read in the `LoadCmbButtons` phase). |

Additionally, `[Organ]` doubles as the **main panel** section for old-format GUIs (no `[Panel000]` present): the GUI loader reads `HasPedals` again plus display keys from it — `NumberOfImages` (int 0–999, default 0, sections `[Image%03d]`), `NumberOfSetterElements` (int 0–999, default 0, sections `[SetterElement%03d]`; old format only), and all `Disp*` screen metrics and per-object display keys (covered by the GUI/panel specification; `GOGUIPanel.cpp`). If a `[Panel000]` section with `NumberOfGUIElements` exists, the "new format" panel model is used instead.

### 4.2 CMB-namespace keys under `[Organ]` (from `.cmb`, not the ODF)

`ChurchName`, `ChurchAddress`, `ODFPath`, `ODFHash`, `ArchiveID` (informational, skipped), `GrandOrgueVersion` (string), `Volume` (int −120–100, default = global setting; values > 20 are reset to 0), `Temperament` (string, temperament name). The root pipe-config additionally reads CMB overrides with no prefix: `AudioGroup`, `Amplitude` (0–1000, default = `AmplitudeLevel`), `UserGain` (−120–40, default = `Gain`), `Tuning` (legacy, −1800–1800), `ManualTuning`, `AutoTuningCorrection`, `Delay`, `BitsPerSample` (−1/8–24), `Channels` (−1–2), `LoopLoad` (−1–2), `Compress`, `AttackLoad`, `ReleaseLoad` (int tri-state −1/0/1), `IgnorePitch` (Y/N), `ReleaseTail` (0–3000 ms), `ToneBalance` (−100–100) (`GOPipeConfig::LoadFromCmb`). The same CMB key set exists for every pipe-config node in the tree.

## 5. Object creation and load order

Exact order of `GOOrganModel::Load` followed by `GOOrganController::ReadOrganFile`:

1. Windchest objects are **created** (empty) — count = `NumberOfWindchestGroups`.
2. Root pipe-config loaded from `[Organ]` (rows 16–22 above).
3. `NumberOfManuals` and `HasPedals` read. `firstManual = HasPedals ? 0 : 1`; `odfManualCount = NumberOfManuals + 1`. Manual objects created for indices `firstManual … odfManualCount−1`, plus **4 floating manuals** (indices `odfManualCount … odfManualCount+3`) used by virtual couplers.
4. **Enclosures** loaded: sections `[Enclosure001] … [Enclosure%03d]`, i = 1…`NumberOfEnclosures`.
5. **Switches** loaded: `[Switch%03d]`, i = 1…`NumberOfSwitches` (before manuals because manuals reference switches).
6. **Tremulants** loaded: `[Tremulant%03d]`, i = 1…`NumberOfTremulants`.
7. **Windchests** loaded: `[WindchestGroup%03d]`, i = 1…`NumberOfWindchestGroups` (§6). Ranks/stops loaded later may reference windchest numbers 1…N.
8. **Ranks** loaded: `[Rank%03d]`, i = 1…`NumberOfRanks`.
9. **Manuals** loaded: section `[Manual%03d]` where the number is the *manual index*: with `HasPedals=Y` the pedal is `[Manual000]` and manuals continue `[Manual001]…[Manual%03d(NumberOfManuals)]`; with `HasPedals=N` sections run `[Manual001]…[Manual%03d(NumberOfManuals)]` and no `Manual000` exists. (Stops, couplers, manual switches/tremulants and their sub-sections are loaded from within each manual.)
10. Floating manuals initialized (internal groups `SetterFloating001…004`; nothing required in the ODF; their compass is derived from the min/max MIDI notes of the ODF manuals).
11. **Reversible pistons** loaded: `[ReversiblePiston%03d]`, i = 1…`NumberOfReversiblePistons` (§7).
12. **Divisional couplers** loaded: `[DivisionalCoupler%03d]`, i = 1…`NumberOfDivisionalCouplers` (§8).
13. Internal wiring: recorder element IDs (`E%d`, `S%d`, `T%d`), switch contexts, resolution of by-name references.
14. Virtual couplers initialized (purely internal; CMB MIDI settings only, groups like `VirtualCoupler…` — nothing read from the ODF).
15. **Generals** loaded (`LoadCmbButtons`): `[General%03d]`, i = 1…`NumberOfGenerals` (§8.1); then per manual, **divisionals**: `[Manual%03d]/NumberOfDivisionals` (int 0–999, optional, default 0) and keys `Divisional%03d = <n>` giving the *section number*: section `[Divisional%03d(n)]` (n 1–999). Each divisional section may be used only once globally (`MarkGroupInUse` throws "Section %s already in use").
16. Element creators (setter, metronome, recorders…) load their internal (CMB/setter) state; labels `SetterMasterPitch`, `SetterMasterTemperament`, main-window data.
17. **Panels**: the implicit main panel is loaded from `[Organ]` (or `[Panel000]` in new format), then `[Panel%03d]`, i = 1…`NumberOfPanels`.
18. **Combinations**: every saveable object re-reads its state from the CMB namespace (`ReadCombinations`).
19. The organ name is hashed to produce two 28-bit sample-set IDs used in GO's MIDI SysEx protocol.

Counts strictly control what is loaded: sections beyond the declared counts are ignored (unused-entry warnings); a count pointing at a missing section fails fatally at that section's first required key.

## 6. Windchests — `[WindchestGroup%03d]` (`GOWindchest::Load`)

Read order:

| Key | Type | Range | Required | Default |
|-----|------|-------|----------|---------|
| `NumberOfEnclosures` | int | 0 – (organ `NumberOfEnclosures`) | yes | – |
| `NumberOfTremulants` | int | 0 – (organ `NumberOfTremulants`) | yes | – |
| `Enclosure%03d` (i=1…) | int | 1 – enclosure count | yes | – (1-based global enclosure number) |
| `Tremulant%03d` (i=1…) | int | 1 – tremulant count | yes | – (1-based global tremulant number) |
| `Name` | string, non-empty | – | no | `Windchest N` (N = 1-based index) |
| pipe-config keys | | | | `AmplitudeLevel`, `Gain`, `PitchTuning`, `PitchCorrection`, `TrackerDelay`, `Percussive`, `HasIndependentRelease` (same types/ranges/defaults as §4.1 rows 16–22), parent node = `[Organ]` root; `Percussive` inherits the organ-level effective value when absent; `HasIndependentRelease` again only read when effectively percussive. Plus the CMB override set of §4.2. |

The windchest volume is the product of its enclosures' attenuations; tremulants listed here apply to all pipes on the chest.

## 7. Reversible pistons — `[ReversiblePiston%03d]` (`GOPistonControl::Load`)

| Key | Type | Range | Required | Default |
|-----|------|-------|----------|---------|
| `ObjectType` | string | `Stop`, `Coupler`, `Tremulant`, `Switch` (compared case-insensitively after upper-casing) | yes | – |
| `ManualNumber` | int | firstManualIndex – odfManualCount−1 | yes (only for `Stop`/`Coupler`) | – |
| `ObjectNumber` | int | 1 – count of that object type (stop/ODF-coupler count of the manual, or global tremulant/switch count) | yes | – |

Then the standard button keys (`GOPushbuttonControl` → `GOButtonControl::Load`):

| Key | Type | Range | Required | Default |
|-----|------|-------|----------|---------|
| `Name` | string, non-empty | – | yes | – |
| `ShortcutKey` | int | 0–255 | no | 0 (keyboard shortcut code; also CMB-overridable) |
| `Displayed` | bool | Y/N | no | N (legacy main-panel display flag) |
| `DisplayInInvertedState` | bool | Y/N | no | N |

If the referenced drawstop is read-only (a logical function switch), an error is logged but loading continues. Pressing the piston toggles ("pushes") the target drawstop; the piston lamp mirrors `engaged XOR DisplayInInvertedState`.

## 8. Divisional couplers — `[DivisionalCoupler%03d]` (`GODivisionalCoupler::Load` + `GODrawstop::Load`)

| Key | Type | Range | Required | Default |
|-----|------|-------|----------|---------|
| `BiDirectionalCoupling` | bool | Y/N | yes | – |
| `NumberOfManuals` | int | 1 – (manualAndPedalCount − firstManualIndex + 1) | yes | – |
| `Manual%03d` (i=1…) | int | firstManualIndex – manualAndPedalCount | yes | – (manual indices participating) |

Then the drawstop keys:

| Key | Type | Range | Required | Default |
|-----|------|-------|----------|---------|
| `Function` | enum | `Input`, `And`, `Or`, `Not`, `Nand`, `Nor`, `Xor` (exact case) | no | `Input` |
| `DefaultToEngaged` | bool | Y/N | yes if Function=Input | – (CMB may override) |
| `GCState` | int | −1–1 | no | 0 (−1: unaffected by General Cancel; 0: off on GC; 1: on on GC) |
| `SwitchCount` | int | 1 – switch count | yes for And/Or/Nand/Nor/Xor (fixed 1 for Not) | 1 |
| `Switch%03d` | int | 1 – switch count | yes (per i=1…SwitchCount) | 1 (controlling switch; self/duplicate references fatal) |
| `Name`, `ShortcutKey`, `Displayed`, `DisplayInInvertedState` | as §7 | | | |
| `StoreInDivisional` | bool | Y/N | no | **false** for divisional couplers |
| `StoreInGeneral` | bool | Y/N | no | value of organ `GeneralsStoreDivisionalCouplers` |

(For ordinary drawstops the `StoreIn*` defaults are `CombinationsStoreNonDisplayedDrawstops OR not-read-only`; divisional couplers override them as shown.) Non-`Input` functions make the object read-only. When engaged, pressing divisional N on one member manual presses divisional N on the following member manuals (all members if `BiDirectionalCoupling=Y`, only later-listed ones otherwise), transitively through other engaged divisional couplers.

## 8.1 Generals — `[General%03d]` (`GOGeneralButtonControl::Load` → `GOGeneralCombination`)

Read order: `Protected` (bool, optional, default N — prevents overwriting by the setter), then the combination content, then the button keys of §7 (`Name` required, `ShortcutKey`, `Displayed`, `DisplayInInvertedState`).

Combination content (`GOCombination::LoadCombination` with ODF namespace; the same key set is re-read from the CMB namespace at combination-restore time, CMB taking precedence when `NumberOfStops` exists there):

| Key | Type | Range | Required | Default |
|-----|------|-------|----------|---------|
| `IsFull` | bool | Y/N | no | N |
| `NumberOfStops` | int | 0 – total stop count | yes | – |
| `NumberOfCouplers` | int | 0 – total ODF coupler count | yes | – |
| `NumberOfTremulants` | int | 0 – tremulant count | yes | – |
| `NumberOfSwitches` | int | 0 – switch count | no | 0 |
| `NumberOfDivisionalCouplers` | int | 0 – divisional coupler count | only if `GeneralsStoreDivisionalCouplers=Y` | 0 |
| `StopManual%03d` | int | firstManualIndex – manualAndPedalCount | yes (i=1…NumberOfStops) | – |
| `StopNumber%03d` | int | −999 – 999 | yes | – |
| `CouplerManual%03d` | int | firstManualIndex – manualAndPedalCount | yes (i=1…NumberOfCouplers) | – |
| `CouplerNumber%03d` | int | ±(that manual's coupler count) | yes | – |
| `TremulantNumber%03d` | int | ±tremulant count | yes (i=1…NumberOfTremulants) | – |
| `SwitchManual%03d` | int | −1 – manualAndPedalCount | no | −1 (−1 = global switch) |
| `SwitchNumber%03d` | int | ±switch count | yes (i=1…NumberOfSwitches) | – |
| `DivisionalCouplerNumber%03d` | int | ±divisional coupler count | yes (i=1…NumberOfDivisionalCouplers) | – |
| `HasScope` | bool | Y/N | no | N |

Sign convention: `abs(number)` is the 1-based object number; positive = element drawn/on, negative = element off. Elements not mentioned stay untouched. References to objects excluded from the general template (e.g. `StoreInGeneral=N`) or duplicates log errors but are not fatal.

## 9. Organ identity and registration (`GOOrgan`, `GOOrganController`)

The trimmed `[Organ] ChurchName` is the organ's display identity: it is stored (with `OrganBuilder` and `RecordingDetails`) in GrandOrgue's organ list, shown as "ChurchName, OrganBuilder", used to name the per-organ combination directory (`<CombinationsPath>/<ChurchName>/`), embedded in exported YAML combination files, and checked when importing `.cmb`/`.yaml` files (a mismatch prompts/warns). It also seeds the two sample-set IDs for GO's SysEx protocol. The *storage* identity (settings/cache filenames), however, is the hash of the ODF location (§3.2), so renaming/moving the ODF orphans the settings while a changed `ChurchName` only logs "Organ %s changed its name".

## 10. Combination/settings files (brief)

A companion `.cmb` file (either the auto-saved `<hash>-<preset>.cmb`, a bundled `<odfname>.cmb`, or a user-exported file) uses the same INI syntax as the ODF but populates the CMB namespace: `[Organ]` holds `ChurchName`/`ChurchAddress`/`ODFPath`/`ODFHash`/`ArchiveID`/`GrandOrgueVersion`/`Volume`/`Temperament`; object sections hold MIDI configuration, per-node pipe-config overrides (`Amplitude`, `UserGain`, `ManualTuning`…), `DefaultToEngaged` states, and re-saved combination contents for `[General%03d]`, `[Divisional%03d]` and internal setter banks (`[SetterGeneral…]`, `[SetterCrescendo…]`, `[FrameGeneral…]`); it is matched to the ODF via `ODFHash` (hash of the raw ODF bytes) with a user override. Since GO 3.x combinations can also be exported/imported as YAML documents (`GOYamlModel`, content type "GrandOrgue Combinations") that reference elements by manual number *and object name* with name-based fallback matching; an ODF importer does not need to support these beyond knowing they exist.


---

# Part 3 — The Playable Organ Model: Manuals, Stops, Ranks, Pipes, Couplers, Switches, Tremulants, Windchests, Enclosures

## Scope and conventions

This section specifies the playable organ model of a GrandOrgue ODF (`.organ` file) exactly as the reference implementation reads it (sources: `src/grandorgue/model/*`, `src/grandorgue/control/*`, `src/grandorgue/combinations/*`, `src/core/config/GOConfigReader.cpp`).

An ODF is an INI-style file: `[Group]` section headers with `Key=Value` entries. Facts about the reader (`GOConfigReader.cpp`, `GOConfigReaderDB.cpp`):

- **Lookup**: `group/key` is looked up case-sensitively first; if not found, a case-insensitive lookup is done (accepted with a warning). Duplicate entries in one group produce a warning; the last one wins.
- **Empty values**: for all trimmed reads (integers, floats, booleans, enums, file names) a value that is empty after trimming whitespace is treated as *missing* (the default applies, or an error if required).
- **Required**: a missing required key is a fatal load error.
- **Boolean** (`ReadBoolean`/`ReadBooleanTriple`): `Y`/`y` = true, `N`/`n` = false. Any other value starting with `Y`/`N` (case-insensitive) is accepted with a warning; anything else is a fatal error. A "tri-state boolean" additionally distinguishes *absent* (inherit/default).
- **Integer** (`ReadInteger`): parsed with `stol`; trailing garbage is logged but the parsed prefix is used; a value out of `[min..max]` is a **fatal error** for ODF keys.
- **Float** (`ReadFloat`): C-locale parse, `.` decimal point (`,` accepted with a warning); out of range is fatal.
- **Enum** (`ReadEnum`): exact string match against the listed names; unknown value is fatal.
- **Tri-state-from-int** (`ReadBool3FromInt`, used for `IsTremulant`): integer `-1` (default/both), `0` (false), `1` (true).
- Numbered keys/sections use `%03d` zero-padding (`Stop001`, `Pipe012`, `Rank%03d`, `MIDIKey000`).
- Each referenced `[Stop###]`, `[Coupler###]`, `[Divisional###]` section may be used by only one referrer organ-wide (`GOConfigReader::MarkGroupInUse` throws "Section already in use").

Notation below: `int(min..max)`, `float(min..max)`, `bool`, `bool3`, `string`. "Default" implies the key is optional; "required" keys have no default.

## Organ-level context ([Organ] keys that shape the model)

Read in `GOOrganModel.cpp` (`GOOrganModel::Load`). Load order matters for references: enclosures → switches → tremulants → windchests → ranks → manuals (each loads its stops and couplers) → reversible pistons → divisional couplers → `REF:` pipe resolution. Divisionals and generals are loaded afterwards (`LoadCmbButtons`).

| Key | Type | Req/Default | Meaning |
|---|---|---|---|
| `NumberOfManuals` | int(1..16) | required | Manuals `[Manual001]`..`[Manual0NN]`. Internally manual indices run 0..NumberOfManuals; index 0 is the pedal. |
| `HasPedals` | bool | required | If `Y`, section `[Manual000]` (the pedal) exists and the first manual index is 0, else 1. |
| `NumberOfEnclosures` | int(0..999) | required | `[Enclosure###]` sections. |
| `NumberOfSwitches` | int(0..999) | default 0 | `[Switch###]` sections. Loaded before manuals (manuals/drawstops reference them). |
| `NumberOfTremulants` | int(0..999) | required | `[Tremulant###]` sections. |
| `NumberOfWindchestGroups` | int(1..999) | required | `[WindchestGroup###]` sections. |
| `NumberOfRanks` | int(0..999) | default 0 | Standalone `[Rank###]` sections. |
| `NumberOfDivisionalCouplers` | int(0..8) | required | `[DivisionalCoupler###]` sections. |
| `DivisionalsStoreIntermanualCouplers` | bool | required | Affects divisional defaults (see below). |
| `DivisionalsStoreIntramanualCouplers` | bool | required | ditto |
| `DivisionalsStoreTremulants` | bool | required | ditto |
| `GeneralsStoreDivisionalCouplers` | bool | required | Default for divisional couplers' `StoreInGeneral`. |
| `CombinationsStoreNonDisplayedDrawstops` | bool | default N (`GOOrganModel.cpp:80`) | Feeds `StoreInDivisional`/`StoreInGeneral` defaults. |

`[Organ]` is also the **root pipe-config node**: it accepts the pipe-config keys `AmplitudeLevel`, `Gain`, `PitchTuning`, `PitchCorrection`, `TrackerDelay`, `Percussive`, `HasIndependentRelease` (no prefix) — see "Pipe config inheritance".

Internally 4 extra "floating" coupling manuals exist beyond the ODF manuals; therefore keys validated against "the manual count" (e.g. `DestinationManual`) accept `firstManualIndex .. NumberOfManuals+4`.

## Common button keys (GOButtonControl)

Every stop, coupler, switch, tremulant, divisional coupler, and divisional button reads (`GOButtonControl.cpp`, `GOMidiShortcutReceiver.cpp`):

| Key | Type | Req/Default | Meaning |
|---|---|---|---|
| `Name` | string (non-empty) | required | Display name. |
| `Displayed` | bool | default N | Legacy built-in-panel display flag (GUI). |
| `DisplayInInvertedState` | bool | default N | Invert displayed on/off image. |
| `ShortcutKey` | int(0..255) | default 0 | Keyboard shortcut code (0 = none). |
| `StopControlMIDIKeyNumber` | int(-1..127) | ignored | Legacy HW1 key, read only to suppress "unused" warnings (drawstop receivers). |
| `MIDIProgramChangeNumber` | int(0..128) | ignored | Legacy, ditto (pushbutton receivers). |

Buttons do **not** read `MIDIInputNumber`; manuals and enclosures do (`GOMidiReceivingSendingObject.cpp`).

## Drawstop logic (GODrawstop — base of Stop, Coupler, Switch, Tremulant, Divisional Coupler)

Source: `GODrawstop.cpp`.

| Key | Type | Req/Default | Meaning |
|---|---|---|---|
| `Function` | enum `Input`,`And`,`Or`,`Not`,`Nand`,`Nor`,`Xor` | default `Input` | Logic function of the element. |
| `DefaultToEngaged` | bool | **required if** Function=Input | Initial engaged state. |
| `GCState` | int(-1..1) | default 0 (Function=Input only) | General-Cancel behaviour: -1 = unaffected by GC, 0 = GC disengages, 1 = GC engages. |
| `SwitchCount` | int(1..organ switch count) | **required** for And/Or/Nand/Nor/Xor | Number of controlling switches. For `Not` it is fixed to 1 and the key is not read. |
| `Switch%03d` (001..SwitchCount) | int(1..organ switch count) | required | Global switch number (1-based, `[Switch###]`). Duplicates and self-reference are fatal errors. |
| `StoreInDivisional` | bool | default: see below | Whether divisional combinations capture this element by default. |
| `StoreInGeneral` | bool | default: see below | Same for generals. |

Semantics:
- `Function=Input`: the element is directly user-operable.
- Any other function makes the element **read-only**; its state is permanently computed from the referenced switches' engaged states: `And` = all on, `Or` = any on, `Not` = negation of the single input, `Nand`/`Nor` = negations, `Xor` = odd number of inputs on (`GODrawstop::Update`).
- Default for `StoreInDivisional`/`StoreInGeneral` = `CombinationsStoreNonDisplayedDrawstops` OR (element is user-operable, i.e. Function=Input). For couplers the divisional default is additionally AND-ed with `DivisionalsStoreIntermanualCouplers` (intermanual) or `DivisionalsStoreIntramanualCouplers` (intramanual) (`GOCoupler::SetupIsToStoreInCmb`); for tremulants with `DivisionalsStoreTremulants`; divisional couplers force `StoreInDivisional=false` and default `StoreInGeneral=GeneralsStoreDivisionalCouplers`.

## Switch ([Switch001]…)

`GOSwitch.cpp`: a switch has **only** the common button keys and the drawstop keys above (typically `DefaultToEngaged`, optionally a `Function` combining other switches). Switches referenced from exactly one manual (via the manual's `Switch%03d` list) become that manual's switches for divisional purposes; referenced from more than one manual (or none) they stay global.

## Manual ([Manual000] = pedal, [Manual001]…)

Source: `GOManual.cpp` (`Load`, `LoadDivisionals`).

| Key | Type | Req/Default | Meaning |
|---|---|---|---|
| `Name` | string (non-empty) | required | |
| `NumberOfLogicalKeys` | int(1..192) | required | Size of the logical key range driving stops/couplers. |
| `FirstAccessibleKeyLogicalKeyNumber` | int(1..NumberOfLogicalKeys) | required | Logical key number (1-based) of the first *playable* key. |
| `FirstAccessibleKeyMIDINoteNumber` | int(0..127) | required | MIDI note of the first playable key. |
| `NumberOfAccessibleKeys` | int(0..85) | required | Number of playable keys. |
| `MIDIInputNumber` | int(0..200) | default -1 (unset) | Selects a preconfigured initial MIDI match from user settings; no effect on the model itself. |
| `Displayed` | bool | default N | Legacy built-in panel flag. |
| `NumberOfStops` | int(0..999) | required | |
| `Stop%03d` (001..) | int(1..999) | required | Global stop **section number**: value `27` means this manual owns section `[Stop027]`. |
| `NumberOfCouplers` | int(0..999) | default 0 | |
| `Coupler%03d` | int(1..999) | required | Section number of `[Coupler###]`. |
| `NumberOfTremulants` | int(0..organ tremulant count) | default 0 | |
| `Tremulant%03d` | int(1..organ tremulant count) | required | Global tremulant number; duplicates within one manual are fatal. Association only affects divisionals (which tremulants a divisional can store). |
| `NumberOfSwitches` | int(0..organ switch count) | default 0 | |
| `Switch%03d` | int(1..organ switch count) | required | Global switch number; duplicates fatal; associates the switch with this manual. |
| `MIDIKey000` … `MIDIKey127` | int(0..127) | default = own number | Incoming MIDI note remap table: incoming note *n* is translated to `MIDIKey<n>` before key matching. |
| `NumberOfDivisionals` | int(0..999) | default 0 | |
| `Divisional%03d` | int(1..999) | required | Section number of `[Divisional###]`. |

**Logical vs accessible keys.** Logical keys are numbered 1..`NumberOfLogicalKeys` and are what stops and couplers operate on. Accessible (playable) key *k* (0-based, k < `NumberOfAccessibleKeys`) corresponds to logical key `FirstAccessibleKeyLogicalKeyNumber + k` and to MIDI note `FirstAccessibleKeyMIDINoteNumber + k`. Consequently the MIDI note of logical key 1 is `FirstAccessibleKeyMIDINoteNumber − FirstAccessibleKeyLogicalKeyNumber + 1` (`GOManual::GetFirstLogicalKeyMIDINoteNumber`). Coupled keys may reach logical keys outside the accessible window (that is what extra logical keys are for).

Runtime notes (`GOManual::SetKeyState`): each logical key holds one velocity per input source (source 0 = the physical keyboard, one slot per incoming coupler); the sounding velocity is the maximum. When one or more Unison Off couplers are engaged, the manual's stops receive the maximum over *coupler* inputs only (physical input excluded). Pressed MIDI velocity `v` is scaled internally to `(v<<2)+3` and scaled back down by `>>2` when passed to stops; each coupler hop decrements the internal velocity by 1.

## Stop ([Stop001]…)

Source: `GOStop.cpp`. In addition to button + drawstop keys:

| Key | Type | Req/Default | Meaning |
|---|---|---|---|
| `FirstAccessiblePipeLogicalKeyNumber` | int(1..128) | required | First *logical key* of the manual that this stop responds to. |
| `NumberOfAccessiblePipes` | int(1..192) | required | Number of consecutive logical keys the stop responds to. |
| `NumberOfRanks` | int(0..999) | default 0 | 0 selects the "internal rank" shorthand. |

**With `NumberOfRanks` ≥ 1** (rank-reference form), for i = 1..NumberOfRanks:

| Key | Type | Req/Default | Meaning |
|---|---|---|---|
| `Rank%03d` | int(1..organ `NumberOfRanks`) | required | Global rank number (`[Rank###]`). |
| `Rank%03dFirstPipeNumber` | int(1..rank pipe count) | default 1 | First pipe (1-based) of that rank used by this stop. |
| `Rank%03dPipeCount` | int(1..rankPipeCount−FirstPipeNumber+1) | default = max | Number of pipes used. |
| `Rank%03dFirstAccessibleKeyNumber` | int(1..NumberOfAccessiblePipes) | default 1 | Which stop key (1-based inside the stop's window) the first used pipe is placed on. |

**With `NumberOfRanks` = 0** (internal-rank shorthand): the stop section itself additionally contains a complete rank definition (all keys of the Rank section below, including `NumberOfLogicalPipes`, `WindchestGroup`, `Pipe%03d…`), plus:

| Key | Type | Req/Default | Meaning |
|---|---|---|---|
| `FirstAccessiblePipeLogicalPipeNumber` | int(1..192) | required | Pipe (1-based within the internal rank) that sounds on the first accessible stop key. |

For the internal rank, `FirstMidiNoteNumber` is optional and defaults to `manualFirstLogicalKeyMIDI + FirstAccessiblePipeLogicalKeyNumber − FirstAccessiblePipeLogicalPipeNumber` (clamped ≥ 0), where `manualFirstLogicalKeyMIDI` is the MIDI note of the manual's logical key 1. The implicit rank is registered in the global rank list (after the ODF-declared ranks) and its whole pipe range is mapped with `FirstAccessibleKeyNumber = 1`.

**Key→pipe mapping** (`GOStop::SetKeyState`/`SetRankKeyState`): logical manual key `L` is in the stop's window if `FirstAccessiblePipeLogicalKeyNumber ≤ L < FirstAccessiblePipeLogicalKeyNumber + NumberOfAccessiblePipes`; stop key index `s = L − FirstAccessiblePipeLogicalKeyNumber` (0-based). For each rank entry, `s` maps to rank pipe index `s + FirstPipeNumber − FirstAccessibleKeyNumber` (0-based) if `FirstAccessibleKeyNumber − 1 ≤ s` and `s < FirstAccessibleKeyNumber + PipeCount` (the code's upper bound is one lax; out-of-range pipe indices are discarded by the rank). A pipe sounds when the stop is engaged and the key is down; velocities from multiple stops on one rank pipe are combined by maximum (`GORank::SetPipeState`).

**Effect stop**: if a stop has exactly one rank entry and that rank has exactly one pipe, the stop is "for effects" (`GOStop::IsForEffects`): keys are ignored and the single pipe plays at velocity 0x7F while the stop is engaged, stopping when disengaged.

## Rank ([Rank001]…, or inline in a stop)

Source: `GORank.cpp`.

| Key | Type | Req/Default | Meaning |
|---|---|---|---|
| `Name` | string | required | (For inline ranks this is the stop's own `Name`.) |
| `FirstMidiNoteNumber` | int(0..256) | required for `[Rank###]`; computed default for stop-internal ranks (see Stop) | MIDI key number assigned to pipe 1; pipe *p* (1-based) has MIDI key `FirstMidiNoteNumber + p − 1` (used for temperament/retuning math, not for keyboard mapping). Note the exact spelling `FirstMidiNoteNumber`. |
| `NumberOfLogicalPipes` | int(1..192) | required | Number of `Pipe%03d` entries. |
| `WindchestGroup` | int(1..NumberOfWindchestGroups) | required | Windchest the rank belongs to; also the parent of the rank's pipe-config node. |
| `HarmonicNumber` | int(1..1024) | default 8 | Default harmonic number for the rank's pipes (8 = 8-foot unison). |
| `MinVelocityVolume` | float(0..1000) | default 100 | Volume % at MIDI velocity 0. |
| `MaxVelocityVolume` | float(0..1000) | default 100 | Volume % at MIDI velocity 127. Linear interpolation: `vol(v) = (Min + (Max−Min)·v/127)/100` (`GOSoundProvider::SetVelocityParameter`). |
| `AcceptsRetuning` | bool | default Y | Default for the rank's pipes: whether temperament retuning applies. |
| `Percussive` | bool3 | inherit | Pipe-config key (see inheritance). |
| `HasIndependentRelease` | bool3 | inherit | Pipe-config key; **only read when the effective Percussive at this node is true**. |
| `AmplitudeLevel`, `Gain`, `PitchTuning`, `PitchCorrection`, `TrackerDelay` | see pipe config | | Pipe-config keys, no prefix. |
| `Pipe%03d` (+sub-keys) | | required | See Pipes. |

## Pipes (Pipe001…, GOPipe/GOSoundingPipe/GOReferencePipe/GODummyPipe)

The value of `Pipe%03d` (whitespace-trimmed) selects the pipe type (`GORank::Load`):

- `DUMMY` — a silent pipe (`GODummyPipe.h`); no further keys are read.
- `REF:<manual>:<stop>:<pipe>` — reference pipe (`GOReferencePipe.cpp`). Exactly three `:`-separated unsigned integers after `REF:`. `<manual>` = ODF manual index (pedal = 0), valid range `firstManualIndex ≤ manual < NumberOfManuals+1`; `<stop>` = 1-based stop number within that manual; `<pipe>` = 1-based pipe number within that stop's **first** rank. Resolution happens after the whole model loads; playing the reference pipe forwards its velocity to the target pipe (velocities merged by maximum).
- anything else — a sample file path (relative to the organ package, `\` directory separators; a leading-`/` warning exists for portability) and the pipe is a sounding pipe; the value is also the pipe's **primary (first) attack sample**.

### Sounding-pipe keys (prefix `P` = `Pipe%03d`) — `GOSoundingPipe::Load`

| Key | Type | Req/Default | Meaning |
|---|---|---|---|
| `P` + `AmplitudeLevel`/`Gain`/`PitchTuning`/`PitchCorrection`/`TrackerDelay`/`Percussive`/`HasIndependentRelease` | | | Pipe-config keys with prefix `P` (see inheritance). |
| `P` + `HarmonicNumber` | int(1..1024) | default = rank's HarmonicNumber | Overtone number relative to 8' pitch; contributes `1200·log2(H/8)` cents to auto-tuning. |
| `P` + `WindchestGroup` | int(1..windchest count) | default = rank's | Per-pipe windchest override. |
| `P` + `MIDIKeyNumber` | int(-1..127) | default -1 | Overrides the sample's embedded MIDI note (pitch metadata). If set, the sample's embedded pitch *fraction* is ignored (treated as 0 unless `MIDIPitchFraction` is also given). |
| `P` + `MIDIPitchFraction` | float(0..100.0) | default -1 (unset) | Overrides the sample's pitch fraction, in **cents** (fraction of a semitone). |
| `P` + `AcceptsRetuning` | bool | default = rank's | Whether temperament offsets/auto-tuning apply to this pipe. |
| `MinVelocityVolume`, `MaxVelocityVolume` | float(0..1000) | default = rank value | **Quirk:** read from the pipe's *group* (rank or stop section) without the `Pipe%03d` prefix, i.e. these are effectively rank/stop-level keys re-read per pipe. |
| `P` + `AttackCount` | int(0..100) | default 0 | Number of *additional* attack samples beyond the primary one. |
| `P` + `ReleaseCount` | int(0..100) | default 0 | Number of separate release samples. If the pipe's effective `HasIndependentRelease` is true and `ReleaseCount`=0, a warning is issued. |

### Per-attack keys — `GOSoundingPipe::LoadAttackFileInfo`

Attack prefixes: `A` = `P` itself (primary attack) and `A` = `P + Attack%03d` for 001..AttackCount.

| Key | Type | Req/Default | Meaning |
|---|---|---|---|
| `A` (the value itself) | file path | required | Sample file (wav/wv). |
| `A+IsTremulant` | int(-1..1) | default -1 | Wave-tremulant sample selector: -1 = used regardless of tremulant state, 0 = only when wave tremulant off, 1 = only when on. |
| `A+LoadRelease` | bool | default = NOT effective `Percussive` | Whether the release segment embedded in this attack sample is used. |
| `A+MaxKeyPressTime` | int(-1..100000) ms | default -1 | Attack is eligible only if the previous key-press duration ≤ this (-1 = unlimited); used for multi-attack selection. |
| `A+AttackVelocity` | int(0..127) | default 0 | Minimum MIDI velocity for which this attack is eligible. |
| `A+MaxTimeSinceLastRelease` | int(-1..100000) ms | default -1 | Attack eligible only if time since the pipe's last release ≤ this (-1 = unlimited; there should be at least one attack with -1). |
| `A+CuePoint` | int(-1..158760000) | default -1 | Sample frame overriding the wav cue marker as release start (-1 = use marker). |
| `A+AttackStart` | int(0..158760000) | default 0 | First sample frame to play. |
| `A+ReleaseEnd` | int(-1..158760000) | default -1 | Last sample frame of the embedded release (-1 = end of file). |
| `A+LoopCount` | int(0..100) | default 0 | Number of explicit loops (0 = use the wav's own loop markers). |
| `A+Loop%03dStart` | int(0..158760000) | default 0 | Loop start frame. |
| `A+Loop%03dEnd` | int(LoopStart+1..158760000) | **required** | Loop end frame. |
| `A+LoopCrossfadeLength` | int(0..3000) | default 0 | Loop crossfade length (see Part 4 for unit discussion). |
| `A+ReleaseCrossfadeLength` | int(0..3000) | default 0 | Release crossfade length, in **ms**; only read when `LoadRelease` is true (otherwise 0). |

The attack's `percussive` flag is the pipe's effective `Percussive` value. `MAX_SAMPLE_LENGTH` = 158760000 (`src/core/go_limits.h`).

### Per-release keys — `GOSoundingPipe::LoadReleaseFileInfo`

Release prefix `R` = `P + Release%03d`, 001..ReleaseCount:

| Key | Type | Req/Default |
|---|---|---|
| `R` (value) | file path | required |
| `R+IsTremulant` | int(-1..1) | default -1 |
| `R+MaxKeyPressTime` | int(-1..100000) ms | default -1 (release selected by key-press duration) |
| `R+CuePoint` | int(-1..158760000) | default -1 |
| `R+ReleaseEnd` | int(-1..158760000) | default -1 |
| `R+ReleaseCrossfadeLength` | int(0..3000) ms | default 0 |

## Pipe-config inheritance (GOPipeConfig / GOPipeConfigNode)

Sources: `pipe-config/GOPipeConfig.cpp`, `GOPipeConfigNode.cpp`. Nodes form a tree: **[Organ] (root) → [WindchestGroup###] → rank ([Rank###] or stop section) → pipe (`Pipe%03d` prefix)**. Every node reads the same ODF keys, `prefix` being empty except at pipe level:

| Key (with node prefix) | Type | Default | Stored as |
|---|---|---|---|
| `AmplitudeLevel` | float(0..1000) | 100 | percent amplitude |
| `Gain` | float(-120..40) | 0 | dB |
| `PitchTuning` | float(-1800..1800) | 0 | cents (tuning used when *not* auto-retuning) |
| `PitchCorrection` | float(-1800..1800) | 0 | cents (correction applied when auto-retuning to a temperament) |
| `TrackerDelay` | int(0..10000) | 0 | ms |
| `Percussive` | bool3 | inherit (root default N) | one-shot samples, no looping |
| `HasIndependentRelease` | bool3 | inherit (root default N) | only read when the node's effective `Percussive` is true; releases play independently of the attack |

(Other members of the same structure — `Amplitude`, `UserGain`, `ManualTuning`, `AutoTuningCorrection`, `Delay`, `ReleaseTail`, `ToneBalance`, `BitsPerSample`, `Channels`, `LoopLoad`, `Compress`, `AttackLoad`, `ReleaseLoad`, `IgnorePitch`, `AudioGroup` — are read **only from the user's .cmb settings**, not from the ODF; for a pure ODF import `Amplitude=AmplitudeLevel`, `UserGain=Gain`, and the rest are 0/unset.)

**Effective-value formulas** (`GOPipeConfigNode`):

- `EffectiveAmplitude(node) = (AmplitudeLevel(node)/100) × EffectiveAmplitude(parent)`; root: `AmplitudeLevel/100`. I.e. the **product** of `AmplitudeLevel_i/100` over pipe, rank, windchest, organ.
- `EffectiveGain = Σ Gain_i` (dB, summed over the chain).
- `EffectivePitchTuning = Σ PitchTuning_i` (cents); `EffectivePitchCorrection = Σ PitchCorrection_i` (cents); `EffectiveDelay = Σ TrackerDelay_i` (ms).
- `Percussive` / `HasIndependentRelease`: nearest node with an explicit `Y`/`N` wins; if absent everywhere, false.

**Amplitude application** (`GOSoundProviderWave::SetAmplitude`): linear playback gain = `EffectiveAmplitude × 10^(EffectiveGain/20)` (1.0 = nominal), further multiplied by the velocity volume (see Rank) and the windchest volume (see Enclosure).

**Pitch application** (`GOSoundingPipe`): playback speed ratio = `2^(cents/1200)` where, for the "Original temperament" (default), `cents = EffectivePitchTuning` (plus user tunings, 0 for pure ODF); for any other temperament and if `AcceptsRetuning`, `cents = autoOffset + EffectivePitchCorrection + temperamentOffset(midiKey mod 12)` with
`autoOffset = 1200·log2(HarmonicNumber/8) + 100·(pipeMidiKey − sampleMidiKey) − sampleMidiPitchFraction_cents`,
where `pipeMidiKey = rank FirstMidiNoteNumber + pipeIndex`, and `sampleMidiKey`/`sampleMidiPitchFraction` come from `MIDIKeyNumber`/`MIDIPitchFraction` overrides or from the wav's `smpl` chunk. If retuning would exceed ±1800 cents it is rejected.

## Coupler ([Coupler001]…)

Source: `GOCoupler.cpp`. In addition to button + drawstop keys:

| Key | Type | Req/Default | Meaning |
|---|---|---|---|
| `UnisonOff` | bool | **required** | Unison-off coupler: engaging suppresses the source manual's *own* (non-coupled) keys from sounding its stops. All destination keys below are then ignored. |
| `DestinationManual` | int(firstManualIndex..manualCount) | required iff UnisonOff=N | Target ODF manual index (pedal = 0). |
| `DestinationKeyshift` | int(-24..24) | required iff UnisonOff=N | Shift in keys (semitones). |
| `CouplerType` | enum `Normal`,`Bass`,`Melody` | default `Normal` | `Bass` couples only the lowest pressed key; `Melody` only the highest. |
| `CoupleToSubsequentUnisonIntermanualCouplers` | bool | **required** for Normal non-unison-off couplers; ignored for UnisonOff/Bass/Melody | Recursion into destination's couplers with keyshift 0. |
| `CoupleToSubsequentUpwardIntermanualCouplers` | bool | ditto | …into intermanual couplers with keyshift > 0. |
| `CoupleToSubsequentDownwardIntermanualCouplers` | bool | ditto | …keyshift < 0. |
| `CoupleToSubsequentUpwardIntramanualCouplers` | bool | ditto | …same-manual couplers with keyshift > 0. |
| `CoupleToSubsequentDownwardIntramanualCouplers` | bool | ditto | …keyshift < 0. |
| `FirstMIDINoteNumber` | int(0..127) | default 0 | Lowest source MIDI note the coupler acts on (0 = from the manual's first logical key). |
| `NumberOfKeys` | int(0..127) | default 127 | Number of source keys acted on. |

Runtime semantics (`GOCoupler::SetKey`, `ChangeKey`): the effective key offset is `DestinationKeyshift + srcFirstLogicalKeyMIDI − destFirstLogicalKeyMIDI` applied to 0-based logical key indices. A key event arriving at a manual via coupler *X* is propagated onward through coupler *Y* on that manual only if *X*'s `CoupleToSubsequent…` flag matching *Y*'s class (unison/upward/downward × inter/intramanual, judged by *Y*'s `DestinationKeyshift` sign and whether *Y* is intermanual) is set; direct (keyboard) input always propagates. Each hop decrements the internal velocity by 1. Bass/Melody couplers re-evaluate the lowest/highest pressed key on every change and couple only that note. Intermanual = source manual ≠ destination manual.

## Tremulant ([Tremulant001]…)

Source: `GOTremulant.cpp`. In addition to button + drawstop keys (a tremulant is a drawstop):

| Key | Type | Req/Default | Meaning |
|---|---|---|---|
| `TremulantType` | enum `Synth`,`Wave` | default `Synth` | |
| `Period` | int(32..441000) ms | required (Synth only) | Length of one modulation cycle. |
| `StartRate` | int(1..100) | required (Synth only) | Startup speed. |
| `StopRate` | int(1..100) | required (Synth only) | Stop speed. |
| `AmpModDepth` | int(1..100) | required (Synth only) | Amplitude modulation depth, percent. |

`Synth` tremulants generate a modulating sampler on every windchest that lists the tremulant. `Wave` tremulants read none of the four parameters; engaging/disengaging one switches every pipe of each windchest that lists the tremulant (windchest `Tremulant%03d`) between its samples marked `IsTremulant=0` and `IsTremulant=1` (`GOWindchest::UpdateTremulant`, `GOSoundingPipe::SetWaveTremulant`). Pipes intended for wave tremulants must therefore supply attack/release samples for both states.

## Windchest ([WindchestGroup001]…)

Source: `GOWindchest.cpp`.

| Key | Type | Req/Default | Meaning |
|---|---|---|---|
| `Name` | string (non-empty) | default "Windchest N" | |
| `NumberOfEnclosures` | int(0..organ enclosure count) | required | |
| `Enclosure%03d` | int(1..organ enclosure count) | required | Global enclosure number affecting this windchest. |
| `NumberOfTremulants` | int(0..organ tremulant count) | required | |
| `Tremulant%03d` | int(1..organ tremulant count) | required | Global tremulant number affecting this windchest. |
| pipe-config keys (no prefix) | | | `AmplitudeLevel`, `Gain`, `PitchTuning`, `PitchCorrection`, `TrackerDelay`, `Percussive`, `HasIndependentRelease` — parent node for its ranks' pipe configs. |

Windchest volume = product of the attenuations of all its enclosures (`GOWindchest::UpdateVolume`).

## Enclosure ([Enclosure001]…)

Source: `GOEnclosure.cpp`.

| Key | Type | Req/Default | Meaning |
|---|---|---|---|
| `Name` | string (non-empty) | required | |
| `AmpMinimumLevel` | int(0..100) | required | Volume percent when fully closed. |
| `MIDIInputNumber` | int(0..200) | default -1 (unset) | Initial-MIDI-config selector, like manuals. |
| `Displayed` | bool | default Y (legacy GUI) / N (new GUI format) | Read twice with the two defaults; GUI only. |

Attenuation for enclosure value `v` (0..127, initial 127): `(v·(100 − AmpMinimumLevel) + 127·AmpMinimumLevel) / 12700`, i.e. linear from `AmpMinimumLevel%` at closed to 100% at open (`GOEnclosure::GetAttenuation`).

## Divisional coupler ([DivisionalCoupler001]…)

Source: `GODivisionalCoupler.cpp`. Button + drawstop keys plus:

| Key | Type | Req/Default | Meaning |
|---|---|---|---|
| `BiDirectionalCoupling` | bool | required | If `N`, pressing a divisional on a listed manual also fires the same-numbered divisional only on manuals listed *after* it; if `Y`, on all other listed manuals. |
| `NumberOfManuals` | int(1..manualCount−firstManualIndex+1) | required | |
| `Manual%03d` | int(firstManualIndex..manualCount) | required | ODF manual indices participating. |

When engaged, coupling is applied transitively across engaged divisional couplers (with cycle protection) (`GOOrganModel::GetCoupledManualsForDivisional`).

## Divisional ([Divisional001]…, referenced from a manual)

Sources: `GODivisionalButtonControl.cpp`, `GODivisionalCombination.cpp`, `GOCombination.cpp`. A divisional is a **pushbutton** (common button keys: `Name` required, `Displayed`, `DisplayInInvertedState`, `ShortcutKey`) — *not* a drawstop (no `Function`/`DefaultToEngaged`). Combination content keys:

| Key | Type | Req/Default | Meaning |
|---|---|---|---|
| `Protected` | bool | default N | Prevents the user from overwriting the combination. |
| `NumberOfStops` | int(0..manual stop count) | **required** | Presence of this key is also how GO detects that an ODF combination exists at all (`isCmbOnFile`). |
| `NumberOfCouplers` | int(0..manual ODF coupler count) | required iff `DivisionalsStoreIntermanualCouplers` or `DivisionalsStoreIntramanualCouplers`; else default 0 | |
| `NumberOfTremulants` | int(0..organ tremulant count) | required iff `DivisionalsStoreTremulants`; else default 0 | |
| `NumberOfSwitches` | int(0..organ switch count) | default 0 | |
| `IsFull` | bool | default N | Store/recall every element (rarely used in ODFs). |
| `HasScope` | bool | default N | Scope semantics (rarely used in ODFs). |
| `Stop%03d` (001..NumberOfStops) | int(−N..N), N = manual stop count | required | abs(value) = stop number **within the owning manual** (1-based); value > 0 = engage, value < 0 = disengage when the divisional is pushed. Duplicates or unknown numbers are logged errors. |
| `Coupler%03d` | int(−N..N), N = manual ODF coupler count | required | Same convention, coupler number within the manual. |
| `Tremulant%03d` | int(−N..N), N = manual tremulant count | required | Number within the manual's `Tremulant%03d` list. |
| `Switch%03d` | int(−N..N), N = manual switch count | required | Number within the manual's `Switch%03d` list. |

The divisional's element universe is exactly the owning manual's stops, couplers, manual tremulants and manual switches (`GOCombinationDefinition::InitDivisional`); elements not mentioned in the ODF combination are left untouched only if they are not "stored unconditionally" (see `StoreInDivisional` defaults), otherwise recalled as off.


---

# Part 4 — Audio Sample Loading (WAV/WavPack, Attacks, Releases, Loops, Pitch, Amplitude)

This section documents how GrandOrgue (GO) locates, parses and interprets the pipe sample files referenced from an ODF (`.organ` file), based on the GO source (`src/core/GOWave.*`, `GOWaveTypes.h`, `GOWavPack.*`, `src/grandorgue/model/GOSoundingPipe.cpp`, `src/grandorgue/sound/providers/GOSoundProviderWave.*`, `src/grandorgue/model/pipe-config/GOPipeConfig*`, `src/grandorgue/loader/GOLoaderFilename.cpp`).

## Pipe entries in the ODF

Within a stop or rank section, each pipe is declared as `Pipe999` (1-based, zero-padded to 3 digits, e.g. `Pipe001`), for `NumberOfLogicalPipes` pipes (1–192). The value of `Pipe999` is one of (`GORank.cpp`):

- `DUMMY` — a silent pipe; no sample is loaded, all other `Pipe999...` keys are ignored (`GODummyPipe`).
- `REF:m:s:p` — a reference to pipe `p` of stop `s` of manual `m` (colon-separated decimal numbers). No sample is loaded; playing this pipe plays the referenced pipe. Validation: the manual index must be an existing ODF manual, `1 <= s <= stop count of that manual`, `1 <= p <= pipe count of that stop's first rank` (`GOReferencePipe.cpp`).
- Otherwise — a relative path to the *main sample file* of a sounding pipe.

The MIDI key number of pipe *i* (0-based) is `FirstMidiNoteNumber + i` where `FirstMidiNoteNumber` is a rank-level key (0–256; for a stop's built-in rank it is derived from the manual's first MIDI note and `FirstAccessiblePipeLogicalPipeNumber`).

All `Pipe999`-prefixed keys below also exist for additional attacks with prefix `Pipe999Attack999` and for additional releases with prefix `Pipe999Release999` where noted.

## Sample file path resolution (`GOLoaderFilename.cpp`, `GOConfigReader.cpp`)

- The value is read as a string; leading/trailing whitespace is trimmed (with a warning in strict mode). An empty value is treated as missing.
- Paths are **relative to the directory containing the ODF** (or to the root of the organ package archive if the organ is loaded from a `.orgue` package). If the normalized path is absolute, it is used as-is (discouraged).
- The path is normalized for `.` / `..` segments (`wxPATH_NORM_DOTS`).
- The conventional (Hauptwerk-1 compatible) directory separator is backslash `\`. When loading from the plain filesystem, every `\` is replaced by the platform's native separator (`generateFullPath`). Forward slashes `/` in an ODF trigger a compatibility warning ("non-portable directory separator") but work on the filesystem on all platforms. Inside package archives, entry names are normalized to `\` separators and matched **exactly** (case-sensitively), so ODF paths in packaged organs must use `\` and exact case.
- **Case sensitivity:** GO performs no case-folding fallback. On case-insensitive filesystems (Windows/macOS) a wrong-case path loads anyway; GO emits a warning ("not compatible with case sensitive systems") when the given name differs from the on-disk name. An importer that wants to accept real-world sample sets should match path components case-insensitively.
- **File format is detected by content, not extension.** A referenced file (whatever its extension, typically `.wav` or `.wv`) is loaded as WavPack if its first 4 bytes are `wvpk`, otherwise as RIFF/WAVE. There is no automatic extension substitution (no probing for `file.wav.wv` etc.).
- A missing file is a load error ("File '%s' does not exist").

## WAV file parsing (`GOWave.cpp`, `GOWaveTypes.h`)

The file must begin with `RIFF` (4CC) + 32-bit LE size + `WAVE`. Chunks are then scanned sequentially; each chunk is 4CC + 32-bit LE size + payload, padded to 16-bit word alignment (the pad byte may be omitted at end-of-file). If the on-disk file is larger than `RIFF size + 8`, the extra bytes are ignored; if the scan does not end exactly at the declared length, the file is rejected ("Invalid WAV file"). Only these chunks are read — **everything else (`LIST`, `fact`, `bext`, …) is skipped**:

- **`fmt ` (required, must precede `data`):** `wFormatTag` must be 1 (integer PCM) or 3 (IEEE float), otherwise error. `wBitsPerSample` must be a multiple of 8. For PCM: 8, 16 or 24 bits (8-bit is *unsigned* with 0x80 bias; 16/24-bit are signed little-endian). For float: only 32-bit. Channel count and sample rate are taken as-is (any values); at pipe-load time only 1 or 2 output channels are supported (see below). A wave "frame"/"block" = one sample per channel.
- **`data` (required):** raw sample data; its length must be a whole number of frames. If absent or empty → error "No samples found".
- **`smpl` (optional):** see next section.
- **`cue ` (optional):** release marker. GO reads `dwCuePoints` and the cue-point array; the release marker position is the **maximum `dwSampleOffset`** over all cue points, interpreted as a **frame offset** into the data chunk. Any cue chunk with ≥ 1 point sets "has release marker".

Missing `smpl` chunk ⇒ no loops, MIDI note = 0, pitch fraction = 0 (i.e. "no pitch information"). Missing `cue ` chunk ⇒ no release marker.

### `smpl` chunk

Layout (all 32-bit LE): `dwManufacturer, dwProduct, dwSamplePeriod, dwMIDIUnityNote, dwMIDIPitchFraction, dwSMPTEFormat, dwSMPTEOffset, cSampleLoops, cbSamplerData`, followed by `cSampleLoops` loop records `dwIdentifier, dwType, dwStart, dwEnd, dwFraction, dwPlayCount`.

- **Loops:** `dwStart`/`dwEnd` are frame offsets; `dwEnd` is the **last frame of the loop, inclusive** (playback wraps after `dwEnd`; GO internally uses `dwEnd + 1` as the exclusive end). `dwType`, `dwFraction`, `dwPlayCount` are ignored. On load each loop is validated and silently dropped (with a log message) if `start >= end`, `start >= frameCount`, `end >= frameCount`, or `end == 0`.
- **MIDI note:** `dwMIDIUnityNote` used verbatim (expected 0–127; 0 effectively means "unknown").
- **Pitch fraction:** converted to **cents** as `pitchFractCents = dwMIDIPitchFraction / 4294967295.0 * 100.0` (i.e. the full 32-bit range maps to one semitone = 100 cents).

## WavPack files (`GOWavPack.cpp`)

Files starting with the `wvpk` magic are decoded with libwavpack (lossless mode; GO's own package/cache writer produces them). GO extracts:

- the decoded samples (always delivered as 32-bit integers per channel; for 8/16-bit originals the significant value occupies the low bytes and GO shifts `<<16` / `<<8` respectively when reading; 24-bit used as-is; 32-bit float passthrough),
- the stored **RIFF wrapper** (the original WAV header and trailer around the removed `data` payload).

The wrapper is then parsed with exactly the same chunk logic as a plain WAV file, so `fmt `, `smpl` and `cue ` metadata (loops, MIDI note, pitch fraction, release marker) survive WavPack compression unchanged. The frame count is `decodedBytes / (4 * channels)`.

## Attack and release sample assembly (`GOSoundingPipe.cpp`, `GOSoundProviderWave.cpp`)

A sounding pipe has an ordered list of **attacks** and **releases**:

1. **Implicit attack:** the main sample (`Pipe999=path`) is always attack #0. All per-attack keys are read directly with the `Pipe999` prefix (i.e. `Pipe999LoadRelease`, `Pipe999LoopCount`, `Pipe999AttackStart`, …).
2. **Additional attacks:** `Pipe999AttackCount` (integer 0–100, default 0); files `Pipe999Attack001` … each with their own per-attack keys (`Pipe999Attack001LoopCount` etc.).
3. **Additional releases:** `Pipe999ReleaseCount` (integer 0–100, default 0); files `Pipe999Release001` … with per-release keys.

Per-**attack** keys (prefix `P` = `Pipe999` or `Pipe999Attack999`):

| Key | Type / range | Default | Meaning |
|---|---|---|---|
| `P` | filename | required | sample file |
| `PIsTremulant` | int −1/0/1 | −1 | wave-tremulant sample group (see below) |
| `PLoadRelease` | Y/N | `!Percussive` | take the release segment from this file |
| `PAttackVelocity` | int 0–127 | 0 | minimum note-on velocity for this attack |
| `PMaxTimeSinceLastRelease` | int −1–100000 | −1 | ms; attack only used if the time since the pipe was last released ≤ this (−1 = ∞) |
| `PMaxKeyPressTime` | int −1–100000 | −1 | ms; applies to the release taken from this file (−1 = ∞) |
| `PAttackStart` | int 0–158760000 | 0 | frame offset where attack playback starts |
| `PCuePoint` | int −1–158760000 | −1 | overrides the wav release marker (frame offset) |
| `PReleaseEnd` | int −1–158760000 | −1 | last frame (exclusive end) of the release segment |
| `PLoopCount`, `PLoop999Start/End` | see below | 0 | ODF loop override |
| `PLoopCrossfadeLength` | int 0–3000 | 0 | ms |
| `PReleaseCrossfadeLength` | int 0–3000 | 0 | ms; only read when the release is loaded from this file |

Per-**release** keys (`R` = `Pipe999Release999`): `R` (filename), `RIsTremulant` (−1/0/1, default −1), `RMaxKeyPressTime` (−1–100000, default −1), `RCuePoint` (−1–158760000, default −1), `RReleaseEnd` (−1–158760000, default −1), `RReleaseCrossfadeLength` (0–3000 ms, default 0).

(158760000 = `MAX_SAMPLE_LENGTH`, `go_limits.h`.)

### Channel / bit-depth handling at load

The wave's samples are converted to the user-configured working format (bits per sample 8–24, capped at the file's own bit depth; channels: 2 = as recorded, 1 = downmix all channels by averaging). Waves with more than 2 channels are only usable when downmixed to mono; otherwise "Unsupported channel count". These are user settings, **not** ODF data.

### Release creation rules (`LoadFromOneFile`, `AddReleaseSection`)

- From an **attack file**, a release section is created only if **all** of: `LoadRelease` is true (default: true unless percussive), the wav itself contains **≥ 1 `smpl` loop**, the wav itself contains a **cue-chunk release marker**, and the pipe is not percussive. (An ODF `CuePoint` alone does *not* enable it — the wav must have the cue chunk; `CuePoint` merely overrides the position.)
- From a **separate release file**, a release section is always created.
- Release segment start frame = wav release marker (max cue `dwSampleOffset`) if present, else 0; overridden by ODF `CuePoint` if ≥ 0. Release segment end = file frame count, overridden by `ReleaseEnd` if ≥ 0. Validation: `releaseEnd <= frameCount` ("Invalid release end position") and `start < end` ("Invalid release position"). `CuePoint`/`ReleaseEnd` are **absolute frame offsets in the file**, unaffected by `AttackStart`.

### AttackStart

The attack section's PCM data starts `AttackStart` frames into the file. Loops (from either source) must start at or after `AttackStart`; loop positions are stored relative to the shifted start (`start -= AttackStart`, `end -= AttackStart`). Under `LoopLoad=All`, loops starting before `AttackStart` are excluded; if that leaves none, it is an error ("No loop found").

### Percussive pipes and independent releases

`Percussive` (Y/N triple-state) is inherited: pipe → rank/stop → windchest group → `[Organ]`; ultimate default N. If effectively percussive:

- loops are **not** loaded (even if present in the wav or ODF) — the attack plays once to its end (one-shot); note loop *definitions* are still validated and can still raise errors,
- `LoadRelease` defaults to N, so no implicit release,
- `HasIndependentRelease` (Y/N triple-state, read only for percussive levels, inherited the same way, default N): when Y, the pipe should provide `Pipe999ReleaseCount ≥ 1` separate releases (warning otherwise); at key-release, while the one-shot attack keeps playing to its end, the release sample is additionally started. Non-percussive pipes must have a release; percussive pipes without independent release that *do* have releases get a "percussive sample with a release" warning.
- Mixing one-shot (loop-less) and looped attacks in one pipe is a load error.

### Runtime selection (what must be preserved)

Each attack carries (`AttackSelector`): `min_attack_velocity` (`AttackVelocity`), `max_released_time` (`MaxTimeSinceLastRelease`, ms, −1→∞) and its `IsTremulant` group. Each release carries (`ReleaseSelector`): `max_playback_time` (`MaxKeyPressTime`, ms, −1→∞) and `IsTremulant`. At note-on GO picks randomly among attacks whose tremulant group matches, whose `AttackVelocity <= velocity` and `MaxTimeSinceLastRelease >= time-since-last-release`; at note-off it picks the release with the smallest `MaxKeyPressTime` still ≥ the actual key-hold time (ms) in the matching tremulant group. `IsTremulant` values: −1 = usable for any wave-tremulant state (default), 0 = only when the wave tremulant is off, 1 = only when on ("wave tremulant" = a tremulant with `TremulantType=Wave` switching between differently-recorded sample groups).

## ODF loops and validation

`PLoopCount` (0–100, default 0) with `PLoop001Start` … `PLoop999Start` (0–158760000, default 0) and `PLoop999End` (**required** when the loop exists; range `Start+1`–158760000). **If at least one ODF loop is given for an attack, the wav's `smpl` loops are ignored entirely for that attack; otherwise the wav loops are used** (`AddAttackSection`). Both use the same semantics: `End` = last frame of the loop, inclusive.

Validation at load (error, aborts organ load): `start >= frameCount`, `start >= end`, or `end >= frameCount` ⇒ "Invalid loop definition". With a loop crossfade, additionally `start + 256 + crossfade > end − 1` ⇒ "Loop too short for a cross fade" (256 = `REMAINING_AFTER_CROSSFADE`).

Which loops are actually kept depends on the user (not ODF) setting `LoopLoad`: 0 = first loop with the earliest end point, 1 = longest loop, 2 = all loops (GO default). An importer should preserve all loops.

## Crossfade lengths

- **`LoopCrossfadeLength`** (0–3000, **milliseconds**, default 0): a crossfade of `n_samples = ms * sampleRate / 1000` frames (`GOSoundAudioSection.cpp:331`) is rendered into the loop-end region at load time, blending loop-end material into the material preceding the loop start using an equal-power-style cosine curve: `out[i] = loopEnd[i] * f + preLoopStart[i] * (1−f)`, `f = (cos(π·(i+0.5)/N)+1)/2` (`GOSoundAudioSection::DoCrossfade`). Requires ≥ `n_samples` frames *before* the loop start and a loop at least `n_samples+1` frames long, otherwise the loop is ignored with a warning. Not inherited by additional attack files (each `Pipe999Attack999LoopCrossfadeLength` is independent). **Unit caveat:** GO itself is inconsistent — the parameter is annotated "in samples" (`GOSoundProviderWave.cpp:50`) and the early "Loop too short for a cross fade" validation (`GOSoundProviderWave.cpp:71–75`) compares the raw ODF value directly against frame positions, but the actual crossfade rendering converts it as milliseconds (× sampleRate / 1000). Treat the value as milliseconds for behavior; expect the validation to be stricter than necessary at high sample rates.
- **`ReleaseCrossfadeLength`** (0–3000, **milliseconds**, default 0 = automatic): the fade-out/fade-in time used when transitioning from the sustained sound into a release sample (and when retriggering/switching attacks). 0 selects an automatic length based on the pipe's sample MIDI note (`get_fader_length`, `GOSoundProviderWave.cpp`): 184 ms for notes < 42, 6 ms for notes > 86, linear in between (`184 − (note−42)/44·178` ms), 46 ms if the note is 0 or ≥ 133. The value is stored per release section; per-attack and per-release values are independent (not inherited from `Pipe999ReleaseCrossfadeLength`).

## Pitch computation (`GOSoundingPipe.cpp`, `GOPipeConfig*.cpp`)

Inputs per pipe:

- `Pipe999MIDIKeyNumber` (int −1–127, default −1 = unset): overrides the sample's `smpl` MIDI note.
- `Pipe999MIDIPitchFraction` (float 0–100, **cents**, default −1 = unset): overrides the sample's pitch fraction. If `MIDIKeyNumber` is set but `MIDIPitchFraction` is not, the fraction is forced to **0** (the sample's own fraction is ignored). If neither is set, both come from the **first attack file's** `smpl` chunk (note 0 / fraction 0 when the chunk is absent).
- `HarmonicNumber` (int 1–1024): rank-level default 8; `Pipe999HarmonicNumber` overrides per pipe. 8 = unison (8′ pitch); a 4′ rank has 16, 16′ has 4, 2⅔′ has 24, etc. (`HarmonicNumber = 64 / footage`).
- `PitchTuning` (float −1800–1800 cents, default 0) and `PitchCorrection` (float −1800–1800 cents, default 0): exist at pipe, rank/stop, windchest-group and `[Organ]` level and are **summed over the whole hierarchy** (`GetEffectiveSum`).
- `AcceptsRetuning` (Y/N, rank default Y, pipe override): enables temperament retuning of this pipe.

Two tuning modes (`UpdateTuning`):

- **"Original temperament"** (GO default): `offset_cents = Σ PitchTuning + userManualTuning`. The sample is played as recorded apart from explicit tuning offsets; sample pitch metadata is not used.
- **Any other temperament:** the pipe is auto-retuned to equal temperament (plus the temperament's per-semitone offset). If `IgnorePitch` (user setting) is off and the sample MIDI key ≠ 0:

```
offset_cents = 1200 * log2(HarmonicNumber / 8)          // harmonic correction
             + 100  * (pipeMidiKey - sampleMidiKey)      // note correction
             - samplePitchFractionCents                  // fraction correction
             + Σ PitchCorrection                         // ODF, hierarchy sum
             + userAutoTuningCorrection
             + temperamentOffset(pipeMidiKey mod 12)     // 0 if AcceptsRetuning=N
```

Equivalently: `offset = targetPitchCents − samplePitchCents`, with `target = 100·pipeMidiKey + 1200·log2(H/8)` and `sample = 100·sampleMidiKey + fractionCents`. `pipeMidiKey` is the pipe's keyboard position (`FirstMidiNoteNumber + index`).

The playback rate/frequency factor is `factor = 2^(offset_cents / 1200)` (`GOSoundProvider::SetTuning`: `m_Tuning = (2^(1/1200))^cents`); the sample is resampled by this factor. Validation warnings: no pitch info while retunable (sample key ≤ 0 and fraction ≤ 0); |auto−manual offset| > 600 cents (warning) or > 1800 cents (error, pipe not retuned).

## Amplitude (`GOSoundProviderWave::SetAmplitude`, `GOPipeConfigNode`)

- `AmplitudeLevel` (float 0–1000, percent, default 100) — multiplicative across the hierarchy: `effAmplitude = Π(level_i / 100)` over pipe → rank/stop → windchest group → `[Organ]`.
- `Gain` (float −120–40, dB, default 0) — additive across the same hierarchy: `effGain = Σ gain_i`.

Final linear amplitude scalar applied to the normalized sample (integer samples divided by 2^(bits−1)):

```
amplitude = effAmplitude * 10^(effGain / 20)
```

(so ODF `Gain` and `AmplitudeLevel` are interchangeable via `gain_dB = 20·log10(level/100)`.) Additionally, note-on velocity scales volume linearly: `v = MinVelocityVolume/100 + velocity · (MaxVelocityVolume − MinVelocityVolume)/12700` (`MinVelocityVolume`/`MaxVelocityVolume`: floats 0–1000, group-level keys, default 100, i.e. velocity-insensitive when equal).

## Sample cache (informational only)

GO can write a `.cache` file per organ (in its cache directory, named from a hash of the organ/ODF identity plus preset number) containing the fully converted in-memory audio sections (`GOSoundProvider::SaveCache`, `GOCache.cpp`), optionally zlib-compressed, prefixed with a magic number and a hash covering all load-relevant parameters (file names, bits per sample, channels, loop/attack/release load modes, all attack/release/loop/crossfade parameters and internal struct layouts). On the next load the cache is used instead of re-decoding the WAV/WavPack files and is invalidated whenever the hash mismatches. It is a pure load-time optimization derived entirely from the ODF and sample files; an importer does not need to read or produce it.


---

# Part 5 — Visual Console / Panel Rendering

Derived from GrandOrgue sources (`src/grandorgue/gui/panels/*`, `src/core/config/GOConfigReader.cpp`, `src/grandorgue/gui/GOGuiImageCache.cpp`, `src/images/*`) and cross-checked against `help/grandorgue.xml`. Coordinates are in a virtual pixel space of `DispScreenSizeHoriz × DispScreenSizeVert`; GrandOrgue renders the panel at this size and scales the finished bitmap to the window.

## 1. Shared value types

| Type | Syntax / semantics |
|---|---|
| boolean | `Y` = true, `N` = false (case-insensitive; any value starting with Y/N is tolerated with a warning) |
| colour | One of (case-insensitive): `BLACK, BLUE, DARK BLUE, GREEN, DARK GREEN, CYAN, DARK CYAN, RED, DARK RED, MAGENTA, DARK MAGENTA, YELLOW, DARK YELLOW, LIGHT GREY, DARK GREY, WHITE, BROWN`, or HTML `#RRGGBB` (hex digits must be uppercase `0-9A-F`; see Part 1 §3.9 for GO's decimal-weighting parsing quirk) |
| font size | `SMALL` = 6 pt, `NORMAL` = 7 pt, `LARGE` = 10 pt, or an integer 1–50 (points) |
| panel size | `SMALL/MEDIUM/MEDIUM LARGE/LARGE` or an integer number of pixels 100–32000. Horizontal: SMALL=800, MEDIUM=1007, MEDIUM LARGE=1263, LARGE=1583. Vertical: SMALL=500, MEDIUM=663, MEDIUM LARGE=855, LARGE=1095 (`GOConfigReader::ReadSize`) |
| wood bitmap number | Integer 1–64, a built-in wood texture. Odd number = vertical grain, the following even number = the same texture rotated 90° (horizontal grain). Built-in files are 256×256 JPEGs, tiled |
| image file path | Relative to the ODF location (or inside the organ package archive). `\` or `/` accepted as separator. Supported formats: bmp, gif, jpg, ico, png. (Internally, built-in bitmaps are cached under names `../GO:<name>`; an ODF path exactly equal to `../GO:<name>` — forward slash — resolves to the built-in bitmap. This is an implementation artifact, not documented behaviour.) |
| numeric range violations | For ODF values, out-of-range integers/floats are a fatal error; missing required keys are fatal |

Text on controls is word-wrapped to `TextBreakWidth` pixels and drawn horizontally centred in the text rectangle; vertically centred for buttons/labels, top-aligned for enclosures (`GODC::DrawText`).

## 2. Panels — old vs. new format

`[Organ]` key `NumberOfPanels` (integer 0–100, default 0) declares additional panels `[Panel001]…[Panel100]`. The main panel always exists.

**New-format detection** (`GOGUIPanel::Load`): if `[Panel000] NumberOfGUIElements` is present (value 0–999), the ODF uses the *new panel format*. Otherwise old format.

### 2.1 Main panel
* Old format: display metrics and panel content keys live directly in `[Organ]`. Panel name = `ChurchName`.
* New format: display metrics live in `[Panel000]`; `Name` (string, optional, default `ChurchName`) may override the name. GUI elements are `[Panel000Element001]…`; images are `[Panel000Image001]…` counted by `[Panel000] NumberOfImages`.
* In **both** formats the main panel automatically shows every model object whose own section has `Displayed=Y` (see 2.2); the GUI attributes are then read **from the object's own model section** (e.g. `[Stop003]`, `[Tremulant001]`, `[Manual001]`, `[Enclosure002]`).

Per-panel keys (all panels):

| Key | Type | Default | Notes |
|---|---|---|---|
| `Name` | string | — | required, non-empty for `[Panel001+]`; optional in `[Panel000]` |
| `Group` | string | empty | `[Panel001+]` only; submenu name in the Panel menu |
| `HasPedals` | boolean | required | if `N`, layout manual numbering starts at 1 (slot 0 left empty); if `Y`, manual 0 is drawn as a pedalboard |
| `NumberOfImages` | int 0–999 | 0 | sections `[Image999]` (old main), `[Panel999Image999]` (all others) |

### 2.2 Automatic main-panel population (both formats)
For each object type, if the model object's section contains `Displayed=Y` (boolean, default N — for enclosures default Y in old format, N in new format; `GOEnclosure::Load`), a GUI control is created on the main panel, reading its GUI keys from that same section:
* Enclosures `[Enclosure999]`
* Tremulants `[Tremulant999]`, Divisional couplers `[DivisionalCoupler999]`, Switches `[Switch999]` → drawn as drawstops by default
* Generals `[General999]`, Reversible pistons `[ReversiblePiston999]` → drawn as pistons by default
* Manuals `[Manual000…]` (`Displayed=Y`) → manual background + keyboard
* Per manual: stops/couplers/divisionals referenced by `[ManualNNN]` keys `Stop001…`, `Coupler001…`, `Divisional001…` (values 1–999 = global object number); GUI attrs in the global object section (`[Stop023]` etc.). Divisionals default to piston rendering. Buttons with `Displayed=N` are skipped.

Old-format main panel additionally reads from `[Organ]`:
* `NumberOfSetterElements` (0–999, default 0) → `[SetterElement999]`, each with `Type=` (see §11)
* `NumberOfLabels` (0–999, required) → `[Label999]`

### 2.3 Old-format additional panels `[PanelNNN]`
All display-metric keys (§3) must be repeated in the panel section. Content keys in `[PanelNNN]`:

| Key(s) | Meaning | GUI-attribute section |
|---|---|---|
| `NumberOfEnclosures` (req, 0–enclosure count), `Enclosure999 = k` | reference to global enclosure k | `[PanelNNNEnclosurekkk]` (referenced number!) |
| `NumberOfTremulants` (req), `Tremulant999 = k` | global tremulant k | `[PanelNNNTremulantkkk]` |
| `NumberOfDivisionalCouplers` (req), `DivisionalCoupler999 = k` | | `[PanelNNNDivisionalCouplerkkk]` |
| `NumberOfGenerals` (req), `General999 = k` | piston by default | `[PanelNNNGeneralkkk]` |
| `NumberOfReversiblePistons` (req), `ReversiblePiston999 = k` | piston by default | `[PanelNNNReversiblePistonkkk]` |
| `NumberOfSwitches` (opt, default 0), `Switch999 = k` | | `[PanelNNNSwitchkkk]` |
| `NumberOfManuals` (req, 0–manual count), `Manual999 = k` (999 runs from 0 if `HasPedals` else 1) | manual k | `[PanelNNNManualkkk]` |
| `NumberOfCouplers` (req 0–999), `Coupler999Manual = m`, `Coupler999 = c` (sequential 999) | coupler c of manual m | `[PanelNNNCoupler999]` (sequential index) |
| `NumberOfStops` (req 0–999), `Stop999Manual`, `Stop999` | stop of a manual | `[PanelNNNStop999]` (sequential) |
| `NumberOfDivisionals` (req 0–999), `Divisional999Manual`, `Divisional999` | piston by default | `[PanelNNNDivisional999]` (sequential) |
| `NumberOfSetterElements` (opt, default 0) | see §11 | `[PanelNNNSetterElement999]` |
| `NumberOfLabels` (req) | | `[PanelNNNLabel999]` |

### 2.4 New-format panels — `[PanelNNNElementMMM]`
`NumberOfGUIElements` (0–999, default 0) counts sections `[PanelNNNElementMMM]` (MMM from 001). Each has `Type` plus reference keys plus the normal GUI attributes for its kind:

| `Type` | Reference keys | Rendered as |
|---|---|---|
| `Stop` | `Manual` (valid manual number), `Stop` (1–stop count of that manual) | drawstop |
| `Coupler` | `Manual`, `Coupler` | drawstop |
| `Divisional` | `Manual`, `Divisional` | piston |
| `Tremulant` | `Tremulant` | drawstop |
| `DivisionalCoupler` | `DivisionalCoupler` | drawstop |
| `General` | `General` | piston |
| `ReversiblePiston` | `ReversiblePiston` | piston |
| `Switch` | `Switch` | drawstop |
| `Label` | — (static text label, §6) | label |
| `Manual` | `Manual` | manual background + keyboard |
| `Enclosure` | `Enclosure` (1–enclosure count) | enclosure |
| *anything else* | treated as a setter-element name (§11) | button/label/enclosure |

## 3. Display metrics keys (per panel: `[Organ]` old-main / `[Panel000]` new-main / `[PanelNNN]`)

Source: `GOGUIHW1DisplayMetrics.cpp`. "req" = fatal error when missing.

| Key | Type / range | Default | Purpose |
|---|---|---|---|
| `DispScreenSizeHoriz` | panel size (horiz) | req | panel width in px |
| `DispScreenSizeVert` | panel size (vert) | req | panel height in px |
| `DispDrawstopBackgroundImageNum` | int 1–64 | req | wood for left+right side regions |
| `DispConsoleBackgroundImageNum` | int 1–64 | req | wood for centre region |
| `DispKeyHorizBackgroundImageNum` | int 1–64 | req | wood for horizontal piston strips |
| `DispKeyVertBackgroundImageNum` | int 1–64 | req | wood for vertical strips (manual sides, trims) |
| `DispDrawstopInsetBackgroundImageNum` | int 1–64 | req | wood for paired-column insets |
| `DispControlLabelFont` | string (font name) | req | default font for button labels |
| `DispShortcutKeyLabelFont` | string | req | parsed, **not used** by current code |
| `DispShortcutKeyLabelColour` | colour | req | parsed, **not used** |
| `DispGroupLabelFont` | string | req | default font for Label elements |
| `DispDrawstopCols` | int 2–12 | req | columns in the side jambs (split half left / half right) |
| `DispDrawstopRows` | int 1–20 | req | rows in the side jambs |
| `DispDrawstopColsOffset` | bool | req | stagger alternate columns vertically by half a drawstop |
| `DispDrawstopOuterColOffsetUp` | bool | req iff ColsOffset=Y, else default N | which alternate columns shift |
| `DispPairDrawstopCols` | bool | req | group columns in pairs (needs cols divisible by 4) |
| `DispExtraDrawstopRows` | int 0–99 | req | rows of drawstops in top centre jamb (rows 100+) |
| `DispExtraDrawstopCols` | int 0–40 | req | columns in top centre jamb |
| `DispButtonCols` | int 1–32 | req | piston columns |
| `DispExtraButtonRows` | int 0–99 | req | extra piston rows at top (rows 100+) |
| `DispExtraPedalButtonRow` | bool | req | extra piston row (row 99) above the pedal piston row |
| `DispExtraPedalButtonRowOffset` | bool | req iff ExtraPedalButtonRow=Y, default N | shift row 99 left half a button |
| `DispExtraPedalButtonRowOffsetRight` | bool | req iff ExtraPedalButtonRow=Y, default N | parsed, **not used** |
| `DispButtonsAboveManuals` | bool | req | piston rows above (Y) or below (N) their manual |
| `DispTrimAboveManuals` | bool | req | 8 px trim above top manual |
| `DispTrimBelowManuals` | bool | req | 8 px trim below manual 1 |
| `DispTrimAboveExtraRows` | bool | req | 8 px trim above the top jamb block |
| `DispExtraDrawstopRowsAboveExtraButtonRows` | bool | req | order of extra drawstop rows vs extra piston rows in the top jamb |
| `DispDrawstopWidth` / `DispDrawstopHeight` | int 1–150 | 78 / 69 | grid cell size for drawstops |
| `DispPistonWidth` / `DispPistonHeight` | int 1–150 | 44 / 40 | grid cell size for pistons |
| `DispEnclosureWidth` / `DispEnclosureHeight` | int 1–150 | 52 / 63 | enclosure slot size |
| `DispPedalHeight` | int 1–500 | 40 | pedalboard height |
| `DispPedalKeyWidth` | int 1–500 | 7 | pedal key advance |
| `DispManualHeight` | int 1–500 | 32 | manual keyboard height |
| `DispManualKeyWidth` | int 1–500 | 12 | natural key advance |

## 4. Layout engine (`GOGUILayoutEngine.cpp`)

Let `SW, SH` = screen size; `DW, DH` = drawstop w/h; `BW, BH` = piston w/h; `EW, EH` = enclosure w/h; `MH` = manual height; `PH` = pedal height; `cols, rows` = drawstop cols/rows; `extraRows/extraCols` = extra drawstop rows/cols; `btnCols, extraBtnRows` = piston cols / extra rows. All divisions are integer.

### 4.1 Vertical stack (Update)
Manuals are registered bottom-up: index 0 = pedal (or empty slot if `HasPedals=N`), 1 = lowest manual, etc.

```
centerY = SH - PH
centerW = max(extraCols*DW, btnCols*BW)

if pedal present (slot 0 occupied):
    mri[0].height = PH; mri[0].y = mri[0].keys_y = centerY
    centerY -= PH
    if DispExtraPedalButtonRow: centerY -= BH
    mri[0].piston_y = centerY
    centerW = max(centerW, EW * numEnclosures)
    centerY -= 12; centerY -= EH; enclosureY = centerY; centerY -= 12
else if no pedal but enclosures exist:
    centerY -= 12; centerY -= EH; enclosureY = centerY; centerY -= 12

for each manual i = 1..N (bottom to top):
    if not DispButtonsAboveManuals: centerY -= BH; mri[i].piston_y = centerY
    mri[i].height = MH
    if DispTrimBelowManuals and i == 1: mri[i].height += 8; centerY -= 8
    centerY -= MH; mri[i].keys_y = centerY
    if DispTrimAboveManuals and i == topmost: centerY -= 8; mri[i].height += 8
    if DispButtonsAboveManuals: centerY -= BH; mri[i].piston_y = centerY
    mri[i].y = centerY
    # keyboard width:
    if i == 0 (pedal): width = 1 + Σ over keys: PedalKeyWidth,
                        plus extra PedalKeyWidth whenever key k>0 and keys k-1,k are both naturals
    else: width = 1 + ManualKeyWidth * (number of natural keys)
    mri[i].x = (SW - width) / 2 ;  mri[i].width = width + 16
    centerW = max(centerW, mri[i].width)

hackY = centerY
if centerW + 2*jambLRW < SW: centerW += (SW - centerW - 2*jambLRW) / 3
centerY -= extraBtnRows*BH
centerY -= extraRows*DH
if DispTrimAboveExtraRows: centerY -= 8
centerX = (SW - centerW) / 2
```

### 4.2 Jamb geometry
```
jambLRH = (rows + 1) * DH
jambLRY = (SH - jambLRH - (DispDrawstopColsOffset ? DH/2 : 0)) / 2
jambLRW = cols*DW/2 ;  if pair: jambLRW += (cols>>2)*(DW/4) - 8
jambLeftX  = (centerX - jambLRW) / 2 ; if pair: += 5
jambRightX = jambLeftX + centerX + centerW ; if pair: += 5
jambTopH = extraRows * DH ;  jambTopW = extraCols * DW
jambTopX = (SW - jambTopW) / 2
jambTopY = jambTopDrawstop = jambTopPiston = centerY + (DispTrimAboveExtraRows ? 8 : 0)
pistonTopH = extraBtnRows * BH ; pistonW = btnCols * BW ; pistonX = (SW - pistonW)/2
enclosureX(k) = (SW - EW*numEnclosures + 6)/2 + k*EW   (k = registration order)
enclosureY as computed above
```

### 4.3 Drawstop position (row `r`, col `c` from ODF)
```
if r > 99:                                    # top centre jamb
    x = jambTopX + (c-1)*DW + 6
    y = jambTopDrawstop + (r-100)*DH + 2
    if not DispExtraDrawstopRowsAboveExtraButtonRows: y += extraBtnRows*BH
else:                                         # side jambs
    half = cols >> 1
    if c <= half: x = jambLeftX  + (c-1)*DW + 6
    else:         x = jambRightX + (c-1-half)*DW + 6
    y = jambLRY + (r-1)*DH + 32
    if pair: x += (((c-1) mod half) >> 1) * (DW/4)
    d = (c <= half) ? c : cols - c + 1        # distance from outer edge
    if DispDrawstopColsOffset and ((d & 1) XOR DispDrawstopOuterColOffsetUp): y += DH/2
```

### 4.4 Piston position (row `r`, col `c`)
```
x = pistonX + (c-1)*BW + 6
if r > 99:                                    # top rows
    y = jambTopPiston + (r-100)*BH + 5
    if DispExtraDrawstopRowsAboveExtraButtonRows: y += extraRows*DH
else:
    i = (r == 99) ? 0 : r                     # manual index; 99 = extra pedal row
    if i >= number of manual slots:
        y = hackY - (i+1-slots)*(MH+BH) + MH + 5
    else: y = mri[i].piston_y + 5
    if DispExtraPedalButtonRow and r == 0: y += BH
    if DispExtraPedalButtonRowOffset and r == 99: x -= BW/2 + 2
if piston and DispKeyLabelOnLeft == N: x -= 13
```

### 4.5 Background regions (`GOGUIHW1Background`) — drawn first, all tiled wood
1. `(0, 0, centerX, SH)` — `DispDrawstopBackgroundImageNum`
2. `(centerX+centerW, 0, SW-centerX-centerW, SH)` — same wood
3. `(centerX, 0, centerW, SH)` — `DispConsoleBackgroundImageNum`
4. If paired cols: for `i = 0 .. (cols>>2)-1`, inset rects `(jambLeftX-5 + i*(2*DW+18), jambLRY, 2*DW+10, jambLRH)` and the same at `jambRightX-5 …` — `DispDrawstopInsetBackgroundImageNum`
5. If `DispTrimAboveExtraRows`: `(centerX, centerY, centerW, 8)` — `DispKeyVertBackgroundImageNum`
6. If top jamb non-empty: `(centerX, jambTopY, centerW, jambTopH + pistonTopH)` — `DispKeyHorizBackgroundImageNum`

Per displayed manual (`GOGUIManualBackground`): vertical strip `(centerX, mri.y, centerW, mri.height)` with `DispKeyVertBackgroundImageNum`; piston strip `(centerX, mri.piston_y, centerW, BH)` (pedal with extra row: `2*BH`) with `DispKeyHorizBackgroundImageNum`.

Draw order: HW1 background → manual backgrounds → images → all other controls in load order.

## 5. Buttons (drawstops & pistons) — GUI keys (`GOGUIButton.cpp`)

`W`/`H` below = final bounding width/height. All optional.

| Key | Type / range | Default |
|---|---|---|
| `DisplayAsPiston` | bool | true for Generals, ReversiblePistons, Divisionals (and piston-flavoured setter elements); false otherwise |
| `DispLabelColour` | colour | `Dark Red` |
| `DispLabelFontSize` | font size | `NORMAL` (7) |
| `DispLabelFontName` | string | empty = panel `DispControlLabelFont` |
| `DispLabelText` | string | object `Name` |
| `DispKeyLabelOnLeft` | bool | true; if false and piston, x −= 13 |
| `DispImageNum` | int 1–7 (drawstop) / 1–5 (piston) | 1; read-only buttons: 4 (drawstop) / 3 (piston) |
| `DispDrawstopRow` | int 1–(99+extraRows) | 1 — rows ≤ 99 side jambs, ≥ 100 top jamb |
| `DispDrawstopCol` | int 1–cols (row≤99) or 1–extraCols (row>99) | 1 |
| `DispButtonRow` | int 0–(99+extraBtnRows) | 1 — 0 = pedal row, 99 = extra pedal row, ≥100 top rows |
| `DispButtonCol` | int 1–btnCols | 1 |
| `ImageOn`, `ImageOff` | file path | built-in `drawstopNN_on/off` or `pistonNN_on/off` per `DispImageNum` |
| `MaskOn` | file path | empty |
| `MaskOff` | file path | value of `MaskOn` |
| `PositionX`, `PositionY` | int 0–SW / 0–SH | −1 = position from layout grid (row/col) |
| `Width`, `Height` | int 1–SW / 1–SH | on-bitmap size; larger ⇒ bitmap tiled |
| `TileOffsetX/Y` | int 0–(bitmap size −1) | 0 — offset into the bitmap of the top-left pixel |
| `MouseRectLeft/Top` | int 0–(W−1)/(H−1) | 0 |
| `MouseRectWidth/Height` | int 1–(W−left)/(H−top) | remainder of bounding rect |
| `MouseRadius` | int 0–max(mw,mh) | min(mw,mh)/2; 0 = whole rect clickable, else circle |
| `TextRectLeft/Top` | int 0–(W−1)/(H−1) | 1 |
| `TextRectWidth/Height` | int | remainder of bounding rect |
| `TextBreakWidth` | int 0–TextRectWidth | TextRectWidth − (4 if TextRectWidth<50 else 14); **0 = draw no text** |

On/off bitmaps must be the same size (fatal otherwise). Drawn bitmap = engaged XOR `DisplayInInvertedState` ? on : off. Model-side keys affecting rendering: `Displayed` (default N), `DisplayInInvertedState` (default N).

## 6. Labels (`GOGUILabel.cpp`)

| Key | Type / range | Default |
|---|---|---|
| `FreeXPlacement` / `FreeYPlacement` | bool | true |
| `DispXpos` / `DispYpos` | int 0–SW / 0–SH | 0 (only when free placement) |
| `DispDrawstopCol` | int 1–cols | **required** if FreeXPlacement=N |
| `DispSpanDrawstopColToRight` | bool | **required** if FreeXPlacement=N |
| `DispAtTopOfDrawstopCol` | bool | **required** if FreeYPlacement=N |
| `DispLabelColour` | colour | `BLACK` |
| `DispLabelFontSize` | font size | `NORMAL` |
| `DispLabelFontName` | string | empty = panel `DispGroupLabelFont` |
| `Name` | string | empty (the displayed text; ignored for dynamic setter labels) |
| `DispImageNum` | int 0–15 | 1; 0 = no background image (transparent). Note: built-in `label09` exists on disk but is **not registered**; `DispImageNum=9` fails at load |
| `Image` / `Mask` | file path | built-in `labelNN` per DispImageNum / empty |
| `PositionX` / `PositionY` | int 0–SW / 0–SH | overrides the Disp*-derived position; −1 sentinel = jamb-relative |
| `Width` / `Height` | int 1–SW / 1–SH | bitmap size, or 80×25 if no bitmap; larger ⇒ tiled |
| `TileOffsetX/Y` | int 0–(W−1)/(H−1) | 0 |
| `TextRectLeft/Top` | int | 1 / 1 |
| `TextRectWidth/Height` | int | remainder |
| `TextBreakWidth` | int 0–TextRectWidth | TextRectWidth; 0 = no text |

Non-free placement (relative to side jambs, drawstop width hard-coded 78):
```
half = cols >> 1;  span = DispSpanDrawstopColToRight ? 39 : 0
X: col <= half: x = jambLeftX  + span + (col-1)*78 + 1
   else:        x = jambRightX + span + (col-1-half)*78 + 1
Y: top:    y = jambLRY + 1
   bottom: y = jambLRY + 1 + jambLRH - 32
```

## 7. Enclosures (`GOGUIEnclosure.cpp`)

| Key | Type / range | Default |
|---|---|---|
| `EnclosureStyle` | int 1–4 | 1 — built-in styles A–D |
| `BitmapCount` | int 1–128 | 16 |
| `Bitmap999` / `Mask999` (1…BitmapCount) | file path | built-in `enclosure<style><NN>` (NN = 000-based frame, `enclosureA00`…`enclosureA15`) / empty. All frames must be the same size |
| `PositionX/Y` | int 0–SW / 0–SH | −1 = layout slot (centred row above pedal) |
| `Width` / `Height` | int | bitmap size (built-ins 46×61); larger ⇒ tiled |
| `TileOffsetX/Y` | int 0–(bmp−1) | 0 |
| `DispLabelColour` | colour | `White` |
| `DispLabelFontSize` | font size | 7 |
| `DispLabelFontName` | string | empty |
| `DispLabelText` | string | enclosure `Name` |
| `TextRectLeft/Top` | int | 0 / 0 |
| `TextRectWidth/Height` | int | remainder |
| `TextBreakWidth` | int | TextRectWidth; 0 = no text (text drawn top-aligned) |
| `MouseRectLeft/Top` | int | 0 / 13 |
| `MouseRectWidth/Height` | int | W−left / H−top−3 |
| `MouseAxisStart` | int 0–mh | mh/3 |
| `MouseAxisEnd` | int MouseAxisStart–mh | mh·2/3 |

Displayed frame index = `(BitmapCount−1) × value / 127` (value = MIDI 0–127). Model key: `AmpMinimumLevel` (0–100, model section, req).

## 8. Manuals as GUI elements (`GOGUIManual.cpp`)

Key naturalness from MIDI note n: natural iff `(n%12 < 5 and n even) or (n%12 ≥ 5 and n odd)`; else sharp.

| Key | Type / range | Default |
|---|---|---|
| `PositionX/Y` | int 0–SW/SH | −1 = layout (`mri.x + 1`, `mri.keys_y`) |
| `DispKeyColourInverted` | bool | N |
| `DispKeyColourWooden` | bool | N (manuals only) |
| `DispImageNum` | int 1–2 | 1 — built-in key bitmap set |
| `DisplayFirstNote` | int 0–127 | model `FirstAccessibleKeyMIDINoteNumber` |
| `DisplayKeys` | int 1–NumberOfAccessibleKeys | NumberOfAccessibleKeys |
| `DisplayKey999` | int 0–127 | FirstNote + (999−1) — MIDI note of the backend key this GUI key plays |
| `DisplayKey999Note` | int 0–127 | same default — note used for the *displayed* shape |
| `ImageOn_KEYTYPE`, `ImageOff_KEYTYPE`, `MaskOn_KEYTYPE`, `MaskOff_KEYTYPE` | file path | built-in |
| `Width_KEYTYPE`, `Offset_KEYTYPE` (−500–500), `YOffset_KEYTYPE` | int 0–500 / ±500 | see below |
| `Key999ImageOn/ImageOff/MaskOn/MaskOff` | file path | per-key override of the above |
| `Key999Width`, `Key999Offset`, `Key999YOffset` | int | per-key override |
| `Key999MouseRectLeft/Top/Width/Height` | int | whole key bitmap |

`KEYTYPE` = note name of the *display* note: `C, Cis, D, Dis, E, F, Fis, G, Gis, A, Ais, B`, prefixed `First` for the first and `Last` for the last displayed key (e.g. `FirstC`, `LastB`).

Built-in bitmap names: `<T><NN>On_<S>` / `<T><NN>Off_<S>` where `T` = `Manual` / `ManualInverted` / `ManualWood` / `ManualInvertedWood` / `Pedal` / `PedalInverted`, `NN` = DispImageNum (01/02), and shape `S`:
* Pedal: `Natural` (7×40), `Sharp` (7×18)
* Manual: `Sharp` (6×19); naturals by neighbourhood — prev not sharp & next sharp → `C`; prev & next sharp → `D`; prev sharp & next not → `E`; else `Natural` (all naturals 12×32)

Default per-key geometry (x advances left→right, y = 0):
* Manual sharp: width (advance) = 0, offset = −(bitmapWidth/2) (i.e. −3), drawn overlapping
* Manual natural: advance = bitmap width (12)
* Pedal key: advance = bitmap width (7); doubled (14) when the key is a natural **and** the next key is a natural
* Key rect = `(x + offset, y + yoffset, bmpW, bmpH)`; bounding box is the union; on/off bitmap must match in size

## 9. Images (`GOGUIImage.cpp`) — `[Image999]` / `[PanelNNNImage999]`

| Key | Type | Default |
|---|---|---|
| `Image` | file path | **required** |
| `Mask` | file path | empty |
| `PositionX/Y` | int 0–SW / 0–SH | 0 |
| `Width` / `Height` | int 1–SW / 1–SH | bitmap size; larger ⇒ tiled |
| `TileOffsetX/Y` | int 0–(bmp−1) | 0 |

Masks (everywhere): a separate image of identical size; white (`#FFFFFF`) pixels of the mask become transparent.

## 10. Built-in bitmap inventory (`src/images/`, registered in `GOGuiImageCache.cpp`)

| Set | Count | Size (px) |
|---|---|---|
| `drawstop01…07` on/off | 7 sets | 65×65 |
| `piston01…05` on/off | 5 sets | 32×32 |
| `label01…15` (no 09) | 14 usable | 01/03/07/10: 80×25; 02/06: 80×50; 04/08/11: 160×25; 05/12: 200×50; 13–15: 400×50 |
| `enclosureA00–A15` … `D00–D15` | 4 styles × 16 frames | 46×61 |
| Manual keys sets 01, 02 (+Inverted/Wood variants) | naturals 12×32, sharps 6×19 |
| Pedal keys sets 01, 02 (+Inverted) | naturals 7×40, sharps 7×18 |
| `wood01…64` | 32 textures ×2 orientations | 256×256, tiled |

## 11. Setter / special element types (usable as old-format `SetterElement` `Type=` or new-format element `Type=`)

Buttons (default drawstop rendering unless noted; `DisplayAsPiston` can override) — from `GOSetter.cpp`, `GOMetronome.cpp`, `GODivisionalSetter.cpp`:
* Sequencer/setter: `Set`, `GC`, `Prev`, `Next`, `Current`, `Home`, `M1`, `M10`, `M100`, `P1`, `P10`, `P100`, `L0`–`L9`, `SetterN`, `Regular`, `Scope`, `Scoped`, `Full`, `Insert`, `Delete`
* Combination files: `RefreshFiles`, `PrevFile`, `NextFile`, `LoadFile`, `SaveFile`, `Save`
* Banked generals (pistons): `General01`–`General50`; bank switch: `GeneralPrev`, `GeneralNext`
* Crescendo: `CrescendoA`–`CrescendoD`, `CrescendoPrev`, `CrescendoCurrent`, `CrescendoNext`, `CrescendoOverride`
* Tuning: `PitchM1/M10/M100/P1/P10/P100`, `TemperamentPrev`, `TemperamentNext`, `TransposeDown`, `TransposeUp`
* Misc (pistons): `PanicButton`, `ExitGO`
* Banked divisionals (pistons): `Setter{MMM}Divisional{NNN}` (MMM = ODF manual number 000-padded; NNN = 000–009 for divisional 1–10), `Setter{MMM}DivisionalPrevBank`, `Setter{MMM}DivisionalNextBank`
* Not available to ODF panels (internal only): `OnState`, `MetronomeOn`, `MetronomeMeasureP1/M1`, `MetronomeBpmP1/M1/P10/M10`

Labels (take Label GUI attributes; text is dynamic, `Name` ignored): `Label` (old format only), `SequencerLabel`, `CrescendoLabel`, `GeneralLabel`, `PitchLabel`, `TemperamentLabel`, `TransposeLabel`, `CurrFileName`, `Setter{MMM}DivisionalBank`. (`OrganNameLabel`, `MetronomeBPM`, `MetronomeMeasure` are internal-panel-only.)

Enclosure-style element: `Swell` — the crescendo pedal; takes all enclosure GUI attributes.

## 12. Rendering summary for an importer

1. Read format flag; build the panel list (main + `NumberOfPanels`).
2. Per panel, read display metrics (§3); run the layout algorithm (§4) after registering, in order: pedal/manuals bottom-up and enclosures in load order.
3. Paint wood background regions (§4.5), manual backgrounds, then images and controls in ODF order.
4. For every control: bounding box = `PositionX/Y` if given, else the layout position; bitmap = custom file or built-in per `DispImageNum`; bitmap drawn tiled from `TileOffsetX/Y` over `Width×Height`; then the wrapped text in the text rectangle.


---

# Part 6 — Appendix

## A. Importer implementation checklist (recommended order)

1. **INI reader** (Part 1 §1): gzip detection → BOM/UTF-8 vs ISO-8859-1 → line split on `\n` (strip trailing `\r`) → strip `;` comments → `[Section]` headers → `Key=Value` at first `=`. Store per-section key→value maps; last duplicate wins; merge duplicate sections. Case-insensitive lookup with exact-case preference.
2. **Typed readers** (Part 1 §3): booleans (`Y`/`N` + lenient prefix), integers (`stol` semantics, range check), floats (C locale, comma fix-up), enums (exact match), file names (trailing-space trim, `\` separators).
3. **`[Organ]` header** (Part 2 §4): metadata + all `NumberOf*` counts + `HasPedals` → derive `firstManual` (0 or 1) and section list.
4. **Global objects in load order** (Part 2 §5): enclosures → switches → tremulants → windchest groups → ranks. Respect the pipe-config inheritance tree (Part 3).
5. **Manuals** (Part 3): key geometry (logical vs accessible), then per manual its stops (`[StopNNN]` by section number) and couplers.
6. **Pipes and samples** (Part 4): `DUMMY` / `REF:m:s:p` / sample path; attacks, releases, loops, crossfades; pitch and amplitude parameters. Resolve `REF:` pipes only after all manuals/stops are loaded.
7. **Optional**: reversible pistons, divisional couplers, divisionals, generals (Part 2 §7–8.1); panels (Part 5); `.orgue` package reading (Part 1 §7).
8. **Validation stance**: to maximize compatibility with real-world sample sets, treat GrandOrgue's *warnings* as ignorable and reproduce only its *fatal* checks (Part 1 §9); consider case-insensitive file matching even though GO matches exactly (many sets are authored on Windows).

## B. Known quirks an importer should be aware of

| Quirk | Detail | Where |
|---|---|---|
| `#RRGGBB` colour parsing bug | GO weights hex digits decimally (`d1*10+d2`), so `#FF0000` renders as RGB(165,0,0). Named colours are safe. | Part 1 §3.9 |
| `LoopCrossfadeLength` unit inconsistency | Validated as if in samples, applied as milliseconds (`× sampleRate / 1000`). Range 0–3000. | Part 4 (Crossfades) |
| `ReadSize`/`ReadFontSize` dead range checks | The documented ranges (100–32000 / 1–50) are never enforced due to `&&` bugs; any integer passes. | Part 1 §3.10–3.11 |
| Keys are not whitespace-trimmed | `Key = value` defines key `"Key "`, which silently fails lookups. Values keep leading whitespace for plain string reads. | Part 1 §1.6 |
| Semicolon kills the rest of the line anywhere | `;` cannot appear in any section name, key or value. | Part 1 §1.4 |
| Comment stripping trims trailing whitespace only on commented lines | A value's trailing spaces survive only when the line has no `;`. | Part 1 §1.4 |
| Float parse failures are silent | Unparsable floats become `0.0` without error; integers in the same situation are fatal. | Part 1 §3.7 |
| `MinVelocityVolume`/`MaxVelocityVolume` per-pipe quirk | Read per pipe but *without* the `Pipe%03d` prefix (i.e. from the rank/stop section). | Part 3 (pipes) |
| Old GO 0.2 embedded settings | Sections starting with `_` are combination data, not model data. | Part 1 §2.2 |
| `HauptwerkOrganFileFormatVersion` | Read and discarded; its presence does not make the file a Hauptwerk ODF. GO's ODF is HW1-*like* but distinct. | Part 2 §4.1 |
| Legacy per-object `Displayed=Y` | Model sections double as main-panel GUI definitions (old panel format). New format uses `[Panel000…]`/`[PanelNNNElementMMM]`. | Part 5 §2 |
| Percussive + wav loops | Loops present in samples/ODF of an effectively percussive pipe are ignored (one-shot playback), but still validated. | Part 4 |
| `label09` missing | Built-in label bitmap 9 is not registered; `DispImageNum=9` on a label is fatal. | Part 5 §6 |
| Windows-1252 vs ISO-8859-1 | Non-BOM files decode as ISO-8859-1 exactly; bytes 0x80–0x9F do *not* become “smart quotes”. | Part 1 §1.2 |

## C. Minimal valid example ODF (old panel format)

A one-manual, one-stop organ with 61 pipes. All keys shown are either required or load-bearing; `;` comments here are for the reader (they are legal ODF comments).

```ini
[Organ]
ChurchName=Demo Organ
ChurchAddress=Demo City
OrganBuilder=Example Builder
NumberOfManuals=1
HasPedals=N
NumberOfWindchestGroups=1
NumberOfEnclosures=0
NumberOfTremulants=0
NumberOfReversiblePistons=0
NumberOfDivisionalCouplers=0
NumberOfGenerals=0
DivisionalsStoreIntermanualCouplers=N
DivisionalsStoreIntramanualCouplers=N
DivisionalsStoreTremulants=N
GeneralsStoreDivisionalCouplers=N
; --- main panel display metrics (old format: they live in [Organ]) ---
DispScreenSizeHoriz=MEDIUM
DispScreenSizeVert=MEDIUM
DispDrawstopBackgroundImageNum=1
DispConsoleBackgroundImageNum=3
DispKeyHorizBackgroundImageNum=5
DispKeyVertBackgroundImageNum=5
DispDrawstopInsetBackgroundImageNum=1
DispControlLabelFont=Arial
DispShortcutKeyLabelFont=Arial
DispShortcutKeyLabelColour=Yellow
DispGroupLabelFont=Arial
DispDrawstopCols=2
DispDrawstopRows=1
DispDrawstopColsOffset=N
DispPairDrawstopCols=N
DispExtraDrawstopRows=0
DispExtraDrawstopCols=0
DispButtonCols=10
DispExtraButtonRows=0
DispExtraPedalButtonRow=N
DispButtonsAboveManuals=N
DispTrimAboveManuals=N
DispTrimBelowManuals=N
DispTrimAboveExtraRows=N
DispExtraDrawstopRowsAboveExtraButtonRows=N
NumberOfLabels=0

[WindchestGroup001]
Name=Main chest
NumberOfEnclosures=0
NumberOfTremulants=0

[Manual001]
Name=Great
NumberOfLogicalKeys=61
FirstAccessibleKeyLogicalKeyNumber=1
FirstAccessibleKeyMIDINoteNumber=36
NumberOfAccessibleKeys=61
Displayed=Y
NumberOfStops=1
Stop001=1
NumberOfCouplers=0
NumberOfDivisionals=0
NumberOfTremulants=0
NumberOfSwitches=0

[Stop001]
Name=Principal 8'
Displayed=Y
DispDrawstopRow=1
DispDrawstopCol=1
DefaultToEngaged=N
FirstAccessiblePipeLogicalKeyNumber=1
NumberOfAccessiblePipes=61
; NumberOfRanks=0 (default) -> internal rank defined inline:
FirstAccessiblePipeLogicalPipeNumber=1
WindchestGroup=1
NumberOfLogicalPipes=61
Pipe001=Pipes\Principal8\036-C.wav
Pipe002=Pipes\Principal8\037-Cis.wav
; ... Pipe003 .. Pipe061 continue the chromatic scale ...
Pipe061=Pipes\Principal8\096-C.wav
```

Notes on the example:
- With `HasPedals=N` there is no `[Manual000]`; the single manual is `[Manual001]` and `firstManual = 1`.
- `Stop001=1` in `[Manual001]` is a *section number*: it points at `[Stop001]`.
- The stop uses the internal-rank shorthand (`NumberOfRanks` absent = 0), so the rank keys (`WindchestGroup`, `NumberOfLogicalPipes`, `Pipe%03d`) sit directly in the stop section, and `FirstMidiNoteNumber` defaults to 36 here (manual's first logical-key MIDI note 36 + 1 − 1).
- Each `Pipe%03d` may carry the full per-pipe key set of Parts 3–4 (`Pipe001Gain=…`, `Pipe001LoopCount=…`, `Pipe001Release001=…`, etc.).
