use std::env;
// STD
use log::{error, info};
use std::env::set_var;
use std::fs::canonicalize;
// Crates
use tauri_bundler::RpmSettings;
use tauri_utils::platform::target_triple;
// Internal
use bundler::utils::{
    get_formatted_cargo_package_version, get_project_identifier,
    get_target_directory_for_current_profile, get_workspace_root,
};

const CRATE_NAME: &str = "nomos-cli";
const RELATIVE_TO_WORKSPACE_PATH: &str = "nomos-cli";

fn prepare_environment(architecture: &str) {
    // Bypass an issue in the current linuxdeploy's version
    set_var("NO_STRIP", "true");

    // Tell `appimagetool` what arch we're building for, without it the tool errors out
    // This could be due to us making an ad-hoc use of `tauri-bundler` here,
    // perhaps we are bypassing some `tauri-bundler` piece of code or config that handles that,
    // but if that's the actual reason I couldn't find where that would be
    // Regardless, this works.
    set_var("ARCH", architecture);
}

fn build_package(version: &str) {
    let crate_path = get_workspace_root().join(RELATIVE_TO_WORKSPACE_PATH);
    info!("Bundling package '{}'", crate_path.display());
    let resources_path = crate_path.join("resources");

    // This simultaneously serves as input directory (where the binary is)
    // and output (where the bundle will be)
    let target_triple = target_triple().expect("Could not determine target triple");
    let project_target_directory = canonicalize(get_target_directory_for_current_profile(
        target_triple.as_str(),
    ))
    .unwrap();
    info!(
        "Bundle output directory: '{}'",
        project_target_directory.display()
    );

    // Any level of GZIP compression will make the binary building fail
    let rpm_settings: RpmSettings = RpmSettings {
        compression: Some(tauri_utils::config::RpmCompression::None),
        ..Default::default()
    };

    // Building settings
    let settings_builder = tauri_bundler::SettingsBuilder::new()
        .log_level(log::Level::Error)
        .package_settings(tauri_bundler::PackageSettings {
            product_name: String::from(CRATE_NAME),
            version: version.to_string(),
            description: "CLI for Nomos".to_string(),
            homepage: None,
            authors: None,
            default_run: None,
        })
        .project_out_directory(&project_target_directory)
        .bundle_settings(tauri_bundler::BundleSettings {
            identifier: Some(get_project_identifier(CRATE_NAME)),
            publisher: None,
            homepage: None,
            icon: Some(vec![
                resources_path
                    .join("icons/icon.ico")
                    .to_string_lossy()
                    .to_string(),
                resources_path
                    .join("icons/512x512.png")
                    .to_string_lossy()
                    .to_string(),
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
            String::from(CRATE_NAME),
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
    info!("Bundling for '{}'", arch);

    prepare_environment(arch);

    if let Err(error) = tauri_bundler::bundle_project(&settings) {
        error!("Error while bundling project: {:?}", error);
    } else {
        info!("Package bundled successfully");
    }
}

fn main() {
    let _ = env_logger::try_init();
    let cargo_package_version = get_formatted_cargo_package_version(CRATE_NAME);

    // Parse arguments
    let args: Vec<String> = env::args().collect();

    // Expecting at least one argument (the version)
    // This is passed by the CI/CD pipeline
    let version = args.get(1).expect(
        "Error: A version argument is required in the format 'vX.Y.Z'. \
        Example usage: `cargo run v1.2.3`",
    );

    // Check for version mismatch
    if version != cargo_package_version.as_str() {
        panic!(
            "Error: Expected Cargo package version: '{}', but received argument: '{}'. \
            Please ensure the version matches the Cargo package version.",
            cargo_package_version, version
        );
    }

    build_package(version);
}
