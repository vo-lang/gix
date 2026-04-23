module github.com/vo-lang/gix

vo ^0.1.0

[extension]
name = "gix"

[extension.native]
path = "rust/target/{profile}/libvo_gix"

[[extension.native.targets]]
target = "aarch64-apple-darwin"
library = "libvo_gix.dylib"

[[extension.native.targets]]
target = "x86_64-unknown-linux-gnu"
library = "libvo_gix.so"
