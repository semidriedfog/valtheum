# VTA1 / VTV2 / VLTKEY01 format

This is the implemented format, not an independent security certification. It is sufficient to restore media without the valtheum database. Operational backup and password guidance is in [SECURITY.md](SECURITY.md).

All offsets are zero-based bytes; ranges below are half-open. AES-GCM tags are 16 bytes appended to ciphertext. Passwords are UTF-8 without normalization. Randomness comes from the OS CSPRNG. Writers generate new per-file keys and randomness for each encryption or rewrite; interrupted ciphertext is not resumed in place.

## Library key file: 96 bytes

The normal location is `.valtheum/vault.key`.

| Offset | Length | Meaning |
| --- | --- | --- |
| 0 | 8 | ASCII `VLTKEY01` |
| 8 | 4 | Argon2 memory cost, u32 little endian: 65536 KiB |
| 12 | 4 | Argon2 time cost, u32 little endian: 3 |
| 16 | 4 | Argon2 parallelism, u32 little endian: 1 |
| 20 | 16 | Random salt |
| 36 | 12 | Random wrapping nonce |
| 48 | 48 | Encrypted 32-byte library master key followed by its tag |

Derive 32 bytes with Argon2id version 19, using the UTF-8 password and the salt. AES-256-GCM decrypt bytes `[48,96)` with that derived key, nonce `[36,48)`, and AAD `[0,48)`. Require a 32-byte result. Accept only the exact file length, magic and KDF parameters above; reject unsupported costs before allocating KDF memory. The key writer requires at least 10 password characters and at most 1,024 UTF-8 bytes.

SQLCipher uses its own password-based DB encryption, not this master key. The DB stores the lowercase hexadecimal SHA-256 of the complete 96-byte key file in `library_info` under `archive_key_sha256`. That fingerprint detects a mismatched key backup but is unnecessary for standalone restoration.

Changing the folder password re-encrypts the DB and rewraps the same master key with a new salt and nonce. Existing media remains decryptable with a matching older key file and its older password.

## Media envelope: first 84 bytes

| Offset | Length | Meaning |
| --- | --- | --- |
| 0 | 8 | `VLTARC01` for VTA1, `VLTARC02` for VTV2 |
| 8 | 16 | Random file ID |
| 24 | 12 | Random wrapping nonce |
| 36 | 48 | Encrypted 32-byte per-file key followed by its tag |

AES-256-GCM decrypt bytes `[36,84)` with the library master key, nonce `[24,36)`, and AAD `[0,36)`. Require a 32-byte result. The entire 84-byte envelope is the associated data for the following streaming primitive. The on-disk filename is a separate random ID and is not needed for decryption.

## Streaming payload: absolute offset 84 onward

The layout is compatible with Google Tink AES-GCM-HKDF Streaming AEAD using a RAW output prefix, ciphertext segment size **65,536 bytes**, derived key size 32, and HKDF-SHA256. RAW means no additional Tink key-ID prefix is present. Segment boundaries are relative to the streaming payload, excluding the 84-byte envelope.

The payload begins with this 40-byte header:

| Payload offset | Length | Meaning |
| --- | --- | --- |
| 0 | 1 | Header length: decimal 40 |
| 1 | 32 | Random HKDF salt |
| 33 | 7 | Random nonce prefix |

Derive the segment AES-256 key using HKDF-SHA256: IKM is the unwrapped per-file key, salt is the 32-byte header salt, info is the complete 84-byte envelope, output length is 32.

For segment index `i`, the 12-byte AES-GCM nonce is:

```text
7-byte nonce prefix || u32 big-endian i || final marker
```

Indices start at zero. The marker is `1` for the final segment and `0` otherwise. Each segment uses empty AAD. Append each ciphertext and its 16-byte tag after the header. Every non-final segment is full; a final full segment does not need an extra empty segment.

| Segment | Plaintext capacity | Absolute ciphertext offset |
| --- | ---: | ---: |
| First (`i = 0`) | 65,480 | `84 + 40` |
| Later (`i >= 1`) | 65,520 | `84 + i * 65,536` |

For a total encrypted file length `N`, let `C = N - 84` and `S = ceil(C / 65,536)`. Require `1 <= S <= u32::MAX`. The final payload segment occupies `C - (S - 1) * 65,536` bytes; the reader requires at least 56 bytes for a single segment or 17 bytes for a later final segment. The authenticated plaintext stream length is `C - 40 - 16 * S`. Reject underflow, oversized values, truncated tags and invalid header lengths.

The Rust reader authenticates the final segment and metadata before exposing the media length. It then authenticates each requested segment before returning plaintext. Authentication failure must be an error, never EOF; invalidate any cached segment label on failure. Quick inspection and range playback do not authenticate every interior segment or recompute the complete media hash.

## VTA1 plaintext stream

VTA1 normally uses `.vta` files and envelope magic `VLTARC01`.

1. A u32 little-endian metadata byte length, at most 16,384.
2. That many UTF-8 JSON bytes with `name`, `format`, `size`, `sha256`.
3. Exactly `size` unchanged original archive bytes.

`name` is the original relative filename or the explicitly changed restoration name. `format` is `zip` or `tar.zst`. `size` is an unsigned original byte length, at most `1024^4` (1 TiB). `sha256` is the original bytes' SHA-256 as 64 lowercase hexadecimal characters. JSON field order and whitespace are not significant.

Require the authenticated plaintext length to equal `4 + metadata_length + size`. Full restoration authenticates every segment and compares the computed SHA-256 with the metadata value.

## VTV2 plaintext stream

VTV2 normally uses `.vtv` files and envelope magic `VLTARC02`. Key wrapping, offsets, streaming parameters and the library key format are identical to VTA1. The magic is authenticated by both the wrapping AAD and stream key derivation.

1. A u32 little-endian metadata byte length, at most 16,384.
2. UTF-8 JSON metadata with `name`, `format`, `size`, `sha256`.
3. Exactly `size` original video bytes.
4. The original video bytes' SHA-256 as **32 raw bytes**, not hexadecimal.

Metadata `sha256` must be the empty string. `format` is one of `mp4`, `webm`, `mkv`, `avi`, `mov`, `m4v`, `wmv`. `name` must end in the corresponding extension, case-insensitively. `size` is at most 1 TiB. Require the authenticated plaintext length to equal `4 + metadata_length + size + 32`.

The digest footer can cross a segment boundary and is authenticated as part of the stream; it has no separate tag. Never return footer bytes as video content. Full restoration computes the hash while writing exactly `size` bytes and compares it with the footer. Encryption computes the hash during one sequential input pass, then fully verifies the completed encrypted output before the app marks it ready.

## Restoration and validation requirements

- Reject unknown versions, invalid key sizes, invalid metadata, unsupported formats, inconsistent lengths and failed AES-GCM authentication.
- Restore to an explicit destination or a validated leaf name inside the user-selected output folder. Never follow an embedded absolute path or `..` component. CLI and Python require an explicit output filename; the GUI validates the embedded leaf name and supplies a fallback when necessary.
- Write to a temporary output, verify every segment and the full media SHA-256, sync, then publish without overwriting an existing destination. Restoration temporary files contain plaintext. Conversion and normal app playback do not create a full plaintext media file.
- Preserve old ciphertext on failed rewrites. Successful rewrites generate new keys/randomness and may change the physical storage filename, so DB and media backups must be taken together.
- Chunk authentication detects changes in accessed chunks, reordering, incorrect final markers and invalid framing. It does not repair damage or prevent replacement by an older complete authentic file.

## Implementations

| Component | Source |
| --- | --- |
| Rust key handling, VTA1, common reader and restoration | [`src/archive_crypto.rs`](../src/archive_crypto.rs) |
| VTV2 writer | [`src/video_crypto.rs`](../src/video_crypto.rs) |
| Authenticated HTTP video range reading | [`src/encrypted_video.rs`](../src/encrypted_video.rs) |
| Windows GUI restorer | [`src/bin/valtheum-restore.rs`](../src/bin/valtheum-restore.rs), [`src/restore_workflow.rs`](../src/restore_workflow.rs) |
| Console restorer | [`src/bin/valtheum-restore-cli.rs`](../src/bin/valtheum-restore-cli.rs) |
| Independent Python / Google Tink reader | [`tools/restore_vta.py`](../tools/restore_vta.py) |

Rust uses RustCrypto `aes-gcm`, `argon2`, `hkdf` and `sha2`; it implements the stream layout without linking the Tink runtime. Exact Rust versions are in [Cargo.lock](../Cargo.lock). The independent reader uses Google Tink for the streaming primitive and `cryptography`/`argon2-cffi` for key wrapping. Its pinned versions are in [restore-requirements.txt](../tools/restore-requirements.txt).

Build using the repository's [build script](../scripts/build.ps1) and [README prerequisites](../README.md). The outputs are `target/release/valtheum.exe`, `valtheum-restore.exe`, and `valtheum-restore-cli.exe`. The restore programs need neither a DB nor an online server. Only the GUI needs WebView2. GUI key selection also accepts a `.vtbackup`/ZIP containing `.valtheum/vault.key`; CLI and Python require the extracted 96-byte key file. The Python implementation requires hard-link support for publishing the output on the same volume; Windows Rust restoration does not require hard links.

## Independent restoration and reproducibility

Run the following from the repository root. These examples use **public synthetic data**, never a personal library. Output paths must not already exist; choose a new output name when repeating a run. Install dependencies before offline restoration.

### Published VTA1 vector

The preserved fixture is [tests/fixtures/vta1](../tests/fixtures/vta1/README.md). It contains `vault.key`, `sample.vta`, `original.zip`, and a [SHA-256 manifest](../tests/fixtures/vta1/manifest.json). The password is **`interop password 123`**, for tests only. `original.zip` is 180,000 bytes with byte `i = i mod 251`; it is not a valid ZIP archive.

```powershell
python -m venv .venv-restore
.\.venv-restore\Scripts\python.exe -m pip install -r tools\restore-requirements.txt
New-Item -ItemType Directory -Force target\restore-check | Out-Null
.\.venv-restore\Scripts\python.exe tools\restore_vta.py tests\fixtures\vta1\vault.key tests\fixtures\vta1\sample.vta target\restore-check\vta1-restored.zip
(Get-FileHash tests\fixtures\vta1\original.zip -Algorithm SHA256).Hash -eq (Get-FileHash target\restore-check\vta1-restored.zip -Algorithm SHA256).Hash
```

Enter the public password when prompted. The final comparison must print `True`. The expected original SHA-256 is `252d829217aaec1f13d3ad2972c8939ca00ae25379f073bdb32aba4442588898`. The Rust CLI can run the same vector with a different unused output path:

```powershell
.\target\release\valtheum-restore-cli.exe tests\fixtures\vta1\vault.key tests\fixtures\vta1\sample.vta target\restore-check\rust-restored.zip
```

### Regenerated VTV2 vector

With the Rust build prerequisites configured, generate a synthetic vector:

```powershell
cargo test --locked --bin vault write_video_interop_fixture -- --ignored
$fixtureVideo = Join-Path 'target/video-v2-interop' (Get-Content 'target/video-v2-interop/latest.txt' -Raw)
.\.venv-restore\Scripts\python.exe tools\restore_vta.py target\video-v2-interop\vault.key $fixtureVideo target\restore-check\vtv2-restored.mp4
(Get-FileHash target\video-v2-interop\original.mp4 -Algorithm SHA256).Hash -eq (Get-FileHash target\restore-check\vtv2-restored.mp4 -Algorithm SHA256).Hash
```

Enter **`public fixture password 123`** when prompted. The final comparison must print `True`. The 220,000-byte source is synthetic byte-pattern data, not a playable video. Regeneration replaces this generated fixture's key and writes a new randomly named ciphertext; use the path recorded in `latest.txt`. Do not reuse either public fixture's key or password for real data.

## VTI1: individual images and notes

Magic `VLTIMG01` uses the same 84-byte key envelope and 64 KiB AES-GCM-HKDF stream described above. The stream plaintext is `[u32 metadata length][UTF-8 JSON manifest][original image bytes][sealed note bodies]`. Original image bytes are preserved exactly. Formats: jpg, jpeg, png, webp, gif, bmp, avif. Manifest size is limited to 262144 bytes.

The manifest contains the existing `name`, `format`, `size`, and hexadecimal `sha256` fields, plus a random 32-hex-character `revision`, `notes_size`, and ordered `notes` records. Each record has a unique 32-hex-character `id`, `title` (up to 512 UTF-8 bytes), `offset` relative to the note tail, `length` including the GCM tag, and `nonce` as an array of 12 byte values. Offsets are contiguous from zero, nonces are unique within a file, and the final offset plus length must equal `notes_size`. Exact total plaintext length is required. Maximum 128 records, 262144 plaintext bytes per body, and 8 MiB for all sealed bodies. Revision is for optimistic edit conflict checks, not rollback protection.

For note bodies, derive a separate 32-byte key using HKDF-SHA256 with the unwrapped file key as input, envelope bytes 8..24 as salt, and ASCII `VTI1 note records` as info. Encrypt each exact UTF-8 body using AES-256-GCM, its fresh 12-byte nonce, and the ASCII note ID as associated data. Append the resulting ciphertext and 16-byte tag at the recorded offset. The index and image are protected by the outer stream; each body is independently sealed inside it. Listing decrypts the index; reading one note decrypts only that inner body. Full verification or mutation reads all bodies.

A rewrite creates fresh file key, envelope randomness, stream randomness, revision, and body nonces; existing note IDs remain stable. Verify the image digest, authenticate all records, and compare all saved titles and bodies to the input before publishing a replacement. The database points to `.valtheum/images/<random ID>.vti`, and contains no note text. Existing encrypted archive/video formats are unchanged. The common Reader exposes only the original image bytes through Read/Seek and a separate selected-note interface. Quick integrity checks authenticate the index and boundary blocks, but do not verify every note body or image byte.

Source: `src/image_crypto.rs`. Independent GUI normal restore writes only the original image. For legacy VTI notes, CLI and Python accept `--notes` and an explicit JSON destination. JSON output is an array of `{id,title,body}`. Both modes verify the entire container before publication, never overwrite existing outputs, and require the matching key and password. Restore temporaries are plaintext; normal viewing and encrypted rewrites do not write plaintext image or note files.

Generate a synthetic VTI vector with `cargo test --locked --bin vault write_image_interop_fixture -- --ignored`. It writes `target/image-v1-interop/vault.key`, `original.png` (180000 synthetic bytes, not a decoded image), `expected-notes.json`, and a random ciphertext named in `latest.txt`. Password: `public fixture password 123`, for tests only. Run `tools/restore_vta.py` once for the image and once with `--notes`, choosing separate unused destination paths; compare original bytes and parsed JSON exactly.

## VTN1: standalone note media

Magic VLTNOTE1 identifies a standalone note file (.vtn). It reuses the same 84-byte key envelope, 64 KiB stream and note record schema as VTI1. It contains exactly one note record and **no image bytes**. The manifest media format is json; the internal name ends with .json; size is zero; sha256 is the SHA-256 of the empty byte sequence. These fields describe the absent image payload, not the JSON export size. The entire note body is authenticated by its inner and outer AES-GCM tags. The note key HKDF info is ASCII VTN1 note records, with the same per-file input key and salt placement as VTI1. Never derive it using the VTI1 label.

Normal restore of VTN1 exports the exact note as a plaintext JSON array containing one {id,title,body} object. It validates the whole container before publishing and never overwrites an output. Titles and exact UTF-8 bodies are compared after writing a replacement. Maximum title 512 bytes and body 262144 bytes. Names and IDs are randomly generated; titles are also indexed in the required encrypted DB. The DB NOTE row points through archive_store to .valtheum/notes/<random32>.vtn. Its file_size is the UTF-8 body length, validated by quick inspection against record.length minus the 16-byte GCM tag. The database never stores the body.

The app no longer offers writing notes into individual photos. New standalone notes require no image. VTI parsing remains compatible with the earlier file format, but there is no photo-note authoring or migration UI/API.

Generate a synthetic independent-restore fixture with cargo test --locked --bin vault write_standalone_note_interop_fixture -- --ignored. Read target/note-interop/latest.txt for the generated ciphertext name. The test password is public fixture password 123. Restore without --notes and compare the output JSON against expected.json.
