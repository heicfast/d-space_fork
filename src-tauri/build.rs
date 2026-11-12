// SPDX-License-Identifier: GPL-3.0-or-later
// DiskWizard build script — Tauri codegen + Windows application manifest.
//
// The manifest embeds `requireAdministrator`: the engine needs raw volume
// access (\\.\X:), MFT parsing, deleted-file recovery, VSS and volume
// repair, all of which require elevation. The OS shows the UAC prompt at
// launch instead of every scan failing with ACCESS_DENIED.
fn main() {
    #[allow(unused_mut)]
    let mut attrs = tauri_build::Attributes::default();
    #[cfg(windows)]
    {
        attrs = attrs.windows_attributes(
            tauri_build::WindowsAttributes::new().app_manifest(include_str!("manifest.xml")),
        );
    }
    tauri_build::try_build(attrs).expect("failed to run tauri-build");
}
