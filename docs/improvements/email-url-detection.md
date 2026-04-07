# Email/URL Detection

## Type

- 🐛 Bug Fix + 🔍 Enhanced Extraction + 🛡️ Security

## Python Reference Status

- Email regex limits TLDs to 2-4 characters, missing valid longer TLDs.
- IPv6/private IP handling has known correctness issues.
- URL canonicalization/error handling is less explicit.

## Rust Improvements

- Rust uses modern regex handling plus threshold and uniqueness controls for email extraction.
- URL extraction applies ordered cleaning and filtering, including credential stripping.
- Host and IP filtering remove more junk and private-address noise.
- Supported images can feed EXIF and XMP metadata text into the existing finder pipeline.
- Supported TTF/OTF fonts can now feed native font metadata text into the same shared finder pipeline.

## Impact

- Better correctness and parity confidence for email/URL extraction.
- Additional beyond-parity detections from EXIF/XMP image metadata on supported image formats and from native metadata embedded in supported TTF/OTF font files.
- Safer handling of sensitive URL credential data.
