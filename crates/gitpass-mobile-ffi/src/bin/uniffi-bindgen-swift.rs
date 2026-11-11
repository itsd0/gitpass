// Host-only helper binary: generates the Swift bindings, C header and
// XCFramework-compatible modulemap from a built library. Not built for
// iOS targets (the build script uses `cargo build --lib` for those).
fn main() {
    uniffi::uniffi_bindgen_swift()
}
