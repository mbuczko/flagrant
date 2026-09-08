fn main() {
    // hugsqlx-derive reads query files via a plain `fs::read_to_string` at macro-expansion
    // time, so Cargo's own staleness check has no idea the crate depends on them - without
    // this, editing a `.sql` file alone wouldn't trigger a rebuild.
    println!("cargo:rerun-if-changed=resources");
}
