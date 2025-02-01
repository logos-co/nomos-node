// STD
use std::env::set_var;
use std::fs::canonicalize;
// Crates
use clap::{arg, Parser};
use log::{error, info};
use tauri_bundler::RpmSettings;
use tauri_utils::platform::target_triple;
// Internal
use bundler::utils::{
    get_formatted_cargo_package_version, get_project_identifier,
    get_target_directory_for_current_profile, get_workspace_root,
};

const CRATE_NAME: &str = "nomos-node";
const CRATE_PATH_RELATIVE_TO_WORKSPACE_ROOT: &str = "nodes/nomos-node";

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

fn build_package(version: String) {
    let crate_path = get_workspace_root().join(CRATE_PATH_RELATIVE_TO_WORKSPACE_ROOT);
    info!("Bundling package '{}'", crate_path.display());
    let resources_path = crate_path.join("resources");

    // This simultaneously serves as input directory (where the binary is)
    // and output (where the bundle will be)
    let target_triple = target_triple().expect("Could not determine target triple");
    let project_target_directory =
        canonicalize(get_target_directory_for_current_profile(target_triple.as_str()).unwrap())
            .unwrap();
    info!(
        "Bundle output directory: '{}'",
        project_target_directory.display()
    );

    // Any level of GZIP compression will make the binary building fail
    // TODO: Re-enable RPM compression when the issue is fixed
    let rpm_settings: RpmSettings = RpmSettings {
        compression: Some(tauri_utils::config::RpmCompression::None),
        ..Default::default()
    };

    // Building settings
    let settings_builder = tauri_bundler::SettingsBuilder::new()
        .log_level(log::Level::Error)
        .package_settings(tauri_bundler::PackageSettings {
            product_name: String::from(CRATE_NAME),
            version,
            description: "Nomos Node".to_string(),
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

#[derive(Parser)]
struct BundleArguments {
    #[arg(
        short,
        long,
        value_name = "VERSION",
        help = "Expected Cargo package version. \
        If passed, this verifies the Cargo package version, panicking if it doesn't match."
    )]
    version: Option<String>,
}

/// If a version argument is provided, verify it matches the Cargo package version
/// This is passed by the CI/CD pipeline to ensure the version is consistent
fn parse_version(arguments: BundleArguments, cargo_package_version: String) -> String {
    if let Some(version) = arguments.version {
        // Check for version mismatch
        if version != cargo_package_version {
            // Maybe this should be a warning instead of a panic?
            panic!(
                "Error: Expected Cargo package version: '{}', but received argument: '{}'. \
            Please ensure the version matches the Cargo package version.",
                cargo_package_version, version
            );
        }

        version
    } else {
        cargo_package_version
    }
}

fn main() {
    let _ = env_logger::try_init();

    let cargo_package_version = get_formatted_cargo_package_version(CRATE_NAME);
    let bundle_arguments = BundleArguments::parse();
    let version = parse_version(bundle_arguments, cargo_package_version);

    build_package(version);
}
