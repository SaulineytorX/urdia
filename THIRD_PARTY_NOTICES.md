# Third-party notices inventory

This is a candidate notice inventory, not final distribution clearance.
Urdia's Apache-2.0 license does not replace third-party terms. Original
copyright, license, and notice bytes are retained under [third-party/texts](third-party/texts).
The [machine-readable inventory](third-party/inventory.json) records every
collected file, artifact hash, original path, and supplemental source revision.
See the [collection and release procedure](docs/third-party-notices.md).

Scope: 274 locked Rust registry packages (including inactive targets/features),
47 unique optional Python wheels including pip, and two pinned Laya model cards.
This is broader than a final binary's dependency set. Python packages and model
weights are not bundled with the Urdia core. No model weights were downloaded.
Collection found 497 Rust archive files, 218 wheel files, eleven supplemental
upstream provenance file references, 277 original source-comment references, 13 Rust library/toolchain notice references,
and two model cards: 496 distinct retained byte sequences.
A retained candidate file is not necessarily a license covering the whole package.

## Unresolved review

- The three previously missing candidates are resolved: r-efi AUTHORS,
  rsqlite-vfs verified workspace MIT license, and tokenizers exact-version source
  license. See the [resolution and first package plan](docs/release-package.md).
  Source license recovery does not prove optional-wheel build provenance.
- ACP SDK/schema and defmt-parser archives lacked standalone candidates; root
  license files were recovered from the exact Git revision embedded in each crate.
  ACP SDK/derive/schema original sources and manifest licensing now match at
  pinned revisions; defmt-parser is outside the core and remains separate.
- The core scope now has explicit candidate license choices, retained ring leading
  headers and verified fiat AUTHORS, and Rust standard-library notices. See
  [core notices](docs/core-third-party-notices.md). Final inclusion and obligation
  approval remains open; defined ring signal coverage is not exhaustive inline review.
  See the source applicability record (private review material omitted from this snapshot).
- The two model cards are provenance evidence, not complete model licensing review.
  SGLang, external agents, host SQLite/runtime libraries, and other deployment inputs
  require a separate inclusion decision and provenance review if distributed.
- Select the actual source/binary/optional bundle before producing final notices.
  Public release and repository visibility changes remain unauthorized.

## Rust package inventory

License expressions below are publisher declarations. A text link opens the
first candidate; the JSON inventory lists all candidates and supplements.

| Package | Version | Declared license | Retained candidate references |
| --- | --- | --- | --- |
| agent-client-protocol | 2.2.0 | Apache-2.0 | [1](third-party/texts/6ed4f049cce59e05197d585bdf8a39980b45d546d6ea718ee344555f3a867c3a.txt) |
| agent-client-protocol-derive | 2.2.0 | Apache-2.0 | [1](third-party/texts/6ed4f049cce59e05197d585bdf8a39980b45d546d6ea718ee344555f3a867c3a.txt) |
| agent-client-protocol-schema | 1.9.1 | Apache-2.0 | [1](third-party/texts/f250d08cee4549b22b3b4aaaf3a743473336fd280316df5d0340717e5127a221.txt) |
| android_system_properties | 0.1.6 | MIT OR Apache-2.0 | [2](third-party/texts/216486f29671a4262efe32af6d84a75bef398127f8c5f369b5c8305983887a06.txt) |
| anstream | 1.0.0 | MIT OR Apache-2.0 | [2](third-party/texts/c6596eb7be8581c18be736c846fb9173b69eccf6ef94c5135893ec56bd92ba08.txt) |
| anstyle | 1.0.14 | MIT OR Apache-2.0 | [2](third-party/texts/c6596eb7be8581c18be736c846fb9173b69eccf6ef94c5135893ec56bd92ba08.txt) |
| anstyle-parse | 1.0.0 | MIT OR Apache-2.0 | [2](third-party/texts/c6596eb7be8581c18be736c846fb9173b69eccf6ef94c5135893ec56bd92ba08.txt) |
| anstyle-query | 1.1.5 | MIT OR Apache-2.0 | [2](third-party/texts/c6596eb7be8581c18be736c846fb9173b69eccf6ef94c5135893ec56bd92ba08.txt) |
| anstyle-wincon | 3.0.11 | MIT OR Apache-2.0 | [2](third-party/texts/c6596eb7be8581c18be736c846fb9173b69eccf6ef94c5135893ec56bd92ba08.txt) |
| anyhow | 1.0.104 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| async-channel | 2.5.0 | Apache-2.0 OR MIT | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| async-io | 2.6.0 | Apache-2.0 OR MIT | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| async-lock | 3.4.2 | Apache-2.0 OR MIT | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| async-process | 2.5.0 | Apache-2.0 OR MIT | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| async-signal | 0.2.14 | Apache-2.0 OR MIT | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| async-task | 4.7.1 | Apache-2.0 OR MIT | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| async-trait | 0.1.92 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT | [3](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| axum | 0.8.9 | MIT | [1](third-party/texts/6a13bc24a100a6812f053879ec51b126b103af7cda6dbf48c4188722da44da9f.txt) |
| axum-core | 0.5.6 | MIT | [1](third-party/texts/008c87afcd2e626eaf564093250bed06dd7efb5732113264bba3dda8f1c556a1.txt) |
| base64 | 0.22.1 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| base64 | 0.23.1 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| bitflags | 1.3.2 | MIT/Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | [2](third-party/texts/a9040321c3712d8fd0b09cf52b17445de04a23a10165049ae187cd39e5c86be5.txt) |
| blocking | 1.7.0 | Apache-2.0 OR MIT | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| bs58 | 0.5.1 | MIT/Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| bytes | 1.12.1 | MIT | [1](third-party/texts/45f522cacecb1023856e46df79ca625dfc550c94910078bd8aec6e02880b3d42.txt) |
| cc | 1.5.1 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| cfg_aliases | 0.2.2 | MIT | [1](third-party/texts/31b94860253d8ec7b4529f51901044d3b459d6292d996504a36b1bae3a36a812.txt) |
| chacha20 | 0.10.2 | MIT OR Apache-2.0 | [2](third-party/texts/a9040321c3712d8fd0b09cf52b17445de04a23a10165049ae187cd39e5c86be5.txt) |
| chrono | 0.4.45 | MIT OR Apache-2.0 | [1](third-party/texts/946c9835d8034d24404f8cfec5f4654cee5dad17e944afc3d06d742cf2882831.txt) |
| clap | 4.6.7 | MIT OR Apache-2.0 | [2](third-party/texts/249fdb6694467dd615b259f7ddde07be3f8ae5a72cecebb0e24f6629af4215fe.txt) |
| clap_builder | 4.6.7 | MIT OR Apache-2.0 | [2](third-party/texts/249fdb6694467dd615b259f7ddde07be3f8ae5a72cecebb0e24f6629af4215fe.txt) |
| clap_derive | 4.6.7 | MIT OR Apache-2.0 | [2](third-party/texts/249fdb6694467dd615b259f7ddde07be3f8ae5a72cecebb0e24f6629af4215fe.txt) |
| clap_lex | 1.1.1 | MIT OR Apache-2.0 | [2](third-party/texts/249fdb6694467dd615b259f7ddde07be3f8ae5a72cecebb0e24f6629af4215fe.txt) |
| colorchoice | 1.0.5 | MIT OR Apache-2.0 | [2](third-party/texts/c6596eb7be8581c18be736c846fb9173b69eccf6ef94c5135893ec56bd92ba08.txt) |
| concurrent-queue | 2.5.0 | Apache-2.0 OR MIT | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| convert_case | 0.10.0 | MIT | [1](third-party/texts/aed7b1758e35afa0cd0fde059d61950747ca11cd0e5e169cb21c11608daed772.txt) |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | [2](third-party/texts/a9040321c3712d8fd0b09cf52b17445de04a23a10165049ae187cd39e5c86be5.txt) |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 | [2](third-party/texts/a9040321c3712d8fd0b09cf52b17445de04a23a10165049ae187cd39e5c86be5.txt) |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | [2](third-party/texts/a9040321c3712d8fd0b09cf52b17445de04a23a10165049ae187cd39e5c86be5.txt) |
| darling | 0.24.1 | MIT | [1](third-party/texts/8ea93490d74a5a1b1af3ff71d786271b3f1e5f0bea79ac16e02ec533cef040d6.txt) |
| darling_core | 0.24.1 | MIT | [1](third-party/texts/8ea93490d74a5a1b1af3ff71d786271b3f1e5f0bea79ac16e02ec533cef040d6.txt) |
| darling_macro | 0.24.1 | MIT | [1](third-party/texts/8ea93490d74a5a1b1af3ff71d786271b3f1e5f0bea79ac16e02ec533cef040d6.txt) |
| defmt | 1.1.1 | MIT OR Apache-2.0 | [2](third-party/texts/8173d5c29b4f956d532781d2b86e4e30f83e6b7878dce18c919451d6ba707c90.txt) |
| defmt-macros | 1.1.1 | MIT OR Apache-2.0 | [2](third-party/texts/8173d5c29b4f956d532781d2b86e4e30f83e6b7878dce18c919451d6ba707c90.txt) |
| defmt-parser | 1.0.0 | MIT OR Apache-2.0 | [2](third-party/texts/8173d5c29b4f956d532781d2b86e4e30f83e6b7878dce18c919451d6ba707c90.txt) |
| deranged | 0.5.8 | MIT OR Apache-2.0 | [2](third-party/texts/edd65bdd88957a205c47d53fa499eed8865a70320f0f03f6391668cb304ea376.txt) |
| derive_more | 2.1.1 | MIT | [1](third-party/texts/8a35369f3ca263b3c62fbb5032947e53b6bfebc6c8a4d1bb982de1c069f6fba5.txt) |
| derive_more-impl | 2.1.1 | MIT | [1](third-party/texts/8a35369f3ca263b3c62fbb5032947e53b6bfebc6c8a4d1bb982de1c069f6fba5.txt) |
| digest | 0.10.7 | MIT OR Apache-2.0 | [2](third-party/texts/a9040321c3712d8fd0b09cf52b17445de04a23a10165049ae187cd39e5c86be5.txt) |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| dyn-clone | 1.0.20 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| equivalent | 1.0.2 | Apache-2.0 OR MIT | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| errno | 0.3.14 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| event-listener | 5.4.2 | Apache-2.0 OR MIT | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| event-listener-strategy | 0.5.4 | Apache-2.0 OR MIT | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| fallible-iterator | 0.3.0 | MIT/Apache-2.0 | [2](third-party/texts/c6596eb7be8581c18be736c846fb9173b69eccf6ef94c5135893ec56bd92ba08.txt) |
| fallible-streaming-iterator | 0.1.9 | MIT/Apache-2.0 | [2](third-party/texts/c6596eb7be8581c18be736c846fb9173b69eccf6ef94c5135893ec56bd92ba08.txt) |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| find-msvc-tools | 0.1.14 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| fixedbitset | 0.5.7 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| foldhash | 0.2.0 | Zlib | [1](third-party/texts/b1181a40b2a7b25cf66fd01481713bc1005df082c53ef73e851e55071b102744.txt) |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| futures | 0.3.34 | MIT OR Apache-2.0 | [2](third-party/texts/275c491d6d1160553c32fd6127061d7f9606c3ea25abfad6ca3f6ed088785427.txt) |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 | [2](third-party/texts/275c491d6d1160553c32fd6127061d7f9606c3ea25abfad6ca3f6ed088785427.txt) |
| futures-concurrency | 7.7.1 | MIT OR Apache-2.0 | [2](third-party/texts/e4705ddab847449a2cdcb3c88b005ea10330aa249d9148ca2eef9c84c5d29895.txt) |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | [2](third-party/texts/275c491d6d1160553c32fd6127061d7f9606c3ea25abfad6ca3f6ed088785427.txt) |
| futures-executor | 0.3.34 | MIT OR Apache-2.0 | [2](third-party/texts/275c491d6d1160553c32fd6127061d7f9606c3ea25abfad6ca3f6ed088785427.txt) |
| futures-io | 0.3.34 | MIT OR Apache-2.0 | [2](third-party/texts/275c491d6d1160553c32fd6127061d7f9606c3ea25abfad6ca3f6ed088785427.txt) |
| futures-lite | 2.6.1 | Apache-2.0 OR MIT | [3](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 | [2](third-party/texts/275c491d6d1160553c32fd6127061d7f9606c3ea25abfad6ca3f6ed088785427.txt) |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 | [2](third-party/texts/275c491d6d1160553c32fd6127061d7f9606c3ea25abfad6ca3f6ed088785427.txt) |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | [2](third-party/texts/275c491d6d1160553c32fd6127061d7f9606c3ea25abfad6ca3f6ed088785427.txt) |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | [2](third-party/texts/275c491d6d1160553c32fd6127061d7f9606c3ea25abfad6ca3f6ed088785427.txt) |
| generic-array | 0.14.7 | MIT | [1](third-party/texts/c09aae9d3c77b531f56351a9947bc7446511d6b025b3255312d3e3442a9a7583.txt) |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | [2](third-party/texts/aaff376532ea30a0cd5330b9502ad4a4c8bf769c539c87ffe78819d188a18ebf.txt) |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | [2](third-party/texts/aaff376532ea30a0cd5330b9502ad4a4c8bf769c539c87ffe78819d188a18ebf.txt) |
| hashbrown | 0.12.3 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| hashlink | 0.12.2 | MIT OR Apache-2.0 | [2](third-party/texts/c144680885b29e4719e2a51f0aab5439a1e02d980692b5aaf086cae12727f28b.txt) |
| heck | 0.5.0 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| hermit-abi | 0.5.3 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| hex | 0.4.3 | MIT OR Apache-2.0 | [2](third-party/texts/c6596eb7be8581c18be736c846fb9173b69eccf6ef94c5135893ec56bd92ba08.txt) |
| http | 1.5.0 | MIT OR Apache-2.0 | [2](third-party/texts/8bb1b50b0e5c9399ae33bd35fab2769010fa6c14e8860c729a52295d84896b7a.txt) |
| http-body | 1.1.0 | MIT | [1](third-party/texts/248378d0a3383c173fb925f17141b88e71580b3ba17ddc6ac3d2a344683232ab.txt) |
| http-body-util | 0.1.5 | MIT | [1](third-party/texts/248378d0a3383c173fb925f17141b88e71580b3ba17ddc6ac3d2a344683232ab.txt) |
| httparse | 1.10.1 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| httpdate | 1.0.3 | MIT OR Apache-2.0 | [2](third-party/texts/4d10fe5f3aa176b05b229a248866bad70b834c173f1252a814ff4748d8a13837.txt) |
| hyper | 1.11.1 | MIT | [1](third-party/texts/2d01890414494742ba4a509fcec8efa40f6d8be22cbd72be7cff08d6fda4ec89.txt) |
| hyper-rustls | 0.27.10 | Apache-2.0 OR ISC OR MIT | [3](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| hyper-util | 0.1.21 | MIT | [1](third-party/texts/9e0a97848ea543aef745c98e84fde696a9a3e0735538f6daefdd3cb1942effc1.txt) |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 | [2](third-party/texts/696759d65dfe558ff7d9f031c76db19ec5c0767470fb67c4e8d990820d1e99c9.txt) |
| iana-time-zone-haiku | 0.1.2 | MIT OR Apache-2.0 | [2](third-party/texts/696759d65dfe558ff7d9f031c76db19ec5c0767470fb67c4e8d990820d1e99c9.txt) |
| icu_collections | 2.3.0 | Unicode-3.0 | [1](third-party/texts/f367c1b8e1aa262435251e442901da4607b4650e0e63a026f5044473ecfb90f2.txt) |
| icu_locale_core | 2.3.0 | Unicode-3.0 | [1](third-party/texts/f367c1b8e1aa262435251e442901da4607b4650e0e63a026f5044473ecfb90f2.txt) |
| icu_normalizer | 2.3.0 | Unicode-3.0 | [1](third-party/texts/f367c1b8e1aa262435251e442901da4607b4650e0e63a026f5044473ecfb90f2.txt) |
| icu_normalizer_data | 2.3.0 | Unicode-3.0 | [1](third-party/texts/f367c1b8e1aa262435251e442901da4607b4650e0e63a026f5044473ecfb90f2.txt) |
| icu_properties | 2.3.0 | Unicode-3.0 | [1](third-party/texts/f367c1b8e1aa262435251e442901da4607b4650e0e63a026f5044473ecfb90f2.txt) |
| icu_properties_data | 2.3.0 | Unicode-3.0 | [1](third-party/texts/f367c1b8e1aa262435251e442901da4607b4650e0e63a026f5044473ecfb90f2.txt) |
| icu_provider | 2.3.1 | Unicode-3.0 | [1](third-party/texts/f367c1b8e1aa262435251e442901da4607b4650e0e63a026f5044473ecfb90f2.txt) |
| ident_case | 1.0.1 | MIT/Apache-2.0 | [1](third-party/texts/508a77d2e7b51d98adeed32648ad124b7b30241a8e70b2e72c99f92d8e5874d1.txt) |
| idna | 1.1.0 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| indexmap | 1.9.3 | Apache-2.0 OR MIT | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| ipnet | 2.12.2 | MIT OR Apache-2.0 | [2](third-party/texts/87d9feb9238c6bd8e0024fc4733b06cff036f89f36d93b7df1c8a0549bbb7a5b.txt) |
| is_terminal_polyfill | 1.70.2 | MIT OR Apache-2.0 | [2](third-party/texts/c6596eb7be8581c18be736c846fb9173b69eccf6ef94c5135893ec56bd92ba08.txt) |
| itoa | 1.0.18 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| jiff | 0.2.37 | Unlicense OR MIT | [2](third-party/texts/01c266bced4a434da0051174d6bee16a4c82cf634e2679b6155d40d75012390f.txt) |
| jiff-core | 0.1.1 | Unlicense OR MIT | [2](third-party/texts/01c266bced4a434da0051174d6bee16a4c82cf634e2679b6155d40d75012390f.txt) |
| jiff-static | 0.2.37 | Unlicense OR MIT | [2](third-party/texts/01c266bced4a434da0051174d6bee16a4c82cf634e2679b6155d40d75012390f.txt) |
| jiff-tzdb | 0.1.8 | Unlicense OR MIT | [2](third-party/texts/01c266bced4a434da0051174d6bee16a4c82cf634e2679b6155d40d75012390f.txt) |
| jiff-tzdb-platform | 0.1.3 | Unlicense OR MIT | [2](third-party/texts/01c266bced4a434da0051174d6bee16a4c82cf634e2679b6155d40d75012390f.txt) |
| js-sys | 0.3.106 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| libc | 0.2.189 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| libsqlite3-sys | 0.38.2 | MIT | [2](third-party/texts/c10c1f27337546471e5f7e4e97fdd398b35b9d4e126115dcd22de8d8e65abf6f.txt) |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [4](third-party/texts/3290ae0fbc9ddb77d2239121d710f0bb9d31b3b4744e6d97fe01e652b4c1870b.txt) |
| litemap | 0.8.3 | Unicode-3.0 | [1](third-party/texts/f367c1b8e1aa262435251e442901da4607b4650e0e63a026f5044473ecfb90f2.txt) |
| log | 0.4.34 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| lru-slab | 0.1.3 | MIT OR Apache-2.0 OR Zlib | [3](third-party/texts/cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30.txt) |
| matchit | 0.8.4 | MIT AND BSD-3-Clause | [2](third-party/texts/de701d0618d694feb1af90f02181a1763d9b0bdeb70a3a592781e529077dba65.txt) |
| memchr | 2.8.3 | Unlicense OR MIT | [2](third-party/texts/01c266bced4a434da0051174d6bee16a4c82cf634e2679b6155d40d75012390f.txt) |
| mime | 0.3.17 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| mio | 1.2.3 | MIT | [1](third-party/texts/07919255c7e04793d8ea760d6c2ce32d19f9ff02bdbdde3ce90b1e1880929a9b.txt) |
| num-conv | 0.2.2 | MIT OR Apache-2.0 | [2](third-party/texts/0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594.txt) |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| once_cell_polyfill | 1.70.2 | MIT OR Apache-2.0 | [2](third-party/texts/c6596eb7be8581c18be736c846fb9173b69eccf6ef94c5135893ec56bd92ba08.txt) |
| parking | 2.2.1 | Apache-2.0 OR MIT | [3](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| pin-project | 1.1.13 | Apache-2.0 OR MIT | [2](third-party/texts/0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594.txt) |
| pin-project-internal | 1.1.13 | Apache-2.0 OR MIT | [2](third-party/texts/0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594.txt) |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | [2](third-party/texts/0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594.txt) |
| piper | 0.2.5 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| polling | 3.11.0 | Apache-2.0 OR MIT | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| portable-atomic | 1.15.0 | Apache-2.0 OR MIT | [2](third-party/texts/0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594.txt) |
| portable-atomic-util | 0.2.8 | Apache-2.0 OR MIT | [2](third-party/texts/0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594.txt) |
| potential_utf | 0.1.6 | Unicode-3.0 | [1](third-party/texts/f367c1b8e1aa262435251e442901da4607b4650e0e63a026f5044473ecfb90f2.txt) |
| powerfmt | 0.2.0 | MIT OR Apache-2.0 | [2](third-party/texts/155420c6403d4e0fca34105e3c03fdd6939b64c393c7ec6f95f5b72c5474eab0.txt) |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| quinn | 0.11.12 | MIT OR Apache-2.0 | [2](third-party/texts/c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4.txt) |
| quinn-proto | 0.11.18 | MIT OR Apache-2.0 | [2](third-party/texts/c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4.txt) |
| quinn-udp | 0.5.15 | MIT OR Apache-2.0 | [2](third-party/texts/c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4.txt) |
| quote | 1.0.47 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | [1](third-party/texts/d027e91dbc9cdbb2f1190068e498bd6b61cff022b6a032b191021ba658d96111.txt) |
| rand | 0.10.3 | MIT OR Apache-2.0 | [3](third-party/texts/90eb64f0279b0d9432accfa6023ff803bc4965212383697eee27a0f426d5f8d5.txt) |
| rand_core | 0.10.1 | MIT OR Apache-2.0 | [3](third-party/texts/92b81db30f7ab6d693e0af5661a40d273ca1947e891123ed823924192c10cbc7.txt) |
| rand_pcg | 0.10.2 | MIT OR Apache-2.0 | [3](third-party/texts/90eb64f0279b0d9432accfa6023ff803bc4965212383697eee27a0f426d5f8d5.txt) |
| ref-cast | 1.0.27 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| ref-cast-impl | 1.0.27 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| reqwest | 0.12.28 | MIT OR Apache-2.0 | [2](third-party/texts/751963a8b88c0e3a9f27e98933079fbcf09b9998b2c13ac49c2e4d58444520d6.txt) |
| ring | 0.17.14 | Apache-2.0 AND ISC | [6](third-party/texts/b3d734001a94efff3579978d953391aa7115f877657d25eb54037a43875d078a.txt) |
| rsqlite-vfs | 0.1.1 | MIT | [3](third-party/texts/e1c1975c3474cce31722836f101a40ffaf1cc784e4d2f38ce3e77b9712934e78.txt) |
| rusqlite | 0.40.2 | MIT | [1](third-party/texts/c10c1f27337546471e5f7e4e97fdd398b35b9d4e126115dcd22de8d8e65abf6f.txt) |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT | [2](third-party/texts/95bd3988beee069fa2848f648dab43cc6e0b2add2ad6bcb17360caf749802bcc.txt) |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [4](third-party/texts/377c2e7c53250cc5905c0b0532d35973392af16ffb9596a41d99d202cf3617c9.txt) |
| rustls | 0.23.45 | Apache-2.0 OR ISC OR MIT | [3](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 | [2](third-party/texts/45fd05c4865e7c350b98ad7ac50e1b15462d49af4a91e9b0c9dd933dc9a69742.txt) |
| rustls-webpki | 0.103.15 | ISC | [1](third-party/texts/5b698ca13897be3afdb7174256fa1574f8c6892b8bea1a66dd6469d3fe27885a.txt) |
| rustversion | 1.0.23 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| schemars | 0.9.0 | MIT | [1](third-party/texts/1954992a2b32e8a2af24a4c11b726902e344c1934b947a77f04d404908f8db30.txt) |
| schemars | 1.2.2 | MIT | [1](third-party/texts/1954992a2b32e8a2af24a4c11b726902e344c1934b947a77f04d404908f8db30.txt) |
| schemars_derive | 1.2.2 | MIT | [1](third-party/texts/1954992a2b32e8a2af24a4c11b726902e344c1934b947a77f04d404908f8db30.txt) |
| semver | 1.0.28 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| serde | 1.0.229 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| serde_derive_internals | 0.30.0 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| serde_path_to_error | 0.1.20 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| serde_spanned | 1.1.1 | MIT OR Apache-2.0 | [2](third-party/texts/c6596eb7be8581c18be736c846fb9173b69eccf6ef94c5135893ec56bd92ba08.txt) |
| serde_urlencoded | 0.7.1 | MIT/Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| serde_with | 3.23.0 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| serde_with_macros | 3.23.0 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | [2](third-party/texts/a9040321c3712d8fd0b09cf52b17445de04a23a10165049ae187cd39e5c86be5.txt) |
| shell-words | 1.1.1 | MIT/Apache-2.0 | [2](third-party/texts/848d7bf14c618c39f1e29fcac2318a2fcc4254045ba7e2f6f5fa799e057225cb.txt) |
| shlex | 2.0.1 | MIT OR Apache-2.0 | [2](third-party/texts/553fffcd9b1cb158bc3e9edc35da85ca5c3b3d7d2e61c883ebcfa8a65814b583.txt) |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| slab | 0.4.12 | MIT | [1](third-party/texts/8ce0830173fdac609dfb4ea603fdc002c2f4af0dc9b1a005653f5da9cf534b18.txt) |
| smallvec | 1.16.2 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| socket2 | 0.6.5 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| sqlite-wasm-rs | 0.5.5 | MIT | [1](third-party/texts/e1c1975c3474cce31722836f101a40ffaf1cc784e4d2f38ce3e77b9712934e78.txt) |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| strsim | 0.11.1 | MIT | [1](third-party/texts/1e697ce8d21401fbf1bddd9b5c3fd4c4c79ae1e3bdf51f81761c85e11d5a89cd.txt) |
| strum | 0.28.0 | MIT | [1](third-party/texts/8bce3b45e49ecd1461f223b46de133d8f62cd39f745cfdaf81bee554b908bd42.txt) |
| strum_macros | 0.28.0 | MIT | [1](third-party/texts/8bce3b45e49ecd1461f223b46de133d8f62cd39f745cfdaf81bee554b908bd42.txt) |
| subtle | 2.6.1 | BSD-3-Clause | [1](third-party/texts/d1fc1bc0d155df60b2e7705b6b2ae02a05c96f948e1cec6e2fb86360b09f346b.txt) |
| syn | 2.0.119 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| syn | 3.0.6 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| sync_wrapper | 1.0.2 | Apache-2.0 | [1](third-party/texts/0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594.txt) |
| synstructure | 0.14.0 | MIT | [1](third-party/texts/219920e865eee70b7dcfc948a86b099e7f4fe2de01bcca2ca9a20c0a033f2b59.txt) |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| time | 0.3.55 | MIT OR Apache-2.0 | [2](third-party/texts/0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594.txt) |
| time-core | 0.1.9 | MIT OR Apache-2.0 | [2](third-party/texts/0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594.txt) |
| time-macros | 0.2.32 | MIT OR Apache-2.0 | [2](third-party/texts/0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594.txt) |
| tinystr | 0.8.4 | Unicode-3.0 | [1](third-party/texts/f367c1b8e1aa262435251e442901da4607b4650e0e63a026f5044473ecfb90f2.txt) |
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT | [3](third-party/texts/cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30.txt) |
| tokio | 1.53.1 | MIT | [1](third-party/texts/253cd04c6714889df2d32f3f64d669179a1c95c76ac43c40882c52eb06bc3552.txt) |
| tokio-macros | 2.7.2 | MIT | [1](third-party/texts/0b83dc40cba89b9922bb84b0a9c7d2768ce37c1d7e138b7424fd4549915778c9.txt) |
| tokio-rustls | 0.26.5 | MIT OR Apache-2.0 | [2](third-party/texts/cc117d90b498b32b11a886f279b359da16a73c3b01efbb2f5cc004b20262334e.txt) |
| toml | 1.1.6+spec-1.1.0 | MIT OR Apache-2.0 | [2](third-party/texts/c6596eb7be8581c18be736c846fb9173b69eccf6ef94c5135893ec56bd92ba08.txt) |
| toml_datetime | 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 | [2](third-party/texts/c6596eb7be8581c18be736c846fb9173b69eccf6ef94c5135893ec56bd92ba08.txt) |
| toml_parser | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | [2](third-party/texts/c6596eb7be8581c18be736c846fb9173b69eccf6ef94c5135893ec56bd92ba08.txt) |
| toml_writer | 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 | [2](third-party/texts/c6596eb7be8581c18be736c846fb9173b69eccf6ef94c5135893ec56bd92ba08.txt) |
| tower | 0.5.3 | MIT | [1](third-party/texts/4249c8e6c5ebb85f97c77e6457c6fafc1066406eb8f1ef61e796fbdc5ff18482.txt) |
| tower-http | 0.6.11 | MIT | [1](third-party/texts/5049cf464977eff4b4fcfa7988d84e74116956a3eb9d5f1d451b3f828f945233.txt) |
| tower-layer | 0.3.3 | MIT | [1](third-party/texts/4249c8e6c5ebb85f97c77e6457c6fafc1066406eb8f1ef61e796fbdc5ff18482.txt) |
| tower-service | 0.3.3 | MIT | [1](third-party/texts/4249c8e6c5ebb85f97c77e6457c6fafc1066406eb8f1ef61e796fbdc5ff18482.txt) |
| tracing | 0.1.44 | MIT | [1](third-party/texts/898b1ae9821e98daf8964c8d6c7f61641f5f5aa78ad500020771c0939ee0dea1.txt) |
| tracing-attributes | 0.1.31 | MIT | [1](third-party/texts/898b1ae9821e98daf8964c8d6c7f61641f5f5aa78ad500020771c0939ee0dea1.txt) |
| tracing-core | 0.1.36 | MIT | [2](third-party/texts/898b1ae9821e98daf8964c8d6c7f61641f5f5aa78ad500020771c0939ee0dea1.txt) |
| try-lock | 0.2.5 | MIT | [1](third-party/texts/c816a0749cdc6bf062a5111c159723de51b2bfac66a1dac2655abd9e6b1583eb.txt) |
| typenum | 1.20.1 | MIT OR Apache-2.0 | [3](third-party/texts/db11fec9946737df39ca3898d9cd8c10ec6f6c3a884a6802b0ad0b81b4e8f23a.txt) |
| unicode-ident | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | [3](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 | [3](third-party/texts/23860c2a7b5d96b21569afedf033469bab9fe14a1b24a35068b8641c578ce24d.txt) |
| unicode-xid | 0.2.6 | MIT OR Apache-2.0 | [3](third-party/texts/23860c2a7b5d96b21569afedf033469bab9fe14a1b24a35068b8641c578ce24d.txt) |
| untrusted | 0.9.0 | ISC | [1](third-party/texts/7abd9b6960dcf7d4d0a48606a5b71bfe37d472db68d70637f3a58a56785f1621.txt) |
| url | 2.5.8 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT | [3](third-party/texts/c30152c94a6d75e021adbc52b3a52470366a46edb917e17deae3259251af244c.txt) |
| utf8parse | 0.2.2 | Apache-2.0 OR MIT | [2](third-party/texts/62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a.txt) |
| uuid | 1.26.1 | Apache-2.0 OR MIT | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| vcpkg | 0.2.15 | MIT/Apache-2.0 | [2](third-party/texts/60c93a31f490375aadf64098b75f10715010379541021e00261045d7800611d3.txt) |
| version_check | 0.9.5 | MIT/Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| want | 0.3.1 | MIT | [1](third-party/texts/a65f5d0a945d267751344c95665945b90c030ea107faf5c85d518929886187da.txt) |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [3](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| wasm-bindgen | 0.2.129 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| wasm-bindgen-futures | 0.4.79 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| wasm-bindgen-macro | 0.2.129 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| wasm-bindgen-macro-support | 0.2.129 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| wasm-bindgen-shared | 0.2.129 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| web-sys | 0.3.106 | MIT OR Apache-2.0 | [2](third-party/texts/a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2.txt) |
| web-time | 1.1.0 | MIT OR Apache-2.0 | [2](third-party/texts/54a744942eb3fa63a74d8d359989ed6962c04f3f10ff20cfa08ff8aa7efbe4e6.txt) |
| webpki-roots | 1.0.9 | CDLA-Permissive-2.0 | [1](third-party/texts/e271993808fec50ab29350b39539cdec611a9103f827e0aa26d61da70e2d33f8.txt) |
| windows-core | 0.62.2 | MIT OR Apache-2.0 | [2](third-party/texts/c16f8dcf1a368b83be78d826ea23de4079fe1b4469a0ab9ee20563f37ff3d44b.txt) |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 | [2](third-party/texts/c16f8dcf1a368b83be78d826ea23de4079fe1b4469a0ab9ee20563f37ff3d44b.txt) |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 | [2](third-party/texts/c16f8dcf1a368b83be78d826ea23de4079fe1b4469a0ab9ee20563f37ff3d44b.txt) |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | [2](third-party/texts/c16f8dcf1a368b83be78d826ea23de4079fe1b4469a0ab9ee20563f37ff3d44b.txt) |
| windows-result | 0.4.1 | MIT OR Apache-2.0 | [2](third-party/texts/c16f8dcf1a368b83be78d826ea23de4079fe1b4469a0ab9ee20563f37ff3d44b.txt) |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 | [2](third-party/texts/c16f8dcf1a368b83be78d826ea23de4079fe1b4469a0ab9ee20563f37ff3d44b.txt) |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 | [2](third-party/texts/c16f8dcf1a368b83be78d826ea23de4079fe1b4469a0ab9ee20563f37ff3d44b.txt) |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | [2](third-party/texts/c16f8dcf1a368b83be78d826ea23de4079fe1b4469a0ab9ee20563f37ff3d44b.txt) |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 | [2](third-party/texts/c16f8dcf1a368b83be78d826ea23de4079fe1b4469a0ab9ee20563f37ff3d44b.txt) |
| windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | [2](third-party/texts/c16f8dcf1a368b83be78d826ea23de4079fe1b4469a0ab9ee20563f37ff3d44b.txt) |
| windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 | [2](third-party/texts/c16f8dcf1a368b83be78d826ea23de4079fe1b4469a0ab9ee20563f37ff3d44b.txt) |
| windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 | [2](third-party/texts/c16f8dcf1a368b83be78d826ea23de4079fe1b4469a0ab9ee20563f37ff3d44b.txt) |
| windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 | [2](third-party/texts/c16f8dcf1a368b83be78d826ea23de4079fe1b4469a0ab9ee20563f37ff3d44b.txt) |
| windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 | [2](third-party/texts/c16f8dcf1a368b83be78d826ea23de4079fe1b4469a0ab9ee20563f37ff3d44b.txt) |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 | [2](third-party/texts/c16f8dcf1a368b83be78d826ea23de4079fe1b4469a0ab9ee20563f37ff3d44b.txt) |
| windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | [2](third-party/texts/c16f8dcf1a368b83be78d826ea23de4079fe1b4469a0ab9ee20563f37ff3d44b.txt) |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 | [2](third-party/texts/c16f8dcf1a368b83be78d826ea23de4079fe1b4469a0ab9ee20563f37ff3d44b.txt) |
| winnow | 1.0.4 | MIT | [1](third-party/texts/cb5aedb296c5246d1f22e9099f925a65146f9f0d6b4eebba97fd27a6cdbbab2d.txt) |
| writeable | 0.6.4 | Unicode-3.0 | [1](third-party/texts/f367c1b8e1aa262435251e442901da4607b4650e0e63a026f5044473ecfb90f2.txt) |
| yoke | 0.8.3 | Unicode-3.0 | [1](third-party/texts/f367c1b8e1aa262435251e442901da4607b4650e0e63a026f5044473ecfb90f2.txt) |
| yoke-derive | 0.8.4 | Unicode-3.0 | [1](third-party/texts/f367c1b8e1aa262435251e442901da4607b4650e0e63a026f5044473ecfb90f2.txt) |
| zerofrom | 0.1.8 | Unicode-3.0 | [1](third-party/texts/f367c1b8e1aa262435251e442901da4607b4650e0e63a026f5044473ecfb90f2.txt) |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | [1](third-party/texts/f367c1b8e1aa262435251e442901da4607b4650e0e63a026f5044473ecfb90f2.txt) |
| zeroize | 1.9.0 | Apache-2.0 OR MIT | [2](third-party/texts/cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30.txt) |
| zerotrie | 0.2.5 | Unicode-3.0 | [1](third-party/texts/f367c1b8e1aa262435251e442901da4607b4650e0e63a026f5044473ecfb90f2.txt) |
| zerovec | 0.11.8 | Unicode-3.0 | [1](third-party/texts/f367c1b8e1aa262435251e442901da4607b4650e0e63a026f5044473ecfb90f2.txt) |
| zerovec-derive | 0.11.6 | Unicode-3.0 | [1](third-party/texts/f367c1b8e1aa262435251e442901da4607b4650e0e63a026f5044473ecfb90f2.txt) |
| zmij | 1.0.23 | MIT | [1](third-party/texts/23f18e03dc49df91622fe2a76176497404e46ced8a715d9d2b67a7446571cca3.txt) |

## Optional Python wheel inventory

Signals are unreviewed wheel metadata; full fields and all file references are
in the JSON inventory. Shared wheels are retained once.

| Package | Version | Profiles | License signal | Candidate references |
| --- | --- | --- | --- | --- |
| annotated-doc | 0.0.5 | laya, typellm | MIT | [1](third-party/texts/fff170779a6acbf65abdb405c087f1cee1786691e4a96a4034517e4a504a0cdf.txt) |
| annotated-types | 0.8.0 | laya | MIT | [1](third-party/texts/fe1049884b1a0d9342901e88e07f32925d24b3121d9972b6a6805fb9824b095d.txt) |
| anyio | 4.15.1 | laya, typellm | MIT | [1](third-party/texts/5361ac9dc58f2ef5fd2e9b09c68297c17f04950909bbc8023bdb82eacf22c2b0.txt) |
| certifi | 2026.7.22 | laya, typellm | License :: OSI Approved :: Mozilla Public License 2.0 (MPL 2.0) | [1](third-party/texts/e93716da6b9c0d5a4a1df60fe695b370f0695603d21f6f83f053e42cfc10caf7.txt) |
| click | 8.5.0 | laya, typellm | BSD-3-Clause | [1](third-party/texts/9a8ad106a394e853bfe21f42f4e72d592819a22805d991b5f3275029292b658d.txt) |
| fastapi | 0.141.1 | laya | MIT | [1](third-party/texts/4ec89ffc81485b97fec584b2d4a961032eeffe834453894fd9c1274906cc744e.txt) |
| filelock | 3.32.3 | laya | MIT | [1](third-party/texts/608c89d5060ae9921adccf3236695bc654a9946e12323ef6c021dfa04e294d48.txt) |
| filelock | 4.0.5 | typellm | MIT | [1](third-party/texts/608c89d5060ae9921adccf3236695bc654a9946e12323ef6c021dfa04e294d48.txt) |
| fsspec | 2026.7.0 | laya | BSD-3-Clause | [1](third-party/texts/2dc35496ce53a7307915c008a844aad53e772b49b34cdd00445067691407ec94.txt) |
| fsspec | 2026.9.0 | typellm | BSD-3-Clause | [1](third-party/texts/2dc35496ce53a7307915c008a844aad53e772b49b34cdd00445067691407ec94.txt) |
| h11 | 0.16.0 | laya, typellm | License :: OSI Approved :: MIT License | [1](third-party/texts/37db5bb85926db28a427a25867f10b1232003aea1be69ccb851138adb8e6f361.txt) |
| hf-xet | 1.6.0 | laya, typellm | Apache-2.0 | [1](third-party/texts/c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4.txt) |
| httpcore | 1.0.9 | laya, typellm | BSD-3-Clause | [1](third-party/texts/fdcb59154c74cbaba16a11242f7740bea9f23d6feb5547917d8c5f94a80392a5.txt) |
| httpx | 0.28.1 | laya, typellm | License :: OSI Approved :: BSD License | [1](third-party/texts/4ec59d544f12b5f539a3a716fd321ac58ccd8030b465221f2c880200cdf28d8d.txt) |
| huggingface_hub | 1.33.0 | laya, typellm | License :: OSI Approved :: Apache Software License | [1](third-party/texts/c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4.txt) |
| idna | 3.20 | laya, typellm | BSD-3-Clause | [1](third-party/texts/1a9a4f0e3d479a27240ddd59a9137a66ab4a0f9dfdc8ca6188cc0bfd85187f04.txt) |
| Jinja2 | 3.1.6 | laya, typellm | License :: OSI Approved :: BSD License | [1](third-party/texts/3b49dcee4105eb37bac10faf1be260408fe85d252b8e9df2e0979fc1e094437b.txt) |
| laya | 0.3.20 | laya | License :: OSI Approved :: Apache Software License | [1](third-party/texts/a6cba85bc92e0cff7a450b1d873c0eaa2e9fc96bf472df0247a26bec77bf3ff9.txt) |
| markdown-it-py | 4.2.0 | laya, typellm | License :: OSI Approved :: MIT License | [2](third-party/texts/4a2260d6e2cd0f5a151a1e86dbfe7d3ed552b1e2beabf9941c1ba5c49cbce484.txt) |
| MarkupSafe | 3.0.3 | laya, typellm | BSD-3-Clause | [1](third-party/texts/489a8e1108509ed98a37bb983e11e0f7e1d31f0bd8f99a79c8448e7ff37d07ea.txt) |
| mdurl | 0.1.2 | laya, typellm | License :: OSI Approved :: MIT License | [1](third-party/texts/7c605df6e28667a9603118e98274f64a49ce3eed0d26fccce9534a345e0ef955.txt) |
| mpmath | 1.3.0 | laya | License :: OSI Approved :: BSD License | [1](third-party/texts/c26cae81da4508e5e249985777a33821f183223ebb74d7f8cfbf90fe7eef2fb7.txt) |
| networkx | 3.6.1 | laya | BSD-3-Clause | [1](third-party/texts/3cf7c3a179d817b23b8b8568211769fefdf93797d207d984b8b2fa24d0220b33.txt) |
| numpy | 2.5.3 | laya, typellm | BSD-3-Clause AND 0BSD AND MIT AND Zlib AND CC0-1.0 | [20](third-party/texts/4860083caa0de2ac3292ca98bd074bd8f45d8b32624e37b1e70a240bff61e488.txt) |
| packaging | 26.3 | laya, typellm | Apache-2.0 OR BSD-2-Clause | [3](third-party/texts/cad1ef5bd340d73e074ba614d26f7deaca5c7940c3d8c34852e65c4909686c48.txt) |
| pip | 26.2.1 | bootstrap | MIT | [42](third-party/texts/4eb1fb1705e2def898e81bcba19c3aac80620d27ba3569d6a6df7b4629d1b5c2.txt) |
| pydantic | 2.13.5 | laya | MIT | [1](third-party/texts/a9e186f3ca16b5eef84318e7a701721351a00cb7b8ae3a4394b67b49e3529ef3.txt) |
| pydantic_core | 2.46.5 | laya | MIT | [1](third-party/texts/2afdd30d54b4d62b6f488a6bcc1546e84ec5061f13f4209c03d012348783795a.txt) |
| Pygments | 2.21.0 | laya, typellm | BSD-2-Clause | [2](third-party/texts/71a83872ad82f57be692cf827e048e2cd51d45169cfa1ea3f28706d97a872c0f.txt) |
| python-multipart | 0.0.32 | laya | Apache-2.0 | [1](third-party/texts/cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30.txt) |
| PyYAML | 6.0.3 | laya, typellm | License :: OSI Approved :: MIT License | [1](third-party/texts/8d3928f9dc4490fd635707cb88eb26bd764102a7282954307d3e5167a577e8a4.txt) |
| regex | 2026.9.10 | laya, typellm | Apache-2.0 AND CNRI-Python | [1](third-party/texts/bff55ef4cdcc8c14ce259f8e8ab60e264418440d6335f4dc138273fbd506144d.txt) |
| rich | 15.0.0 | laya, typellm | License :: OSI Approved :: MIT License | [1](third-party/texts/deed7c17a4318158190a3ea239cc879a5a50271cebb98ae7025f48fbe58dca15.txt) |
| safetensors | 0.8.0 | laya, typellm | License :: OSI Approved :: Apache Software License | [1](third-party/texts/c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4.txt) |
| setuptools | 83.0.0 | laya | MIT | [17](third-party/texts/86da0f01aeae46348a3c3d465195dc1ceccde79f79e87769a64b8da04b2a4741.txt) |
| shellingham | 1.5.4 | laya, typellm | License :: OSI Approved :: ISC License (ISCL) | [1](third-party/texts/f388fd38cad13112c1dc0f669bbe80e7f84541edbafb72f3030d2ca7642c3c9d.txt) |
| starlette | 1.7.0 | laya | BSD-3-Clause | [1](third-party/texts/dcb95677a02240243187e964f941847d19b17821cf99e5afae684fab328c19bf.txt) |
| sympy | 1.14.0 | laya | License :: OSI Approved :: BSD License | [3](third-party/texts/82b1038521b0696c63c90b31ed4a0a2c9c4c16871c648b157b07ca44b6f18f8d.txt) |
| tokenizers | 0.23.2 | laya, typellm | License :: OSI Approved :: Apache Software License | [2](third-party/texts/d2d200c0d0c48895b2c3b02cdd8c06c26dea8f9be38ef193e9b4dd5a566c89be.txt) |
| torch | 2.14.0+cpu | laya | Apache-2.0 AND Apache-2.0 WITH LLVM-exception AND BSD-2-Clause AND BSD-3-Clause AND BSL-1.0 AND MIT | [90](third-party/texts/bd018feef8825e88181c84eb7e3aa4eafb8f08a20d9fd6ef948569610c4a3e43.txt) |
| tqdm | 4.70.1 | laya, typellm | MPL-2.0 AND MIT | [1](third-party/texts/fcff87c3a47ce8028a8512aa182d4fcf0ad1c90544ee75cf9b343684cac194de.txt) |
| transformers | 5.17.0 | laya, typellm | Apache 2.0 License | [1](third-party/texts/77fd4710def9ec3c0f6225800e0235f15a425abd4a8b03559127fcd782612049.txt) |
| typellm | 0.2.6 | typellm | Apache-2.0 | [1](third-party/texts/cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30.txt) |
| typer | 0.27.2 | laya, typellm | MIT | [2](third-party/texts/58992cebcf8dfb6e40c4e2112ed12126c243666dca3912a3d78b7ecac4859d49.txt) |
| typing-inspection | 0.4.4 | laya | MIT | [1](third-party/texts/804b59b25f2c31bd278f9202a19ae49a3945aa2664387e2d0a128c7cacc61ec3.txt) |
| typing_extensions | 4.16.0 | laya, typellm | PSF-2.0 | [1](third-party/texts/3b2f81fe21d181c499c59a256c8e1968455d6689d269aa85373bfb6af41da3bf.txt) |
| uvicorn | 0.54.0 | laya | BSD-3-Clause | [1](third-party/texts/efe1acf3e62fb99c288b0ec73e5a773b7268ef4320fe757ea994214e4b63c371.txt) |

## Pinned model cards

- convaiinnovations/laya-typed-decisions at `1a793eb568e6718f15941d08f85432581df534e3`: [retained card](third-party/texts/83e67a995a70a9a9ef6cc1f8c68da654f11eee081c9bb14de07c37b08fcc3113.txt); [upstream](https://huggingface.co/convaiinnovations/laya-typed-decisions/resolve/1a793eb568e6718f15941d08f85432581df534e3/README.md).
- convaiinnovations/laya at `55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851`: [retained card](third-party/texts/00355613796b3f5860b0d5b20392381d26580ec242e04a1fd150c7f116858602.txt); [upstream](https://huggingface.co/convaiinnovations/laya/resolve/55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851/README.md).
