use clap::{Parser, Subcommand};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, USER_AGENT};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use serde_yaml;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

const DEFAULT_GITHUB_API_BASE_URL: &str = "https://api.github.com";

#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize wdm in the current directory
    Init,
    /// Add a dependency to wdm.yml
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
    /// Install dependencies
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

            // Optionally, you can add code to remove the plugin from the wordpress_path
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

            // Ensure that .lock file is at root_dir
            let lockfile_path = root_dir.join("wdm.lock");

            // Ensure that . directory is at root_dir
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

            let client = Client::new();
            let github_api_base_url = env::var("GITHUB_API_BASE_URL")
                .unwrap_or_else(|_| DEFAULT_GITHUB_API_BASE_URL.to_string());

            for dep in &config.dependencies {
                println!("Installing {}...", dep.name);

                let token = if let Some(token_env) = &dep.token_env {
                    env::var(token_env).ok()
                } else {
                    None
                };

                let version = match resolve_github_version(
                    &client,
                    &github_api_base_url,
                    &dep.repo,
                    &dep.version,
                    token.as_deref(),
                ) {
                    Ok(ver) => ver,
                    Err(e) => {
                        println!("Error resolving version for {}: {}", dep.name, e);
                        continue;
                    }
                };

                let download_url = format!(
                    "https://github.com/{}/archive/refs/tags/v{}.zip",
                    dep.repo, version
                );

                let response = match download_with_auth(&client, &download_url, token.as_deref()) {
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

                // Save the zip to wdm-cache
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
                        // Construct the prefix based on repo and version
                        let repo_name = dep.repo.split('/').last().unwrap();
                        let prefix = format!("{}-{}", repo_name, version);
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

fn resolve_root_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Determine the root directory (where wdm.yml is located)
    let root_dir = Path::new("wdm.yml")
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    // Ensure that wdm-cache directory is at root_dir
    let cache_dir = root_dir.join(".wdm-cache");
    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir)?;
    }

    Ok(root_dir)
}

fn resolve_github_version(
    client: &Client,
    base_url: &str,
    repo: &str,
    version_req: &str,
    token: Option<&str>,
) -> Result<String, String> {
    let url = format!("{}/repos/{}/tags", base_url, repo);

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static("wdm-cli"));
    if let Some(token) = token {
        let auth_value = format!("token {}", token);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_value).map_err(|e| e.to_string())?,
        );
    }

    let response = client
        .get(&url)
        .headers(headers)
        .send()
        .map_err(|e| e.to_string())?;

    if response.status().is_success() {
        let tags: Vec<serde_json::Value> = response.json().map_err(|e| e.to_string())?;
        let mut versions = Vec::new();
        for tag in tags {
            if let Some(name) = tag["name"].as_str() {
                let version_str = name.trim_start_matches(|c: char| !c.is_ascii_digit());
                if let Ok(ver) = Version::parse(version_str) {
                    versions.push(ver);
                }
            }
        }

        if versions.is_empty() {
            return Err("No valid versions found in tags.".to_string());
        }

        versions.sort_by(|a, b| b.cmp(a)); // Sort descending

        if version_req == "latest" {
            return Ok(versions[0].to_string());
        } else if let Ok(specific_version) = Version::parse(version_req) {
            if versions.contains(&specific_version) {
                return Ok(specific_version.to_string());
            } else {
                return Err(format!(
                    "Version {} not found in repository tags",
                    version_req
                ));
            }
        } else {
            let req = VersionReq::parse(version_req).map_err(|e| e.to_string())?;
            for ver in &versions {
                if req.matches(ver) {
                    return Ok(ver.to_string());
                }
            }
            Err(format!(
                "No matching version found for requirement {}",
                version_req
            ))
        }
    } else {
        Err(format!(
            "Failed to fetch tags from GitHub: HTTP {}",
            response.status()
        ))
    }
}

fn download_with_auth(client: &Client, url: &str, token: Option<&str>) -> Result<Vec<u8>, String> {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static("wdm-cli"));
    if let Some(token) = token {
        let auth_value = format!("token {}", token);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_value).map_err(|e| e.to_string())?,
        );
    }

    let response = client
        .get(url)
        .headers(headers)
        .send()
        .map_err(|e| e.to_string())?;

    if response.status().is_success() {
        Ok(response.bytes().map_err(|e| e.to_string())?.to_vec())
    } else {
        Err(format!(
            "Failed to download from GitHub: HTTP {}",
            response.status()
        ))
    }
}
