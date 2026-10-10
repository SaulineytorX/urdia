# Core linker observations

New private candidates include `link-observations.json` beside the archive.
The packager first performs its planned locked build, then uses a final-target
`cargo rustc` invocation to request an LLVM lld ELF map. It refuses the candidate
if the second link changes even one executable byte. Map flags are not applied
to dependency binaries or build scripts.

The report binds the executable SHA256 and groups positive-size input sections
by archive/object origin and output section. Rust library archive members,
native objects inside Rust archives, and host startup objects can appear.
Synthetic linker inputs are excluded. Paths under the isolated build and Rust
toolchain are normalized; host library paths are labeled separately. Unknown
origins, unsupported map formats, malformed rows, and empty observations fail
closed. Symbol names and input section names are omitted. Raw maps stay in the
temporary build directory and are deleted with that directory.

`package-evidence.json` binds the sidecar filename and SHA256. Package inspection,
host checks, recovery rehearsals and detached signing verify that binding and
the executable hash. Attestations bind it transitively through package evidence;
no additional publisher key is created. Independent package comparison requires
matching normalized observation bindings and monitors the sidecar for changes.

Older candidates remain readable without this field and have no linker
observation evidence. Their existing signatures are not upgraded by this change.
The sidecar is private review evidence, not an extra installed archive file.

These are section observations, not exact source provenance, instruction counts,
complete software bills of materials, license applicability approval, or proof
that all source in an archive was incorporated. Section sizes include linker
semantics such as merged strings; they are not additive executable file sizes.
Dependency counts also include build-only packages. Dynamic libraries remain
external and are checked separately by the host preflight. Source attribution,
compiler/runtime distribution provenance, final redistribution review and
production key custody remain release gates.

Format and invocation references: [LLVM lld map writer](https://llvm.googlesource.com/llvm-project/lld/+/refs/heads/main/ELF/MapFile.cpp)
and [Cargo final-target compiler arguments](https://doc.rust-lang.org/cargo/commands/cargo-rustc.html).
