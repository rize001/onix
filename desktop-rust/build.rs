fn main() {
    println!("cargo:rerun-if-changed=ui/app-window.slint");
    println!("cargo:rerun-if-changed=assets/onix-logo.svg");
    slint_build::compile("ui/app-window.slint").expect("Slint UI compilation failed");
}
