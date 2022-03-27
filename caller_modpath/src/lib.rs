#![feature(proc_macro_span)]
//! This is an overhaul of [repo](https://github.com/Shizcow/caller_modpath).

// yeah
extern crate proc_macro;

#[doc(hidden)]
pub use quote::quote;
pub use quote::quote_spanned;

use std::path::PathBuf;
use std::sync::RwLock;
use uuid::Uuid;

// use when we call rustc on ourself (this lib gets wild)
#[doc(hidden)]
pub static UUID_ENV_VAR_NAME: &str =
    concat!("CARGO_INJECT_", env!("CARGO_PKG_NAME"), "_SECOND_PASS_UUID");

// so Span is a really special type
// It is very dumb and implements no useful traits (Eq, Hash, Send, Sync, etc)
// A lot of this stuff is crazy because of that
// If this was better I'd stick it in a lazy_static HashMap and call it a day but sometype needs attention
thread_local! {
    // Span, crate name, caller function
    static MODCACHE: RwLock<Vec<(proc_macro2::Span, &'static str, String)>> = RwLock::new(vec![]);
}

#[doc(hidden)]
pub fn generate_paths() -> proc_macro::TokenStream {
    let i = proc_macro2::Ident::new(
        &format!(
            "{}_UUID_{}",
            env!("CARGO_PKG_NAME"),
            std::env::var(UUID_ENV_VAR_NAME).unwrap(),
        ).as_str(),
        proc_macro2::Span::call_site(),
    );
    (quote! {
        static #i: &'static str = module_path!();
    })
        .into()
}

#[doc(hidden)]
pub fn append_span(client_proc_macro_crate_name: &'static str, fn_name: &String) {
    // Make sure we aren't logging the call site twice
    let call_site = proc_macro2::Span::call_site().unwrap();
    let already_calculated = MODCACHE.with(|m| {
        let locked = m.read().unwrap();
        for i in 0..locked.len() {
            if locked[i].0.unwrap().eq(&call_site) {
                return true;
            }
        }
        false
    });
    if already_calculated {
        return;
    }

    MODCACHE.with(move |m| {
        m.write().unwrap().push((
            proc_macro2::Span::call_site(),
            client_proc_macro_crate_name,
            fn_name.clone()
        ))
    });
}

pub fn get_modpaths(client_proc_macro_crate_name: &str) -> Vec<String> {
    let mut modpaths: Vec<String> = vec![];

    // Get entrypoint for this crate
    let entry_p = get_entrypoint();

    // Get the library binary
    let chosen_dir = find_lib_binary(&client_proc_macro_crate_name);
    let liblink_path = format!("{}={}", client_proc_macro_crate_name, chosen_dir);

    // Supply arguments for second compilation
    let rustc_args = vec![
        "-Z",
        "unpretty=expanded",
        "-Z",
        "unstable-options",
        "--edition=2021",
        "--color=never",
        "--extern",
        &liblink_path,
        entry_p.to_str().unwrap(),
    ];

    // Create the UUID for splitting our compilation output
    let uuid_string: String = Uuid::new_v4().to_string().replace("-", "_");

    // Compile the crate while generating the module paths (gen_second_pass)
    let proc = std::process::Command::new("rustc")
        .current_dir(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .args(&rustc_args)
        .env(UUID_ENV_VAR_NAME, &uuid_string)
        .output()
        .expect("failed to execute a second pass of rustc");

    // Split the output by our environment variable (find each module path reference)
    let string = String::from_utf8_lossy(&proc.stdout);
    let split_string = string.split(&uuid_string);

    MODCACHE.with(|m| {
        // If this is our crate, increase number of module paths
        for (i, e) in m.read().unwrap().iter().enumerate() {
            if e.1 == client_proc_macro_crate_name {
                // Hunt down the module path per UUID found
                let mut split = split_string.clone();
                let mut path: String = split.nth(i + 1)
                    .unwrap_or_else(|| panic!("Failed to find internal UUID; rustc metacall probably faliled. Called as `rustc {}`. Stderr:\n{}", rustc_args.join(" "), String::from_utf8_lossy(&proc.stderr)))
                    .chars()
                    .skip_while(|c| c != &'"')
                    .skip(1)
                    .take_while(|c| c != &'"')
                    .collect();

                // Skip the root module
                let root_module = path.find("::").unwrap();
                path = path.chars().skip(root_module + 2).collect();

                // Add the function
                modpaths.push(
                    path + "::" + e.2.as_str()
                );
            }
        }
    });


    return modpaths;
}

fn get_entrypoint() -> PathBuf {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    if let Ok(bin_name) = std::env::var("CARGO_BIN_NAME") {
        // binary: need to parse targets in Cargo.toml to find the correct path

        let manifest = cargo_manifest::Manifest::from_path(manifest_dir.join("Cargo.toml"))
            .expect("Could not parse Cargo.toml of caller");

        let rustc_entry = manifest.bin.unwrap().into_iter().find(|target| target.name.as_ref() == Some(&bin_name)).expect("Could not get binary target path from Cargo.toml. If you are manually specifying targets, make sure the path is included as well.").path.unwrap();

        manifest_dir.join(rustc_entry)
    } else {
        // just a library: can assume it's just src/lib.rs
        manifest_dir.join("src").join("lib.rs")
    }
}

fn find_lib_binary(libname: &str) -> String {
    let target_path = std::env::current_dir()
        .expect("Could not get current dir from env")
        .join("target")
        .join(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        });

    let lib_extension = if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };

    // need to look in two places:
    // target/{}/deps/ for crate dependencies
    let dep_p = target_path
        .join("deps")
        .join(format!("lib{}-*.{}", libname, lib_extension))
        .into_os_string();

    let dep_str = dep_p.to_string_lossy();

    // and target/{}/ for workspace target
    let t_p = target_path.join(format!("lib{}.{}", libname, lib_extension));

    let mut file_candidates: Vec<_> = glob::glob(&dep_str)
        .expect("Failed to read library glob pattern")
        .into_iter()
        .filter_map(|entry| entry.ok())
        .collect();

    file_candidates.push(t_p);

    let fstr = file_candidates
        .iter()
        .map(|p| p.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");

    file_candidates
        .into_iter()
        .filter_map(|entry| {
            std::fs::metadata(&entry)
                .and_then(|f| f.accessed())
                .ok()
                .map(|t| (entry, t))
        })
        .max()
        .map(|(f, _)| f)
        .unwrap_or_else(|| {
            panic!(
                "Could not find suitable backend library paths from file list {}",
                fstr
            )
        })
        .into_os_string()
        .to_string_lossy()
        .to_string()
}
