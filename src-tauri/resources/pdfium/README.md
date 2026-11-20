# pdfium

Native PDFium library used to rasterize PDFs (including password-protected ones).

| Platform | File |
|---|---|
| Windows | `pdfium.dll` |
| macOS | `libpdfium.dylib` |
| Linux | `libpdfium.so` |

Fetch for the current (or CI) target:

```bash
node scripts/fetch-pdfium.mjs
node scripts/fetch-pdfium.mjs --target aarch64-apple-darwin
```

Version pin must stay in sync with `PDFIUM_VERSION` in `src-tauri/src/archive/pdf.rs`.
Downloaded binaries are gitignored; CI / `beforeBuildCommand` fetch them before packaging.
