#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinkConfiguration {
    Static,
    Dynamic,
}

fn get_nvlib_name(lib: &str, conf: LinkConfiguration) -> String {
    if lib == "culibos" && conf == LinkConfiguration::Static {
        return String::from("static=culibos");
    }

    match conf {
        LinkConfiguration::Static => format!("static={}_static", lib),
        LinkConfiguration::Dynamic => format!("dylib={}", lib),
    }
}

#[derive(Clone, Copy, Debug)]
enum TRTVersion {
    V5,
    V6,
    V7,
}

impl TRTVersion {
    fn get_flag(&self) -> &str {
        match self {
            Self::V5 => "TRT5",
            Self::V6 => "TRT6",
            Self::V7 => "TRT7",
        }
    }

    fn get_cmake_config(&self) -> cmake::Config {
        let mut cfg = cmake::Config::new("trt-sys");
        cfg.define(self.get_flag(), "");
        cfg
    }

    fn get_bindings(&self) -> Result<bindgen::Bindings, ()> {
        bindgen::builder()
            .clang_arg(&format!("-D{}", self.get_flag()))
            .clang_args(&["-x", "c++"])
            .header("trt-sys/tensorrt_api.h")
            .size_t_is_usize(true)
            .generate()
    }
}

fn myelin_configuration(conf: LinkConfiguration) {
    // Don't actually need to link specifically for dynamic linking
    if conf == LinkConfiguration::Static {
        println!("cargo:rustc-link-lib=static=myelin_compiler_static");
        println!("cargo:rustc-link-lib=static=myelin_pattern_library_static");
        println!("cargo:rustc-link-lib=static=myelin_pattern_runtime_static");
        println!("cargo:rustc-link-lib=static=myelin_executor_static");
    }
}

fn cuda_configuration(conf: LinkConfiguration) {
    let cudadir = option_env!("CUDA_INSTALL_DIR").unwrap_or("/usr/local/cuda");

    println!("cargo:rustc-link-search={}/lib64", cudadir);

    if conf == LinkConfiguration::Static {
        println!("cargo:rustc-link-lib=static=cudnn_adv_train_static_v8");
        println!("cargo:rustc-link-lib=static=cudnn_ops_train_static_v8");
    }

    for lib in &["cudart", "cublas", "cublasLt", "cudnn", "culibos"] {
        println!("cargo:rustc-link-lib={}", get_nvlib_name(lib, conf));
    }
}

fn tensorrt_configuration(conf: LinkConfiguration) {
    if let Some(trt_lib_dir) = option_env!("TRT_INSTALL_DIR") {
        println!("cargo:rustc-link-search={}", trt_lib_dir);
    }

    // Don't actually need to link specifically for dynamic linking
    if conf == LinkConfiguration::Static {
        println!("cargo:rustc-link-lib=dylib=onnx_proto");
        println!("cargo:rustc-link-lib=dylib=nvrtc");

        if let Some(proto_lib_dir) = option_env!("PROTOBUF_DIR") {
            println!("cargo:rustc-link-search={}", proto_lib_dir);
        }
        // HACK: Nvidia uses namespace `google_private` instead of default
        // `google`, so you need to rebuild protobuf specifically because of that
        //
        // check out:
        //   https://forums.developer.nvidia.com/t/tensorrt-6-static-library-undefined-symbols/82312/3
        //
        // wget https://github.com/protocolbuffers/protobuf/releases/download/v3.0.0/protobuf-cpp-3.0.0.tar.gz
        // tar xf protobuf-cpp-3.0.0.tar.gz
        // cd protobuf-3.0.0
        // CXXFLAAGS=-Dgoogle=google_private ./configure --disable-shared
        // make -j8
        // mv ./src/.libs/libprotobuf.a ./src/.libs/libprotobuf_private.a
        // export PROTOBUF_DIR=./src/.libs/
        //
        println!("cargo:rustc-link-lib=static=protobuf_private");
    }

    for lib in &["nvinfer", "nvonnxparser", "nvparsers", "nvinfer_plugin"] {
        println!("cargo:rustc-link-lib={}", get_nvlib_name(lib, conf));
    }
}

// Not sure if I love this solution but I think it's relatively robust enough for now on Unix systems.
// Still have to thoroughly test what happens with a TRT library installed that's not done by the
// dpkg. It's possible that we'll just have to fall back to only supporting one system library and assuming that
// the user has the correct library installed and is viewable via ldconfig.
//
// Hopefully something like this will work for Windows installs as well, not having a default library
// install location will make that significantly harder.
//
fn main() -> Result<(), ()> {
    #[cfg(feature = "trt-5")]
    let version = TRTVersion::V5;
    #[cfg(feature = "trt-6")]
    let version = TRTVersion::V6;
    #[cfg(feature = "trt-7")]
    let version = TRTVersion::V7;

    let bindings = version.get_bindings()?;
    bindings.write_to_file("src/bindings.rs").unwrap();

    let dst = version.get_cmake_config().build();
    println!("cargo:rustc-link-search=native={}", dst.display());
    println!("cargo:rustc-link-lib=static=trt-sys");

    let link_conf = if cfg!(feature = "static") {
        LinkConfiguration::Static
    } else {
        LinkConfiguration::Dynamic
    };

    // I guess you can link libstdc++ statically
    // https://stackoverflow.com/questions/16629855/is-it-legal-to-statically-link-libstdc-and-libgcc-in-a-binary-only-application/16634179
    let lib_type = match link_conf {
        LinkConfiguration::Static => "static-nobundle",
        LinkConfiguration::Dynamic => "dylib",
    };
    println!("cargo:rustc-link-lib={}=stdc++", lib_type);

    tensorrt_configuration(link_conf);
    cuda_configuration(link_conf);
    myelin_configuration(link_conf);

    Ok(())
}
