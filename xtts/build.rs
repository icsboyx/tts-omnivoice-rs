use std::env;

fn main() {
    // Set only when xomnivoice links a library from OMNIVOICE_LIB_DIR; a system-wide
    // installation is found by the dynamic linker without an rpath.
    if let Ok(library_dir) = env::var("DEP_OMNIVOICE_LIB_DIR") {
        println!("cargo:rustc-link-arg-tests=-Wl,-rpath,{library_dir}");
    }
}
