// STD
use std::env::set_var;
use std::fs::canonicalize;

// Crates
use tauri_bundler::RpmSettings;

pub fn build_package() {
    let _ = env_logger::try_init();

    // Any level of GZIP compression will make the binary building fail
    let rpm_settings: RpmSettings = RpmSettings {
        compression: Some(tauri_utils::config::RpmCompression::None),
        ..Default::default()
    };

    // This simultaneously serves as input directory (where the binary is)
    // and output (where the bundle will be)
    let out_dir = canonicalize("../target/debug").unwrap();

    // Building settings
    let settings_builder = tauri_bundler::SettingsBuilder::new()
        .log_level(log::Level::Error)
        .package_settings(tauri_bundler::PackageSettings {
            product_name: "nomos-cli".to_string(),
            version: "1.0.0".to_string(),
            description: "CLI for Nomos".to_string(),
            homepage: None,
            authors: None,
            default_run: None,
        })
        .project_out_directory(out_dir)
        .bundle_settings(tauri_bundler::BundleSettings {
            identifier: Some("com.nomos.nomos-cli".to_string()),
            publisher: None,
            homepage: None,
            icon: Some(vec![
                "icons/icon.ico".to_string(),
                "icons/512x512.png".to_string(),
            ]),
            resources: None,
            resources_map: None,
            copyright: None,
            license: None,
            license_file: None,
            category: None,
            file_associations: None,
            short_description: None,
            long_description: None,
            bin: None,
            external_bin: None,
            deep_link_protocols: None,
            deb: Default::default(),
            appimage: Default::default(),
            rpm: rpm_settings,
            dmg: Default::default(),
            macos: Default::default(),
            updater: None,
            windows: Default::default(),
        })
        .binaries(vec![tauri_bundler::BundleBinary::new(
            "nomos-cli".to_string(),
            true,
        )]);

    let settings = settings_builder
        .build()
        .expect("Error while building settings");

    let arch = settings
        .target()
        .split("-")
        .next()
        .expect("Could not determine target architecture.");
    println!("Building for '{}'", arch);

    // Bypass an issue in the current linuxdeploy's version
    set_var("NO_STRIP", "true");

    // Tell `appimagetool` what arch we're building for, without it the tool errors out
    // This could be due to us making an ad-hoc use of `tauri-bundler` here,
    // perhaps we are bypassing some `tauri-bundler` piece of code or config that handles that,
    // but if that's the actual reason I couldn't find where that would be
    // Regardless, this works.
    set_var("ARCH", arch);

    if let Err(error) = tauri_bundler::bundle_project(&settings) {
        eprintln!("Error while bundling project: {:?}", error);
    }
}

fn main() {
    let _ = env_logger::try_init();
    let do_build_package = std::env::var("BUILD_PACKAGE")
        .ok()
        .and_then(|x| x.parse::<bool>().ok());
    if let Some(true) = do_build_package {
        log::info!("Building package.");
        build_package();
        log::info!("Package built successfully.");
    } else {
        log::info!("Skipping package building.");
    }
}
