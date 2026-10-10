# Core candidate host compatibility

The private core archive currently targets Linux x86_64 with glibc and external
SQLite. A successful source build on another system does not establish that the
existing archive runs there. Inspect the exact candidate first, then perform a
disposable installed rehearsal on that host. Keep the repository and candidates
private until the release checklist is approved.

## Inspect without running the candidate

Prerequisites: Python 3.12 (the verified environment), GNU binutils `readelf` at
`/usr/bin/readelf`, `getconf` at `/usr/bin/getconf`, and glibc `ldconfig` at
`/sbin/ldconfig`. These are inspection tools; no sudo, model, credential, listener,
or package installation is used by the checker. Obtain the checker and supporting
scripts from trusted project source, separately from an untrusted download.

```sh
python3 -I -B scripts/check-core-host.py \
  --candidate-dir "$HOME/urdia-candidates/core-first" \
  --evidence "$HOME/urdia-candidates/core-first/host-preflight.json"
```

The evidence path must be new, outside the checkout, with an existing parent.
The report is created atomically with owner-only permissions. Keep candidate
inputs stable while inspection runs. Exit status 0 means the listed preflight
conditions pass, 2 means observed host incompatibilities (a report is saved),
and 1 means invalid inputs, unsupported binary layout, unavailable inspection
tools, or an inspection/signature error. Error status 1 produces no new report.

The checker validates archive checksums, the complete file manifest, clean source
identity, and actual ELF64 little-endian x86_64 bytes. It compares actual required
libraries and numeric GLIBC symbol versions with the package build record. It
requires the expected glibc interpreter and refuses embedded RPATH/RUNPATH.
It then checks the host platform, numeric glibc floor, interpreter presence, and
required names in the system library cache backed by x86_64 ELF files.

The checker only reads the library cache with `ldconfig -p`; it never updates it.
`readelf` reads ELF metadata. It does not call `ldd` or launch the candidate;
`ldd` can execute code from an untrusted executable. See the
[GNU readelf documentation](https://sourceware.org/binutils/docs/binutils/readelf.html),
[ldconfig manual](https://man7.org/linux/man-pages/man8/ldconfig.8.html), and
[ldd security warning](https://man7.org/linux/man-pages/man1/ldd.1.html).

Passing does **not** verify every required symbol, library dependency closure,
loader resolution under a user's environment, kernel/CPU features, SQLite
behavior, complete ABI compatibility, build independence, or publisher identity.
It may conservatively block a usable library installed outside the system cache.
The report records `runtime_compatibility_verified: false` even on a passing host.
Run the [packaged rehearsal](release-package.md) separately before making runtime
support claims. This checker changes no installation/rehearsal behavior.

## Trusted signature before execution

For a signed candidate, supply a public key obtained through an independently
trusted channel. OpenSSL 3 with Ed25519 support is additionally required.

```sh
python3 -I -B scripts/check-core-host.py \
  --candidate-dir "$HOME/urdia-candidates/core-first" \
  --public-key /absolute/path/to/independently-trusted-public.pem \
  --evidence "$HOME/urdia-candidates/core-first/signed-host-preflight.json"
```

A supplied key makes valid detached verification mandatory; failure does not
fall back to unsigned inspection. Without it, static inspection is permitted
but `trusted_signature_verified` is false. Unsigned checksums establish internal
consistency, not authorship. Neither a passing inspection nor a key shipped
alongside the same download authorizes execution. The existing rehearsal does
not automatically invoke this checker or require a signature. Verify received
candidates before running it; reverify if files change.

The [preparation guide](release-preparation.md) records remaining production key
custody, independent trust distribution, rotation/revocation, and publication
approval. No production key or identity is created by this flow.

## Observed matrix, not a support promise

The following applies to clean candidate source `8eef8b3` and archive SHA256
`4e307bc6b350bb69a27f30ef99d0f05d703842fdc7c984f524dc6442fe02d657`.

| Host | Static preflight | Installed runtime evidence |
| --- | --- | --- |
| Ubuntu 24.04, x86_64, glibc 2.39, external SQLite | Pass on the current server | Ten disposable recovery checks per candidate; 21 source smoke checks |
| Linux x86_64, glibc older than 2.39 | Blocked by this candidate's numeric symbol floor | Not tested on another host |
| Other Linux x86_64 hosts with glibc 2.39 or later | Must inspect on the actual host | Not tested |
| musl, ARM, macOS, Windows | Outside current package/checker target | Not tested |

The 2.39 floor is extracted from this actual binary, not a minimum for every
future Urdia build. The Python service profiles have separate requirements and
are excluded from the core archive. A broader platform matrix requires actual
host evidence and, where needed, a separately built target. See
preparation evidence (private review material omitted from this snapshot) for previous
same-host builds and runtime checks. Recheck every changed candidate/toolchain.
