use clap::{Parser, Subcommand};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, USER_AGENT};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use serde_yaml;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::Path;

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
                        wordpress_path: None,
                    },
                    dependencies: Vec::new(),
                };
                fs::write("wdm.yml", serde_yaml::to_string(&config)?)?;
                println!("Initialized wdm.yml");
            }
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
                        wordpress_path: None,
                    },
                    dependencies: Vec::new(),
                }
            };

            config.dependencies.push(Dependency {
                name: name.clone(),
                version: version.clone(),
                repo: repo.clone(),
                token_env: token_env.clone(),
            });

            fs::write("wdm.yml", serde_yaml::to_string(&config)?)?;
            println!("Added {} to wdm.yml", name);
        }
        Commands::Remove { name } => {
            if !Path::new("wdm.yml").exists() {
                println!("wdm.yml does not exist. Run 'wdm init' first.");
                return Ok(());
            }

            let mut config: Config = serde_yaml::from_str(&fs::read_to_string("wdm.yml")?)?;
            config.dependencies.retain(|d| d.name != *name);

            fs::write("wdm.yml", serde_yaml::to_string(&config)?)?;
            println!("Removed {} from wdm.yml", name);

            // Optionally, you can add code to remove the plugin from the wordpress_path
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

            let wordpress_path = if let Some(path) = &config.config.wordpress_path {
                Path::new(&path).to_path_buf()
            } else {
                println!("wordpress_path not set in wdm.yml");
                return Ok(());
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

                let version = resolve_github_version(
                    &client,
                    &github_api_base_url,
                    &dep.repo,
                    &dep.version,
                    token.as_deref(),
                )?;

                let download_url = format!(
                    "https://github.com/{}/archive/refs/tags/v{}.zip",
                    dep.repo, version
                );

                let response = download_with_auth(&client, &download_url, token.as_deref())?;

                let hash = format!("{:x}", Sha256::digest(&response));

                // Check if the dependency is already in the lockfile with the same hash
                let mut needs_install = true;
                if let Some(_locked_dep) = lockfile
                    .dependencies
                    .iter()
                    .find(|d| d.name == dep.name && d.hash == hash)
                {
                    println!("{} is already up to date", dep.name);
                    needs_install = false;
                }

                if needs_install {
                    // Extract the zip file to the wordpress_path
                    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(response))?;
                    let plugin_path = wordpress_path.join("wp-content/plugins");

                    for i in 0..zip.len() {
                        let mut file = zip.by_index(i)?;
                        let outpath = plugin_path.join(file.mangled_name().strip_prefix(
                            format!("{}-{}", dep.repo.split('/').last().unwrap(), version),
                        )?);

                        if file.name().ends_with('/') {
                            fs::create_dir_all(&outpath)?;
                        } else {
                            if let Some(p) = outpath.parent() {
                                if !p.exists() {
                                    fs::create_dir_all(&p)?;
                                }
                            }
                            let mut outfile = fs::File::create(&outpath)?;
                            std::io::copy(&mut file, &mut outfile)?;
                        }
                    }

                    // Update the lockfile
                    lockfile.dependencies.retain(|d| d.name != dep.name);
                    lockfile.dependencies.push(LockedDependency {
                        name: dep.name.clone(),
                        version: version.clone(),
                        repo: dep.repo.clone(),
                        hash,
                    });

                    println!("Installed {} {}", dep.name, version);
                }
            }

            // Write the updated lockfile
            fs::write("wdm.lock", serde_yaml::to_string(&lockfile)?)?;
        }
    }

    Ok(())
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
