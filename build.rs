fn main() {
    println!("cargo:rerun-if-changed=assets/redbim.ico");
    println!("cargo:rerun-if-changed=assets/redbim.rc");
    let major = env!("CARGO_PKG_VERSION_MAJOR");
    let minor = env!("CARGO_PKG_VERSION_MINOR");
    let patch = env!("CARGO_PKG_VERSION_PATCH");
    let out_dir = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(
        out_dir.join("redbim_version.h"),
        format!("#define VERSION_PARTS {major},{minor},{patch},0\n#define VERSION_STRING \"{major}.{minor}.{patch}\"\n"),
    )
    .expect("could not write the Windows resource version");
    embed_resource::compile("assets/redbim.rc", embed_resource::NONE)
        .manifest_required()
        .expect("could not compile RedBIM Windows resources");
}
