extern crate cc;
use glob::glob;

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-link-search={}", out_dir);
    cc::Build::new()
        .files(
            glob("./vendor/mruby-compiler2/src/**/*.c")
                .expect("cannot find c source")
                .map(|x| x.unwrap()),
        )
        .files(
            glob("./vendor/mruby-compiler2/lib/prism/src/**/*.c")
                .expect("cannot find c source")
                .map(|x| x.unwrap()),
        )
        .warnings(false)
        .define("MRB_NO_PRESYM", "")
        .define("MRB_INT64", "1")
        .define("PRISM_XALLOCATOR", "")
        .define("PRISM_BUILD_MINIMAL", "")
        .define("PICORB_VM_MRUBYC", "")
        .define("MRBC_ALLOC_LIBC", "")
        .include("./vendor/include")
        .include("./vendor/mruby-compiler2/include")
        .include("./vendor/mruby-compiler2/lib/prism/include")
        .flag("-fPIC")
        .flag("-c")
        .compile("mrubycompiler2");

    println!("cargo:rustc-link-lib=mrubycompiler2");
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let mut builder = bindgen::Builder::default()
        .header("./vendor/mruby-compiler2/include/mruby_compiler.h")
        .header("./vendor/mruby-compiler2/include/mrc_codedump.h")
        .clang_arg("-I./vendor/mruby-compiler2/include")
        .clang_arg("-I./vendor/mruby-compiler2/lib/prism/include")
        .blocklist_item("FP_NAN")
        .blocklist_item("FP_INFINITE")
        .blocklist_item("FP_ZERO")
        .blocklist_item("FP_SUBNORMAL")
        .blocklist_item("FP_NORMAL")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    if target_arch == "wasm32" {
        builder = builder.blocklist_type("max_align_t");
    }
    let bindings = builder.generate().expect("Unable to generate bindings");

    let out = std::path::PathBuf::from(out_dir).join("bindings.rs");
    bindings
        .write_to_file(out)
        .expect("Couldn't write bindings!");
}
