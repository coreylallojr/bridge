// Test executables of this crate link Tauri (and use SetWindowSubclass), which import functions that only
// Common Controls v6 exports by name (TaskDialogIndirect, SetWindowSubclass, ...). Without a manifest asking for v6
// they fail to start with STATUS_ENTRYPOINT_NOT_FOUND. `rustc-link-arg` only reaches this package's own linked
// targets (its tests), never the Bridge app, which gets its manifest from tauri-build.
fn main() {
    println!("cargo:rerun-if-changed=tests-manifest.rc");
    println!("cargo:rerun-if-changed=tests.manifest");
    embed_resource::compile_for_everything("tests-manifest.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
