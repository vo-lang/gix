format = 1
module = "github.com/vo-lang/gix"
version = "0.1.0"
vo = "0.1.0"

[extension]
name = "gix"

[extension.native]
library = "vo_gix"
targets = ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"]

[build.native]
kind = "cargo"
manifest = "rust/Cargo.toml"
package = "vo-gix"
