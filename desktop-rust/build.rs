use std::{fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=ui/app-window.slint");
    println!("cargo:rerun-if-changed=ui/login-v162.fragment");
    println!("cargo:rerun-if-changed=assets/onix-logo.svg");

    let mut source = fs::read_to_string("ui/app-window.slint").expect("cannot read Slint UI");
    source = source.replace("\r\n", "\n");
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
        ("property <bool> menu-open: root.menu-pinned || menu-trigger.has-hover || side-hover.has-hover;", "property <bool> menu-open: root.menu-pinned;"),
        ("width: root.menu-open ? 294px : 72px;", "property <bool> expanded: root.menu-pinned; width: self.expanded ? 294px : 72px;"),
        ("side-hover := TouchArea { }", "side-hover := TouchArea { }\n                    states [ hovered when side-hover.has-hover: { side.expanded: true; } ]"),
        ("root.menu-open", "side.expanded"),
        ("height: visible ? 82px : 0px;", "height: self.visible ? 82px : 0px;"),
        ("for msg in root.messages: HorizontalLayout { alignment: msg.mine ? end : start; height: bubble.height;", "for msg in root.messages: HorizontalLayout { alignment: msg.mine ? end : start; height: 92px;"),
        ("bubble := Rectangle { width: min(590px, max(220px, msg.body.length * 8px)); min-height: 74px;", "bubble := Rectangle { width: 520px; height: 82px;"),
        ("(parent.width-self.width)/2", "(parent.width - self.width) / 2"),
        ("(parent.height-self.height)/2", "(parent.height - self.height) / 2"),
        ("parent.width-32px", "parent.width - 32px"),
        ("parent.height-24px", "parent.height - 24px"),
    ] {
        source = source.replace(from, to);
    }

    let old_start = "    Rectangle {\n        background: @radial-gradient(circle, #6d47ff38 0%, transparent 48%);";
    let old_end = "\n    if root.authenticated: app-page := Rectangle {";
    let start = source.find(old_start).expect("old login start not found");
    let end = source[start..].find(old_end).map(|offset| start + offset).expect("old login end not found");
    let login = fs::read_to_string("ui/login-v162.fragment").expect("cannot read v162 login fragment").replace("\r\n", "\n");
    source.replace_range(start..end, login.trim_end());
    source = source.replace("export component AppWindow inherits Window", "export component OnixWindow inherits Window");

    let generated = PathBuf::from("ui/app-window.generated.slint");
    fs::write(&generated, source).expect("cannot write generated Slint UI");
    slint_build::compile(generated).expect("Slint UI compilation failed");
}
