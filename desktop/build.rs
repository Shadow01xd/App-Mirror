fn main() {
    slint_build::compile("src/ui/layouts/desktop-shell.slint")
        .expect("failed to compile the Slint desktop shell");
}
