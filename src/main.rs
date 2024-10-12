use clap::{Parser, Subcommand};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::exit;

#[derive(Parser)]
#[command(
    name = "wdm",
    version = "0.1.0",
    about = "WordPress Dependency Manager"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize wdm in the current directory
    Init,
    /// Add a plugin to the dependencies
    Add {
        plugin_name: String,
        #[arg(short, long, default_value = "latest")]
        version: String,
        #[arg(short, long, default_value = "github")]
        source: String,
        #[arg(short, long)]
        repo: Option<String>,
    },
    /// Install plugins from wdm.yml
    Install,
}

#[derive(Serialize, Deserialize, Clone)]
struct WdmFile {
    #[serde(default)]
    config: Config,
    dependencies: Vec<Dependency>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
struct Config {
    #[serde(default = "default_wordpress_path")]
    wordpress_path: String,
}

fn default_wordpress_path() -> String {
    ".".to_string()
}

#[derive(Serialize, Deserialize, Clone)]
struct Dependency {
    name: String,
    version: String,
    source: String,
    repo: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct LockEntry {
    name: String,
    version: String,
    checksum: String,
    source: String,
    repo: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct WdmLock {
    dependencies: Vec<LockEntry>,
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Init => init(),
        Commands::Add {
            plugin_name,
            version,
            source,
            repo,
        } => add(plugin_name, version, source, repo),
        Commands::Install => install(),
    }
}

fn init() {
    if Path::new("wdm.yml").exists() {
        eprintln!("wdm.yml already exists.");
        exit(1);
    }
    let wdm_file = WdmFile {
        config: Config::default(),
        dependencies: Vec::new(),
    };
    let yaml = serde_yaml::to_string(&wdm_file).unwrap();
    fs::write("wdm.yml", yaml).expect("Unable to write wdm.yml");
    println!("Initialized wdm.yml");
}

fn add(plugin_name: &str, version: &str, source: &str, repo: &Option<String>) {
    if !Path::new("wdm.yml").exists() {
        eprintln!("wdm.yml not found. Run 'wdm init' first.");
        exit(1);
    }
    let mut wdm_file: WdmFile =
        serde_yaml::from_str(&fs::read_to_string("wdm.yml").unwrap()).unwrap();

    if wdm_file
        .dependencies
        .iter()
        .any(|dep| dep.name == plugin_name)
    {
        eprintln!("Plugin {} is already in dependencies.", plugin_name);
        exit(1);
    }

    let dependency = Dependency {
        name: plugin_name.to_string(),
        version: version.to_string(),
        source: source.to_string(),
        repo: repo.clone(),
    };

    wdm_file.dependencies.push(dependency);

    let yaml = serde_yaml::to_string(&wdm_file).unwrap();
    fs::write("wdm.yml", yaml).expect("Unable to write wdm.yml");
    println!("Added {} to wdm.yml", plugin_name);
}

fn install() {
    if !Path::new("wdm.yml").exists() {
        eprintln!("wdm.yml not found. Run 'wdm init' first.");
        exit(1);
    }
    let wdm_file: WdmFile = serde_yaml::from_str(&fs::read_to_string("wdm.yml").unwrap()).unwrap();

    let wordpress_path = Path::new(&wdm_file.config.wordpress_path);

    let wp_plugins_dir = wordpress_path.join("wp-content/plugins");
    if !wp_plugins_dir.exists() {
        eprintln!(
            "WordPress plugins directory does not exist: {:?}",
            wp_plugins_dir
        );
        exit(1);
    }

    let mut lock_file = if Path::new("wdm.lock").exists() {
        serde_yaml::from_str(&fs::read_to_string("wdm.lock").unwrap()).unwrap()
    } else {
        WdmLock {
            dependencies: Vec::new(),
        }
    };

    if wdm_file.dependencies.is_empty() {
        println!("No dependencies to install.");
        exit(0);
    }

    for dep in wdm_file.dependencies {
        let plugin_name = dep.name.clone();
        let requested_version = dep.version.clone();
        let source = dep.source.clone();
        let repo = dep.repo.clone();

        println!("Installing {}...", plugin_name);

        let lock_entry = lock_file
            .dependencies
            .iter()
            .find(|e| e.name == plugin_name);

        let resolved_version;
        let checksum;

        if let Some(entry) = lock_entry {
            // Use locked version and checksum
            resolved_version = entry.version.clone();
            checksum = entry.checksum.clone();
            // Download and install the plugin using locked version
            if let Some(repo) = repo.clone() {
                match download_and_install_plugin(
                    &plugin_name,
                    &resolved_version,
                    &source,
                    &repo,
                    &wp_plugins_dir,
                ) {
                    Ok(calc_checksum) => {
                        if calc_checksum != checksum {
                            eprintln!(
                                "Checksum mismatch for {}. Expected {}, got {}.",
                                plugin_name, checksum, calc_checksum
                            );
                            exit(1);
                        }
                    }
                    Err(err) => {
                        eprintln!("Failed to install {}: {}", plugin_name, err);
                        exit(1);
                    }
                }
            } else {
                eprintln!("Repository not specified for plugin {}", plugin_name);
                exit(1);
            }
        } else {
            // Resolve version and calculate checksum
            if source == "github" {
                if let Some(repo) = repo.clone() {
                    match resolve_github_version(&repo, &requested_version) {
                        Ok(version) => {
                            resolved_version = version;
                        }
                        Err(err) => {
                            eprintln!("Failed to resolve version for {}: {}", plugin_name, err);
                            exit(1);
                        }
                    }
                } else {
                    eprintln!("Repository not specified for plugin {}", plugin_name);
                    exit(1);
                }
            } else {
                eprintln!("Unsupported source {}", source);
                exit(1);
            }

            // Download and calculate checksum
            if let Some(repo) = repo.clone() {
                match download_and_install_plugin(
                    &plugin_name,
                    &resolved_version,
                    &source,
                    &repo,
                    &wp_plugins_dir,
                ) {
                    Ok(calc_checksum) => {
                        checksum = calc_checksum;
                    }
                    Err(err) => {
                        eprintln!("Failed to install {}: {}", plugin_name, err);
                        exit(1);
                    }
                }
            } else {
                eprintln!("Repository not specified for plugin {}", plugin_name);
                exit(1);
            }

            // Add to lock file
            lock_file.dependencies.push(LockEntry {
                name: plugin_name.clone(),
                version: resolved_version.clone(),
                checksum: checksum.clone(),
                source: source.clone(),
                repo: repo.clone(),
            });
        }

        println!("Installed {} {}", plugin_name, resolved_version);
    }

    // Write lock file
    let yaml = serde_yaml::to_string(&lock_file).unwrap();
    fs::write("wdm.lock", yaml).expect("Unable to write wdm.lock");
}

fn resolve_github_version(repo: &str, version_req: &str) -> Result<String, String> {
    let url = format!("https://api.github.com/repos/{}/tags", repo);
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "wdm-cli")
        .send()
        .map_err(|e| e.to_string())?;

    if response.status().is_success() {
        let tags: Vec<Value> = response.json().map_err(|e| e.to_string())?;
        let mut versions = Vec::new();
        for tag in tags {
            if let Some(name) = tag["name"].as_str() {
                if let Some(v) = name.strip_prefix('v') {
                    if let Ok(ver) = Version::parse(v) {
                        versions.push(ver);
                    }
                }
            }
        }

        let req = VersionReq::parse(version_req).map_err(|e| e.to_string())?;
        versions.sort();
        versions.reverse();

        for ver in versions {
            if req.matches(&ver) {
                return Ok(ver.to_string());
            }
        }

        Err(format!(
            "No matching version found for requirement {}",
            version_req
        ))
    } else {
        Err(format!(
            "Failed to fetch tags from GitHub: HTTP {}",
            response.status()
        ))
    }
}

fn download_and_install_plugin(
    plugin_name: &str,
    version: &str,
    source: &str,
    repo: &str,
    wp_plugins_dir: &Path,
) -> Result<String, String> {
    // Construct the download URL
    let download_url = match source {
        "github" => format!(
            "https://github.com/{}/archive/refs/tags/v{}.zip",
            repo, version
        ),
        _ => return Err(format!("Unsupported source {}", source)),
    };

    // Download the plugin ZIP
    let plugin_data = reqwest::blocking::get(&download_url)
        .map_err(|e| e.to_string())?
        .bytes()
        .map_err(|e| e.to_string())?;

    // Calculate checksum
    let mut hasher = Sha256::new();
    hasher.update(&plugin_data);
    let checksum = format!("{:x}", hasher.finalize());

    // Extract ZIP to plugins directory
    let reader = std::io::Cursor::new(plugin_data);
    let mut zip = zip::ZipArchive::new(reader).map_err(|e| e.to_string())?;

    // Define the plugin directory path
    let plugin_dir = wp_plugins_dir.join(plugin_name);

    // Remove existing plugin directory if it exists
    if plugin_dir.exists() {
        fs::remove_dir_all(&plugin_dir).map_err(|e| e.to_string())?;
    }

    // Ensure the plugin directory exists
    fs::create_dir_all(&plugin_dir).map_err(|e| e.to_string())?;

    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| e.to_string())?;
        let extracted_path = file.mangled_name();

        // Remove the root directory from the extracted path
        let relative_path = extracted_path
            .components()
            .skip(1) // Skip the first component (root directory in the ZIP)
            .collect::<PathBuf>();

        // If skipping the first component results in an empty path, continue
        if relative_path.as_os_str().is_empty() {
            continue;
        }

        let outpath = plugin_dir.join(&relative_path);

        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = outpath.parent() {
                if !parent.exists() {
                    fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
                }
            }
            let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;

            // Set permissions (optional)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = file.unix_mode() {
                    fs::set_permissions(&outpath, fs::Permissions::from_mode(mode))
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }

    Ok(checksum)
}
