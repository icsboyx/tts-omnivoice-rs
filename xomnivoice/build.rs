use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

const LIBRARY: &str = "libomnivoice.so";
const INSTALL_HINT: &str = "see xomnivoice/README.md#install-the-native-library";

fn main() {
    println!("cargo:rerun-if-env-changed=OMNIVOICE_LIB_DIR");

    let project = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let header = project.join("include/omnivoice.h");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    println!("cargo:rerun-if-changed={}", header.display());

    // docs.rs has no native library; documentation only needs the bindings.
    if env::var_os("DOCS_RS").is_some() {
        generate_bindings(&header, &out_dir);
        return;
    }

    // An explicit directory always wins over a system-wide installation.
    if let Some(directory) = env::var_os("OMNIVOICE_LIB_DIR") {
        let library_dir = PathBuf::from(directory)
            .canonicalize()
            .unwrap_or_else(|error| panic!("OMNIVOICE_LIB_DIR does not exist: {error}"));
        assert!(
            library_dir.join(LIBRARY).is_file(),
            "OMNIVOICE_LIB_DIR must contain {LIBRARY}: {}",
            library_dir.display()
        );
        println!("cargo:rustc-link-search=native={}", library_dir.display());
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", library_dir.display());
        println!("cargo::metadata=lib_dir={}", library_dir.display());
    } else {
        println!("cargo:rerun-if-changed=/etc/ld.so.cache");
        let library = system_library().unwrap_or_else(|| {
            panic!(
                "{LIBRARY} is not installed system-wide and OMNIVOICE_LIB_DIR is not set; {INSTALL_HINT}"
            )
        });
        let library_dir = library.parent().expect("library path has a directory");
        println!("cargo:rustc-link-search=native={}", library_dir.display());
    }
    println!("cargo:rustc-link-lib=dylib=omnivoice");
    // Device selection reads the GGML device registry directly.
    println!("cargo:rustc-link-lib=dylib=ggml");
    println!("cargo:rustc-link-lib=dylib=ggml-base");

    generate_bindings(&header, &out_dir);
}

fn generate_bindings(header: &Path, out_dir: &Path) {
    bindgen::Builder::default()
        .header(header.to_string_lossy())
        .allowlist_function("ov_.*")
        .allowlist_type("ov_.*")
        .allowlist_var("OV_.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("cannot generate OmniVoice bindings; install libclang")
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("cannot write OmniVoice bindings");
}

/// Looks the library up in the dynamic linker cache, so executables also find it at
/// runtime without an rpath.
fn system_library() -> Option<PathBuf> {
    let output = ["ldconfig", "/sbin/ldconfig", "/usr/sbin/ldconfig"]
        .iter()
        .find_map(|program| {
            Command::new(program)
                .arg("-p")
                .output()
                .ok()
                .filter(|output| output.status.success())
        })?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| {
            // Format: "\tlibomnivoice.so (libc6,x86-64) => /usr/local/lib/libomnivoice.so"
            let (name, path) = line.trim().split_once(" => ")?;
            let path = PathBuf::from(path);
            (name.split_whitespace().next() == Some(LIBRARY) && path.is_file()).then_some(path)
        })
}
