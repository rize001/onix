use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=ui/app-window.slint");
    println!("cargo:rerun-if-changed=assets/onix-logo.svg");

    let source_path = PathBuf::from("ui/app-window.slint");
    let mut source = fs::read_to_string(&source_path).expect("cannot read Slint UI");
    for (from, to) in [
        ("clicked => root.register-mode = false;", "clicked => { root.register-mode = false; }"),
        ("clicked => root.register-mode = true;", "clicked => { root.register-mode = true; }"),
        ("clicked => root.menu-pinned = !root.menu-pinned;", "clicked => { root.menu-pinned = !root.menu-pinned; }"),
        ("clicked => root.modal-kind = \"group\";", "clicked => { root.modal-kind = \"group\"; }"),
        ("clicked => root.filter-name = \"all\";", "clicked => { root.filter-name = \"all\"; }"),
        ("clicked => root.filter-name = \"private\";", "clicked => { root.filter-name = \"private\"; }"),
        ("clicked => root.filter-name = \"group\";", "clicked => { root.filter-name = \"group\"; }"),
        ("clicked => root.filter-name = \"channel\";", "clicked => { root.filter-name = \"channel\"; }"),
        ("clicked => root.modal-kind = \"\";", "clicked => { root.modal-kind = \"\"; }"),
    ] {
        source = source.replace(from, to);
    }

    let generated = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR missing")).join("app-window.generated.slint");
    fs::write(&generated, source).expect("cannot write generated Slint UI");
    slint_build::compile(generated).expect("Slint UI compilation failed");
}
