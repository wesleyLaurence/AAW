//! Generates the Swift bindings from the built library; see apps/mac/build.sh.

fn main() {
    uniffi::uniffi_bindgen_main()
}
