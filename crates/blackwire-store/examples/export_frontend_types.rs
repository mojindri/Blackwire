use blackwire_store::CoreSettings;
use ts_rs::TS;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_var("TS_RS_LARGE_INT", "number");
    let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../black-ui/frontend/src/generated");
    if output.exists() {
        std::fs::remove_dir_all(&output)?;
    }
    std::fs::create_dir_all(&output)?;
    CoreSettings::export_all_to(output)?;
    for entry in std::fs::read_dir(output)? {
        let path = entry?.path();
        if path.extension().and_then(std::ffi::OsStr::to_str) != Some("ts") {
            continue;
        }
        let contents = std::fs::read_to_string(&path)?;
        let normalized = contents
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(path, format!("{normalized}\n"))?;
    }
    Ok(())
}
