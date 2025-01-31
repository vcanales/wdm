use clap::{Parser, Subcommand};
use reqwest::header::HeaderValue;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use serde_yaml;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zip::ZipArchive;

/// A CLI tool to manage WordPress dependencies.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize wdm in the current directory
    Init,
    /// Add a dependency to wdm.yml and install it
    Add {
        name: String,
        #[arg(short, long)]
        version: String,
        #[arg(short, long)]
        repo: String,
        #[arg(short = 'e', long)]
        token_env: Option<String>,
    },
    /// Remove a dependency from wdm.yml
    Remove { name: String },
    /// Install all dependencies from wdm.yml
    Install,
}

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    config: ConfigData,
    dependencies: Vec<Dependency>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ConfigData {
    wordpress_path: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Dependency {
    name: String,
    version: String,
    repo: String,
    token_env: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Lockfile {
    dependencies: Vec<LockedDependency>,
}

#[derive(Serialize, Deserialize, Debug)]
struct LockedDependency {
    name: String,
    version: String,
    repo: String,
    hash: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check if Git is installed
    if let Err(e) = check_git_installed() {
        println!("{}", e);
        return Ok(());
    }

    let cli = Cli::parse();

    match &cli.command {
        Commands::Init => {
            if Path::new("wdm.yml").exists() {
                println!("wdm.yml already exists");
            } else {
                let config = Config {
                    config: ConfigData {
                        wordpress_path: Some(".".to_string()), // Set default to "."
                    },
                    dependencies: Vec::new(),
                };
                fs::write("wdm.yml", serde_yaml::to_string(&config)?)?;
                println!("Initialized wdm.yml");
            }
            Ok(())
        }
        Commands::Add {
            name,
            version,
            repo,
            token_env,
        } => {
            let mut config: Config = if Path::new("wdm.yml").exists() {
                serde_yaml::from_str(&fs::read_to_string("wdm.yml")?)?
            } else {
                Config {
                    config: ConfigData {
                        wordpress_path: Some(".".to_string()),
                    },
                    dependencies: Vec::new(),
                }
            };

            // Normalize the name for consistent comparison
            let normalized_name = name.trim().to_lowercase();

            // Remove any existing dependency with the same normalized name to prevent duplicates
            let initial_len = config.dependencies.len();
            config
                .dependencies
                .retain(|d| d.name.trim().to_lowercase() != normalized_name);

            let mut dependency_existed = false;
            if config.dependencies.len() < initial_len {
                println!(
                    "Dependency '{}' already exists. Updating its information.",
                    name
                );
                dependency_existed = true;
            }

            // Add the new or updated dependency
            config.dependencies.push(Dependency {
                name: name.trim().to_string(),
                version: version.trim().to_string(),
                repo: repo.trim().to_string(),
                token_env: token_env.clone(),
                source: None,
            });

            if dependency_existed {
                println!("Updated {} in wdm.yml", name);
            } else {
                println!("Added {} to wdm.yml", name);
            }

            fs::write("wdm.yml", serde_yaml::to_string(&config)?)?;

            // Proceed to install the newly added dependency
            install_dependency(&config.dependencies.last().unwrap())?;

            Ok(())
        }
        Commands::Remove { name } => {
            if !Path::new("wdm.yml").exists() {
                println!("wdm.yml does not exist. Run 'wdm init' first.");
                return Ok(());
            }

            let mut config: Config = serde_yaml::from_str(&fs::read_to_string("wdm.yml")?)?;
            let initial_len = config.dependencies.len();
            config.dependencies.retain(|d| d.name != *name);

            if config.dependencies.len() < initial_len {
                fs::write("wdm.yml", serde_yaml::to_string(&config)?)?;
                println!("Removed {} from wdm.yml", name);
            } else {
                println!("Dependency '{}' not found in wdm.yml", name);
            }

            // Optionally, add code to remove the plugin from the wordpress_path
            Ok(())
        }
        Commands::Install => {
            if !Path::new("wdm.yml").exists() {
                println!("wdm.yml does not exist. Run 'wdm init' first.");
                return Ok(());
            }

            let config: Config = serde_yaml::from_str(&fs::read_to_string("wdm.yml")?)?;
            let mut lockfile = if Path::new("wdm.lock").exists() {
                serde_yaml::from_str(&fs::read_to_string("wdm.lock")?)?
            } else {
                Lockfile {
                    dependencies: Vec::new(),
                }
            };

            // Determine the root directory (where wdm.yml is located)
            let root_dir = resolve_root_dir()?;

            // Ensure that lockfile is at root_dir
            let lockfile_path = root_dir.join("wdm.lock");

            // Ensure that .wdm-cache directory is at root_dir
            let cache_dir = root_dir.join(".wdm-cache");
            if !cache_dir.exists() {
                fs::create_dir_all(&cache_dir)?;
            }

            // Set wordpress_path to default to '.' if not specified
            let wordpress_path = if let Some(path) = &config.config.wordpress_path {
                Path::new(&path).to_path_buf()
            } else {
                Path::new(".").to_path_buf()
            };

            for dep in &config.dependencies {
                println!("Installing {}...", dep.name);

                let token = if let Some(token_env) = &dep.token_env {
                    env::var(token_env).ok()
                } else {
                    None
                };

                let version =
                    match resolve_github_version(&dep.repo, &dep.version, token.as_deref()) {
                        Ok(ver) => ver,
                        Err(e) => {
                            println!("Error resolving version for {}: {}", dep.name, e);
                            continue;
                        }
                    };

                let response = match download_with_http(&dep.repo, &version, token.as_deref()) {
                    Ok(data) => data,
                    Err(e) => {
                        println!("Error downloading {}: {}", dep.name, e);
                        continue;
                    }
                };

                // Define the installation directory inside wp-content/plugins with the plugin's name
                let plugin_install_dir = wordpress_path.join("wp-content/plugins").join(&dep.name);

                // Check if the plugin is already installed by verifying the existence of the directory
                if plugin_install_dir.exists() {
                    println!(
                        "{} is already installed in {:?}",
                        dep.name, plugin_install_dir
                    );
                    continue;
                }

                // Save the zip to .wdm-cache
                let cache_plugin_dir = cache_dir.join(format!("{}.zip", dep.name));
                fs::write(&cache_plugin_dir, &response)?;
                println!("Saved {} to cache at {:?}", dep.name, cache_plugin_dir);

                // Extract the zip file into the plugin_install_dir
                let mut zip = match ZipArchive::new(std::io::Cursor::new(&response)) {
                    Ok(zip) => zip,
                    Err(e) => {
                        println!("Error reading zip for {}: {}", dep.name, e);
                        continue;
                    }
                };

                for i in 0..zip.len() {
                    let mut file = match zip.by_index(i) {
                        Ok(f) => f,
                        Err(e) => {
                            println!("Error accessing file {} in zip: {}", i, e);
                            continue;
                        }
                    };
                    let outpath = match file.enclosed_name().and_then(|name| {
                        // Construct the prefix based on repo and version without 'v'
                        let repo_name = dep.repo.split('/').last().unwrap();
                        let version_no_v = version.trim_start_matches('v');
                        let prefix = format!("{}-{}", repo_name, version_no_v);
                        name.strip_prefix(prefix.as_str()).ok()
                    }) {
                        Some(path) => plugin_install_dir.join(path),
                        None => {
                            println!("Invalid file path in zip for {}", dep.name);
                            continue;
                        }
                    };

                    if file.name().ends_with('/') {
                        if let Err(e) = fs::create_dir_all(&outpath) {
                            println!("Error creating directory {:?}: {}", outpath, e);
                            continue;
                        }
                    } else {
                        if let Some(p) = outpath.parent() {
                            if let Err(e) = fs::create_dir_all(&p) {
                                println!("Error creating directory {:?}: {}", p, e);
                                continue;
                            }
                        }
                        let mut outfile = match fs::File::create(&outpath) {
                            Ok(f) => f,
                            Err(e) => {
                                println!("Error creating file {:?}: {}", outpath, e);
                                continue;
                            }
                        };
                        if let Err(e) = std::io::copy(&mut file, &mut outfile) {
                            println!("Error writing to file {:?}: {}", outpath, e);
                            continue;
                        }
                    }
                }

                // Update the lockfile
                lockfile.dependencies.retain(|d| d.name != dep.name);
                lockfile.dependencies.push(LockedDependency {
                    name: dep.name.clone(),
                    version: version.clone(),
                    repo: dep.repo.clone(),
                    hash: format!("{:x}", Sha256::digest(&response)),
                });

                println!("Installed {} {}", dep.name, version);
            }

            // Write the updated lockfile at root_dir
            fs::write(&lockfile_path, serde_yaml::to_string(&lockfile)?)?;
            println!("Updated lockfile at {:?}", lockfile_path);

            Ok(())
        }
    }
}

/// Installs a single dependency.
///
/// This function encapsulates the installation logic, making it reusable.
/// It takes a reference to a `Dependency` struct and performs the necessary steps
/// to download and install the plugin.
fn install_dependency(dep: &Dependency) -> Result<(), Box<dyn std::error::Error>> {
    if !Path::new("wdm.yml").exists() {
        println!("wdm.yml does not exist. Run 'wdm init' first.");
        return Ok(());
    }

    let config: Config = serde_yaml::from_str(&fs::read_to_string("wdm.yml")?)?;
    let lockfile = if Path::new("wdm.lock").exists() {
        serde_yaml::from_str(&fs::read_to_string("wdm.lock")?)?
    } else {
        Lockfile {
            dependencies: Vec::new(),
        }
    };

    // Determine the root directory (where wdm.yml is located)
    let root_dir = resolve_root_dir()?;

    // Ensure that .wdm-cache directory is at root_dir
    let cache_dir = root_dir.join(".wdm-cache");
    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir)?;
    }

    // Set wordpress_path to default to '.' if not specified
    let wordpress_path = if let Some(path) = &config.config.wordpress_path {
        Path::new(&path).to_path_buf()
    } else {
        Path::new(".").to_path_buf()
    };

    println!("Installing {}...", dep.name);

    let token = if let Some(token_env) = &dep.token_env {
        env::var(token_env).ok()
    } else {
        None
    };

    let version = match resolve_github_version(&dep.repo, &dep.version, token.as_deref()) {
        Ok(ver) => ver,
        Err(e) => {
            println!("Error resolving version for {}: {}", dep.name, e);
            return Ok(());
        }
    };

    let response = match download_with_http(&dep.repo, &version, token.as_deref()) {
        Ok(data) => data,
        Err(e) => {
            println!("Error downloading {}: {}", dep.name, e);
            return Ok(());
        }
    };

    // Define the installation directory inside wp-content/plugins with the plugin's name
    let plugin_install_dir = wordpress_path.join("wp-content/plugins").join(&dep.name);

    // Check if the plugin is already installed by verifying the existence of the directory
    if plugin_install_dir.exists() {
        println!(
            "{} is already installed in {:?}",
            dep.name, plugin_install_dir
        );
        return Ok(());
    }

    // Save the zip to .wdm-cache
    let cache_plugin_dir = cache_dir.join(format!("{}.zip", dep.name));
    fs::write(&cache_plugin_dir, &response)?;
    println!("Saved {} to cache at {:?}", dep.name, cache_plugin_dir);

    // Extract the zip file into the plugin_install_dir
    let mut zip = match ZipArchive::new(std::io::Cursor::new(&response)) {
        Ok(zip) => zip,
        Err(e) => {
            println!("Error reading zip for {}: {}", dep.name, e);
            return Ok(());
        }
    };

    for i in 0..zip.len() {
        let mut file = match zip.by_index(i) {
            Ok(f) => f,
            Err(e) => {
                println!("Error accessing file {} in zip: {}", i, e);
                continue;
            }
        };
        let outpath = match file.enclosed_name().and_then(|name| {
            // Construct the prefix based on repo and version without 'v'
            let repo_name = dep.repo.split('/').last().unwrap();
            let version_no_v = version.trim_start_matches('v');
            let prefix = format!("{}-{}", repo_name, version_no_v);
            name.strip_prefix(prefix.as_str()).ok()
        }) {
            Some(path) => plugin_install_dir.join(path),
            None => {
                println!("Invalid file path in zip for {}", dep.name);
                continue;
            }
        };

        if file.name().ends_with('/') {
            if let Err(e) = fs::create_dir_all(&outpath) {
                println!("Error creating directory {:?}: {}", outpath, e);
                continue;
            }
        } else {
            if let Some(p) = outpath.parent() {
                if let Err(e) = fs::create_dir_all(&p) {
                    println!("Error creating directory {:?}: {}", p, e);
                    continue;
                }
            }
            let mut outfile = match fs::File::create(&outpath) {
                Ok(f) => f,
                Err(e) => {
                    println!("Error creating file {:?}: {}", outpath, e);
                    continue;
                }
            };
            if let Err(e) = std::io::copy(&mut file, &mut outfile) {
                println!("Error writing to file {:?}: {}", outpath, e);
                continue;
            }
        }
    }

    // Update the lockfile
    let mut updated_lockfile = lockfile;
    updated_lockfile.dependencies.retain(|d| d.name != dep.name);
    updated_lockfile.dependencies.push(LockedDependency {
        name: dep.name.clone(),
        version: version.clone(),
        repo: dep.repo.clone(),
        hash: format!("{:x}", Sha256::digest(&response)),
    });

    // Write the updated lockfile at root_dir
    let lockfile_path = root_dir.join("wdm.lock");
    fs::write(&lockfile_path, serde_yaml::to_string(&updated_lockfile)?)?;
    println!("Installed {} {}", dep.name, version);
    println!("Updated lockfile at {:?}", lockfile_path);

    Ok(())
}

/// Resolves the root directory where wdm.yml is located.
fn resolve_root_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Determine the root directory (where wdm.yml is located)
    let root_dir = Path::new("wdm.yml")
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    // Ensure that .wdm-cache directory is at root_dir
    let cache_dir = root_dir.join(".wdm-cache");
    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir)?;
    }

    Ok(root_dir)
}

/// Resolves the appropriate Git tag based on the version requirement using Git CLI.
///
/// # Arguments
///
/// * `repo` - The repository in the format "owner/repo".
/// * `version_req` - The version requirement string (e.g., "^2.0.0").
/// * `token` - Optional authentication token for private repositories.
///
/// # Returns
///
/// * `Ok(String)` containing the resolved version tag.
/// * `Err(String)` with an error message.
fn resolve_github_version(
    repo: &str,
    version_req: &str,
    _token: Option<&str>, // Token is not used for public repos
) -> Result<String, String> {
    let repo_url = format!("https://github.com/{}.git", repo);

    // Execute 'git ls-remote --tags <repo_url>' and capture the output without displaying it
    let output = Command::new("git")
        .args(&["ls-remote", "--tags", &repo_url])
        .stdout(Stdio::piped()) // Capture stdout
        .stderr(Stdio::piped()) // Capture stderr
        .output()
        .map_err(|e| format!("Failed to execute git: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Git command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut versions = Vec::new();

    for line in stdout.lines() {
        // Each line is of the format: <hash>\trefs/tags/<tag>
        if let Some(pos) = line.find('\t') {
            let tag_ref = &line[pos + 1..];
            if let Some(tag) = tag_ref.strip_prefix("refs/tags/") {
                // Handle annotated tags by stripping the ^{}
                let tag = tag.trim_end_matches("^{}");
                if let Ok(ver) = Version::parse(tag.trim_start_matches('v')) {
                    versions.push(ver);
                }
            }
        }
    }

    if versions.is_empty() {
        return Err("No valid versions found in repository tags.".to_string());
    }

    // Sort versions in descending order
    versions.sort_by(|a, b| b.cmp(a));

    // Determine the desired version based on version_req
    if version_req == "latest" {
        return Ok(format!("v{}", versions[0]));
    } else if let Ok(specific_version) = Version::parse(version_req) {
        if versions.contains(&specific_version) {
            return Ok(format!("v{}", specific_version));
        } else {
            return Err(format!(
                "Version {} not found in repository tags",
                version_req
            ));
        }
    } else {
        let req = VersionReq::parse(version_req)
            .map_err(|e| format!("Invalid version requirement '{}': {}", version_req, e))?;
        for ver in &versions {
            if req.matches(ver) {
                return Ok(format!("v{}", ver));
            }
        }
        Err(format!(
            "No matching version found for requirement {}",
            version_req
        ))
    }
}

/// Downloads the specified tag of a repository using HTTP.
///
/// # Arguments
///
/// * `repo` - The repository in the format "owner/repo".
/// * `version` - The specific version tag to download.
/// * `token` - Optional authentication token for private repositories.
///
/// # Returns
///
/// * `Ok(Vec<u8>)` containing the ZIP archive data.
/// * `Err(String)` with an error message.
fn download_with_http(repo: &str, version: &str, token: Option<&str>) -> Result<Vec<u8>, String> {
    let download_url = format!(
        "https://github.com/{}/archive/refs/tags/{}.zip",
        repo, version
    );

    let client = reqwest::blocking::Client::new();
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::USER_AGENT,
        HeaderValue::from_static("wdm-cli"),
    );

    // If a token is provided, add it to the headers for private repositories
    if let Some(token) = token {
        let auth_value = format!("Bearer {}", token);
        headers.insert(
            reqwest::header::AUTHORIZATION,
            HeaderValue::from_str(&auth_value).map_err(|e| format!("Invalid token: {}", e))?,
        );
    }

    let response = client
        .get(&download_url)
        .headers(headers)
        .send()
        .map_err(|e| format!("Failed to send HTTP request: {}", e))?;

    if response.status().is_success() {
        Ok(response
            .bytes()
            .map_err(|e| format!("Failed to read response bytes: {}", e))?
            .to_vec())
    } else {
        match response.status().as_u16() {
            401 => Err("Unauthorized: Invalid or insufficient token permissions.".to_string()),
            403 => Err("Forbidden: Access denied. Check token permissions.".to_string()),
            404 => {
                Err("Not Found: The specified tag does not exist or access is denied.".to_string())
            }
            _ => Err(format!(
                "Failed to download from GitHub: HTTP {}",
                response.status()
            )),
        }
    }
}

/// Checks if Git is installed and accessible.
///
/// # Returns
///
/// * `Ok(())` if Git is installed.
/// * `Err(String)` with an error message if Git is not installed.
fn check_git_installed() -> Result<(), String> {
    let status = Command::new("git")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Failed to execute git: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err("Git is not installed or not accessible in PATH.".to_string())
    }
}
