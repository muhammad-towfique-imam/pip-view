//! Embeds the Windows manifest: Common Controls 6 (visual styles) and
//! per-monitor-v2 DPI awareness.

fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        use embed_manifest::manifest::DpiAwareness;
        embed_manifest::embed_manifest(
            embed_manifest::new_manifest("webcam-view-pip").dpi_awareness(DpiAwareness::PerMonitorV2),
        )
        .expect("embed the Windows manifest");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
