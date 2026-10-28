# Native inputs for Windows CI

The runtime allowlist is `scripts/native-runtime.psd1`. The pinned Windows CT2
4.7.1 binary uses cuBLAS, not cuDNN. Do not add CUDA SDKs, models, Python packages,
personal config or session files to the input bundle.

## Prepare and review

Use a separate folder containing exactly the native runtime files, matching CT2
`include/` headers, and `notices/` with upstream license texts. Preserve the
source/version/license information for each file, including statically linked
components where applicable. A file copied from an existing machine is not, by
itself, evidence of its upstream build or redistribution terms.

`notices/inventory.json` uses schema version 1:

```json
{
  "schemaVersion": 1,
  "ct2Revision": "226c95d94e660c48b11c62e108886b7ef76d589d",
  "reviewed": false,
  "files": [
    {
      "path": "ctranslate2.dll",
      "bytes": 59823104,
      "sha256": "18b07b2c322b30f06383e7092b288470813b9d50eca1a3b23f9ed82483160b47",
      "version": "4.7.1",
      "origin": "https://github.com/OpenNMT/CTranslate2/tree/226c95d94e660c48b11c62e108886b7ef76d589d",
      "license": "MIT",
      "licensePath": "notices/CTranslate2-LICENSE.txt"
    }
  ]
}
```

This is an incomplete example, not an approved inventory. Every runtime file,
header and license/notice text needs its own entry. License texts can refer to
their own entry through `licensePath`. Do not include the inventory itself in
`files`. Paths are relative, forward-slash-separated, and case-insensitively
unique. Every file needs positive size, SHA-256, version, HTTPS upstream origin,
license identifier/description and an inventoried license text. No links allowed.

Set `reviewed` to true only after actual maintainer review. Validators establish
file integrity and metadata coverage, **not legal permission or clean-machine
compatibility**. The separate repository approval variable is also required by
the release workflow. Neither flag is set automatically.

From the repository root, with PowerShell 7:

```powershell
./scripts/test-native-inputs.ps1 -NativeRuntimeSource ./build/reviewed-native
./scripts/new-native-bundle.ps1 -NativeRuntimeSource ./build/reviewed-native -ArchivePath ./build/native-inputs-windows-x64.zip
```

The ZIP has no enclosing folder. Only inventoried files are included. Entries are
sorted with fixed timestamps; identical input files produce identical ZIP bytes.
Existing ZIP/checksum outputs are refused. The builder emits a `.zip.sha256` file.
`-WhatIf` validates and previews without writing a bundle.

## Configure GitHub

Host the reviewed ZIP at an immutable HTTPS location accessible to the runner,
in accordance with each vendor's redistribution conditions. Do not assume that
permission to ship a DLL within Voice Paste permits publishing a standalone SDK
bundle. A draft GitHub Release browser-download URL is not anonymously accessible.
The current downloader uses an unauthenticated HTTPS URL; do not put credentials
or signed URL secrets in repository variables.

Configure repository variables:

- `VOICEPASTE_NATIVE_BUNDLE_URL`: reviewed ZIP URL.
- `VOICEPASTE_NATIVE_BUNDLE_SHA256`: ZIP SHA-256 (64 hex characters).
- `VOICEPASTE_REDISTRIBUTION_APPROVED`: `true` after reviewing native/artwork terms.

Run **Windows portable release** manually to build a CI artifact without creating
a release. A version tag matching `Cargo.toml` builds the same artifact and attaches
it to a **draft** release. Existing published release assets are never overwritten.
Reruns may replace assets on the same draft only. Publishing remains manual.

## Checks

```powershell
./tests/native-inputs.Tests.ps1
./tests/portable-package.Tests.ps1
```

These tests use synthetic files and mocked HTTPS transport, with no native code,
GPU, models or downloads. They cover inventory/hash/notice failures, deterministic
allowlisted bundles, extraction safety, failed-download cleanup and overwrite
protection. Actual remote hosting, clean-machine loading and GPU inference require
separate qualification.
