use std::env;

fn main() {
    // Set only when xomnivoice links a library from OMNIVOICE_LIB_DIR.
    if let Ok(native_lib_dir) = env::var("DEP_OMNIVOICE_LIB_DIR") {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,{native_lib_dir}");
    }
}
