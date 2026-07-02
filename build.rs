fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let content =
        std::fs::read_to_string(format!("{dir}/Cargo.toml")).expect("read Cargo.toml");
    let table: toml::Table = toml::from_str(&content).expect("parse Cargo.toml");

    let meta = table
        .get("package")
        .and_then(|p| p.as_table())
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.as_table())
        .and_then(|m| m.get("solmintwatch"))
        .and_then(|s| s.as_table())
        .expect("package.metadata.solmintwatch missing in Cargo.toml");

    for (key, value) in meta {
        let env_key = format!(
            "SOLMINTWATCH_{}",
            key.replace('-', "_").to_uppercase()
        );
        let env_val = match value {
            toml::Value::String(s) => s.clone(),
            toml::Value::Boolean(b) => b.to_string(),
            toml::Value::Integer(i) => i.to_string(),
            other => panic!("unsupported metadata type for {key}: {other:?}"),
        };
        println!("cargo:rustc-env={env_key}={env_val}");
    }

    println!("cargo:rerun-if-changed=Cargo.toml");
}
