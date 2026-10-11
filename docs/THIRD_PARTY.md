# 第三方依赖

本项目通过 Cargo 使用下列依赖，没有复制或修改这些库的源码。实际版本由 Cargo.lock 固定。依赖包保留各自的版权和许可文件；发行包含第三方代码的二进制时，也须保留相应许可要求的声明。

文件处理使用 jomini、walkdir、csv、image 和 thiserror；命令行使用 clap；存档使用 serde 与 serde_json，内容摘要使用 sha2；确定性随机状态使用 rand_chacha；Godot 扩展使用 godot-rust；测试使用 tempfile。下表包含它们的间接依赖和其他目标平台的依赖，依据锁定版本的包元数据整理。

Godot 引擎本身采用 MIT 许可，不随本仓库分发。若打包引擎，请同时保留 [Godot 版权与第三方声明](https://godotengine.org/license/)。

| 名称 | 版本 | 声明许可 | 来源 |
| --- | --- | --- | --- |
| anstream | 1.0.0 | MIT OR Apache-2.0 | [上游](https://github.com/rust-cli/anstyle.git) |
| anstyle | 1.0.14 | MIT OR Apache-2.0 | [上游](https://github.com/rust-cli/anstyle.git) |
| anstyle-parse | 1.0.0 | MIT OR Apache-2.0 | [上游](https://github.com/rust-cli/anstyle.git) |
| anstyle-query | 1.1.5 | MIT OR Apache-2.0 | [上游](https://github.com/rust-cli/anstyle.git) |
| anstyle-wincon | 3.0.11 | MIT OR Apache-2.0 | [上游](https://github.com/rust-cli/anstyle.git) |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | [上游](https://github.com/cuviper/autocfg) |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | [上游](https://github.com/bitflags/bitflags) |
| block-buffer | 0.12.1 | MIT OR Apache-2.0 | [上游](https://github.com/RustCrypto/utils) |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | [上游](https://github.com/Lokathor/bytemuck) |
| byteorder-lite | 0.1.0 | Unlicense OR MIT | [上游](https://github.com/image-rs/byteorder-lite) |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | [上游](https://github.com/rust-lang/cfg-if) |
| clap | 4.6.7 | MIT OR Apache-2.0 | [上游](https://github.com/clap-rs/clap) |
| clap_builder | 4.6.7 | MIT OR Apache-2.0 | [上游](https://github.com/clap-rs/clap) |
| clap_derive | 4.6.7 | MIT OR Apache-2.0 | [上游](https://github.com/clap-rs/clap) |
| clap_lex | 1.1.1 | MIT OR Apache-2.0 | [上游](https://github.com/clap-rs/clap) |
| colorchoice | 1.0.5 | MIT OR Apache-2.0 | [上游](https://github.com/rust-cli/anstyle.git) |
| const-oid | 0.10.2 | Apache-2.0 OR MIT | [上游](https://github.com/RustCrypto/formats) |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 | [上游](https://github.com/RustCrypto/utils) |
| crypto-common | 0.2.2 | MIT OR Apache-2.0 | [上游](https://github.com/RustCrypto/traits) |
| csv | 1.4.0 | Unlicense/MIT | [上游](https://github.com/BurntSushi/rust-csv) |
| csv-core | 0.1.13 | Unlicense/MIT | [上游](https://github.com/BurntSushi/rust-csv) |
| digest | 0.11.3 | MIT OR Apache-2.0 | [上游](https://github.com/RustCrypto/traits) |
| errno | 0.3.14 | MIT OR Apache-2.0 | [上游](https://github.com/lambda-fairy/rust-errno) |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | [上游](https://github.com/smol-rs/fastrand) |
| gdextension-api | 0.5.1 | MPL-2.0 | [上游](https://github.com/godot-rust/gdext) |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | [上游](https://github.com/rust-random/getrandom) |
| glam | 0.32.1 | MIT OR Apache-2.0 | [上游](https://github.com/bitshifter/glam-rs) |
| godot | 0.5.5 | MPL-2.0 | [上游](https://github.com/godot-rust/gdext) |
| godot-bindings | 0.5.5 | MPL-2.0 | [上游](https://github.com/godot-rust/gdext) |
| godot-cell | 0.5.5 | MPL-2.0 | [上游](https://github.com/godot-rust/gdext) |
| godot-codegen | 0.5.5 | MPL-2.0 | [上游](https://github.com/godot-rust/gdext) |
| godot-core | 0.5.5 | MPL-2.0 | [上游](https://github.com/godot-rust/gdext) |
| godot-ffi | 0.5.5 | MPL-2.0 | [上游](https://github.com/godot-rust/gdext) |
| godot-macros | 0.5.5 | MPL-2.0 | [上游](https://github.com/godot-rust/gdext) |
| heck | 0.5.0 | MIT OR Apache-2.0 | [上游](https://github.com/withoutboats/heck) |
| hybrid-array | 0.4.15 | MIT OR Apache-2.0 | [上游](https://github.com/RustCrypto/hybrid-array) |
| image | 0.25.10 | MIT OR Apache-2.0 | [上游](https://github.com/image-rs/image) |
| is_terminal_polyfill | 1.70.2 | MIT OR Apache-2.0 | [上游](https://github.com/polyfill-rs/is_terminal_polyfill) |
| itoa | 1.0.18 | MIT OR Apache-2.0 | [上游](https://github.com/dtolnay/itoa) |
| jomini | 0.37.1 | MIT | [上游](https://github.com/rakaly/jomini) |
| libc | 0.2.190 | MIT OR Apache-2.0 | [上游](https://github.com/rust-lang/libc) |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [上游](https://github.com/sunfishcode/linux-raw-sys) |
| memchr | 2.8.3 | Unlicense OR MIT | [上游](https://github.com/BurntSushi/memchr) |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 | [上游](https://github.com/awxkee/moxcms.git) |
| nanoserde | 0.2.1 | MIT OR Apache-2.0 | [上游](https://github.com/not-fl3/nanoserde) |
| nanoserde-derive | 0.2.1 | MIT | [上游](https://crates.io/crates/nanoserde-derive/0.2.1) |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | [上游](https://github.com/rust-num/num-traits) |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | [上游](https://github.com/matklad/once_cell) |
| once_cell_polyfill | 1.70.2 | MIT OR Apache-2.0 | [上游](https://github.com/polyfill-rs/once_cell_polyfill) |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 | [上游](https://github.com/cryptocorrosion/cryptocorrosion) |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | [上游](https://github.com/dtolnay/proc-macro2) |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 | [上游](https://github.com/awxkee/pxfm) |
| quote | 1.0.47 | MIT OR Apache-2.0 | [上游](https://github.com/dtolnay/quote) |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | [上游](https://github.com/r-efi/r-efi) |
| rand_chacha | 0.10.0 | MIT OR Apache-2.0 | [上游](https://github.com/rust-random/rand) |
| rand_core | 0.10.1 | MIT OR Apache-2.0 | [上游](https://github.com/rust-random/rand_core) |
| rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [上游](https://github.com/bytecodealliance/rustix) |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 | [上游](https://github.com/dtolnay/ryu) |
| same-file | 1.0.6 | Unlicense/MIT | [上游](https://github.com/BurntSushi/same-file) |
| serde | 1.0.229 | MIT OR Apache-2.0 | [上游](https://github.com/serde-rs/serde) |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | [上游](https://github.com/serde-rs/serde) |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | [上游](https://github.com/serde-rs/serde) |
| serde_json | 1.0.152 | MIT OR Apache-2.0 | [上游](https://github.com/serde-rs/json) |
| sha2 | 0.11.0 | MIT OR Apache-2.0 | [上游](https://github.com/RustCrypto/hashes) |
| strsim | 0.11.1 | MIT | [上游](https://github.com/rapidfuzz/strsim-rs) |
| syn | 2.0.119 | MIT OR Apache-2.0 | [上游](https://github.com/dtolnay/syn) |
| syn | 3.0.7 | MIT OR Apache-2.0 | [上游](https://github.com/dtolnay/syn) |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | [上游](https://github.com/Stebalien/tempfile) |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | [上游](https://github.com/dtolnay/thiserror) |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | [上游](https://github.com/dtolnay/thiserror) |
| typenum | 1.20.1 | MIT OR Apache-2.0 | [上游](https://github.com/paholg/typenum) |
| unicode-ident | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | [上游](https://github.com/dtolnay/unicode-ident) |
| utf8parse | 0.2.2 | Apache-2.0 OR MIT | [上游](https://github.com/alacritty/vte) |
| venial | 0.6.1 | MIT | [上游](https://github.com/PoignardAzur/venial) |
| walkdir | 2.5.0 | Unlicense/MIT | [上游](https://github.com/BurntSushi/walkdir) |
| winapi-util | 0.1.11 | Unlicense OR MIT | [上游](https://github.com/BurntSushi/winapi-util) |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | [上游](https://github.com/microsoft/windows-rs) |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | [上游](https://github.com/microsoft/windows-rs) |
| zerocopy | 0.8.62 | BSD-2-Clause OR Apache-2.0 OR MIT | [上游](https://github.com/google/zerocopy) |
| zerocopy-derive | 0.8.62 | BSD-2-Clause OR Apache-2.0 OR MIT | [上游](https://github.com/google/zerocopy) |
| zmij | 1.0.23 | MIT | [上游](https://github.com/dtolnay/zmij) |
