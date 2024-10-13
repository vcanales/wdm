# wdm-cli

**DISCLAIMER: This project is currently in progress and under active development. Features, documentation, and functionality may change or be incomplete and/or broken.**

**wdm-cli** is a command-line tool for managing WordPress plugin dependencies. It provides a decentralized alternative that empowers authors with control over where they store their plugins and gives users more granular control over their dependencies. With **wdm-cli**, you can specify exact versions, repositories (including private ones), and manage your WordPress projects' dependencies with greater flexibility.

## Table of Contents

- [Features](#features)
- [Installation](#installation)
- [Getting Started](#getting-started)
  - [Initialize wdm in Your Project](#initialize-wdm-in-your-project)
  - [Setting the WordPress Path](#setting-the-wordpress-path)
- [Usage](#usage)
  - [Adding Dependencies](#adding-dependencies)
  - [Installing Dependencies](#installing-dependencies)
  - [Using Private Repositories](#using-private-repositories)
  - [Updating Dependencies](#updating-dependencies)
  - [Removing Dependencies](#removing-dependencies)
- [Configuration](#configuration)
- [Examples](#examples)
- [Contributing](#contributing)
- [License](#license)

---

## Features

- **Decentralized Dependency Management**: Authors can store plugins in their own repositories, including private ones, giving them full control.
- **Granular Control**: Users can specify exact versions and repositories, allowing for precise dependency management.
- **Private Repository Support**: Access private GitHub repositories using tokens defined as environment variables.
- **Multiple Token Support**: Manage multiple private dependencies that require different tokens.
- **Lockfile Support**: Keeps track of exact versions installed to ensure consistent environments.
- **Easy Installation**: Install all dependencies with a single command.
- **Uninstallation**: Remove dependencies cleanly from your project.

## Installation

You can install **wdm-cli** using Cargo, the Rust package manager:

```bash
cargo install wdm-cli
```

Alternatively, you can clone the repository and build it manually:

```bash
git clone https://github.com/vcanales/wdm-cli.git
cd wdm-cli
cargo build --release
```

This will create an executable in `target/release/wdm`, which you can move to a directory in your PATH.

## Getting Started

### Initialize wdm in Your Project

Navigate to your WordPress project directory and initialize **wdm**:

```bash
wdm init
```

This command creates a `wdm.yml` file in your current directory, which will hold your dependencies and configuration.

### Setting the WordPress Path

By default, **wdm** expects your WordPress installation to be in the current directory. If your WordPress installation is located elsewhere, you can set the `wordpress_path` in the `wdm.yml` file:

```yaml
config:
  wordpress_path: "/path/to/your/wordpress"
dependencies: []
```

## Usage

### Adding Dependencies

To add a plugin to your project, use the `add` command:

```bash
wdm add <dependency-name> --version <version> --repo <repository> [--token-env <token-env-variable>]
```

- `<dependency-name>`: The name you want to give to the dependency.
- `--version`: The version of the dependency. You can specify an exact version (e.g., `1.8.0`), `latest`, or a version requirement like `^1.0`.
- `--repo`: The repository where the dependency is stored in the format `owner/repo`.
- `--token-env` *(optional)*: The name of the environment variable that contains the GitHub token for accessing private repositories.

**Examples:**

1. **Adding a Public Dependency:**

    ```bash
    wdm add create-block-theme --version latest --repo WordPress/create-block-theme
    ```

    This command adds the `create-block-theme` plugin from the `WordPress/create-block-theme` repository at the latest version.

2. **Adding a Private Dependency:**

    ```bash
    wdm add private-plugin --version latest --repo yourusername/private-plugin --token-env WDM_TOKEN_PRIVATE_PLUGIN
    ```

    This command adds the `private-plugin` from your private repository, using the token stored in the `WDM_TOKEN_PRIVATE_PLUGIN` environment variable.

### Installing Dependencies

To install all dependencies listed in your `wdm.yml`, run:

```bash
wdm install
```

This command resolves the versions, downloads the dependencies, and installs them into your WordPress installation.

### Using Private Repositories

**wdm-cli** supports installing dependencies from private GitHub repositories. To access private repositories, you need to provide a GitHub Personal Access Token (PAT). Tokens should be defined as environment variables. 

If you have multiple private dependencies that require different tokens, you can specify different environment variables for each dependency.

#### Setting Up Tokens

1. **Create a GitHub Personal Access Token**

   - Log in to your GitHub account.
   - Navigate to **Settings** > **Developer settings** > **Personal access tokens**.
   - Click **Generate new token**.
   - Select the scopes you need (usually `repo` for private repositories).
   - Generate the token and copy it.

2. **Define Environment Variables**

   - For each private dependency, define an environment variable with the token.
   - Use a naming convention that associates the token with the dependency.

   **Example:**

   ```bash
   export WDM_TOKEN_CUSTOM_PLUGIN="your-token-for-custom-plugin"
   export WDM_TOKEN_ANOTHER_PLUGIN="your-token-for-another-plugin"
   ```

#### Adding Private Dependencies

When adding a private dependency, specify the environment variable that contains the token using the `--token-env` option.

```bash
wdm add <dependency-name> --version <version> --repo <repository> --token-env <token-env-variable>
```

- `--token-env`: The name of the environment variable that contains the token for this dependency.

**Example:**

```bash
wdm add private-plugin --version latest --repo yourusername/private-plugin --token-env WDM_TOKEN_CUSTOM_PLUGIN
```

#### Installing Private Dependencies

When you run `wdm install`, **wdm-cli** will use the specified environment variables to access the private repositories.

**Important:**

- Ensure that the environment variables are set in your shell or CI environment before running `wdm install`.
- Do not commit your tokens to version control. Use environment variables to keep your tokens secure.

### Updating Dependencies

If you want to update a dependency to a newer version, you can change the version in `wdm.yml` and run `wdm install` again.

**Example:**

1. Edit `wdm.yml`:

   ```yaml
   dependencies:
     - name: private-plugin
       version: "1.0.0"
       repo: yourusername/private-plugin
       token_env: WDM_TOKEN_CUSTOM_PLUGIN
   ```

2. Change the version to `"1.1.0"` or `"latest"`:

   ```yaml
   dependencies:
     - name: private-plugin
       version: "latest"
       repo: yourusername/private-plugin
       token_env: WDM_TOKEN_CUSTOM_PLUGIN
   ```

3. Run the install command:

   ```bash
   wdm install
   ```

### Removing Dependencies

To remove a dependency from your project, use the `remove` command:

```bash
wdm remove <dependency-name>
```

**Example:**

```bash
wdm remove private-plugin
```

This command removes `private-plugin` from your `wdm.yml` and uninstalls it from your WordPress installation.

## Configuration

Below is a table detailing all the supported fields in the `wdm.yml` configuration file for **wdm-cli**, including their default values.

| **Field**                  | **Type** | **Description**                                                                                              | **Required** | **Default Value**                  |
|----------------------------|----------|--------------------------------------------------------------------------------------------------------------|--------------|------------------------------------|
| `config`                   | Object   | Contains configuration settings for **wdm-cli**.                                                           | Yes          | N/A                                |
| `config.wordpress_path`    | String   | Specifies the file system path to your WordPress installation. Defaults to the current directory if not set. | Yes          | Current working directory (`.`)    |
| `dependencies`             | Array    | Lists all the dependencies (plugins/themes) managed by **wdm-cli**.                                        | Yes          | Empty array `[]`                   |
| `dependencies[].name`      | String   | The unique name you assign to the dependency.                                                                | Yes          | N/A                                |
| `dependencies[].version`   | String   | The version of the dependency. Can be an exact version (e.g., `1.8.0`), `latest`, or a version requirement like `^1.0`. | Yes          | N/A                                |
| `dependencies[].repo`      | String   | The GitHub repository of the dependency in the format `owner/repo`.                                         | Yes          | N/A                                |
| `dependencies[].token_env` | String   | *(Optional)* The name of the environment variable that contains the GitHub token for accessing private repositories. | No           | N/A                                |

### Detailed Descriptions

#### 1. `config` Object

- **`wordpress_path`**
  - **Type:** String
  - **Description:** Defines the absolute or relative path to your WordPress installation directory. If not specified, **wdm-cli** assumes the current working directory is the WordPress path.
  - **Required:** Yes
  - **Default Value:** Current working directory (`.`)

  **Example:**
  ```yaml
  config:
    wordpress_path: "/var/www/html/wordpress"
  ```

#### 2. `dependencies` Array

Each item in the `dependencies` array represents a plugin that you want to manage with **wdm-cli**.

- **`name`**
  - **Type:** String
  - **Description:** A unique identifier for the dependency within your project. This name is used to reference the dependency in **wdm-cli** commands.
  - **Required:** Yes
  - **Default Value:** N/A

  **Example:**
  ```yaml
  dependencies:
    - name: custom-plugin
  ```

- **`version`**
  - **Type:** String
  - **Description:** Specifies the version of the dependency to install. It can be:
    - An exact version number (e.g., `1.8.0`)
    - `latest` to fetch the most recent version
    - A semantic version requirement (e.g., `^1.0`)
  - **Required:** Yes
  - **Default Value:** N/A

  **Example:**
  ```yaml
    - version: "^1.8.0"
  ```

- **`repo`**
  - **Type:** String
  - **Description:** The GitHub repository where the dependency is hosted, formatted as `owner/repo`.
  - **Required:** Yes
  - **Default Value:** N/A

  **Example:**
  ```yaml
    - repo: yourusername/custom-plugin
  ```

- **`token_env`**
  - **Type:** String
  - **Description:** *(Optional)* The name of the environment variable that holds the GitHub Personal Access Token (PAT) required to access private repositories.
  - **Required:** No
  - **Default Value:** N/A

  **Example:**
  ```yaml
    - token_env: WDM_TOKEN_CUSTOM_PLUGIN
  ```

---

## Examples

### Adding and Installing a Private Plugin from a Personal Repository

```bash
# Set up the environment variable with your token
export WDM_TOKEN_CUSTOM_PLUGIN="your-token-for-custom-plugin"

# Initialize wdm
wdm init

# Add a private plugin from your own repository
wdm add private-plugin --version ^1.0 --repo yourusername/private-plugin --token-env WDM_TOKEN_CUSTOM_PLUGIN

# Install all dependencies
wdm install
```

### Using Multiple Private Dependencies with Different Tokens

```bash
# Set up environment variables for each token
export WDM_TOKEN_CUSTOM_PLUGIN="your-token-for-custom-plugin"
export WDM_TOKEN_ANOTHER_PLUGIN="your-token-for-another-plugin"

# Add the first private plugin
wdm add private-plugin --version ^1.0 --repo yourusername/private-plugin --token-env WDM_TOKEN_CUSTOM_PLUGIN

# Add the second private plugin
wdm add another-plugin --version ^2.0 --repo anotheruser/private-plugin --token-env WDM_TOKEN_ANOTHER_PLUGIN

# Install all dependencies
wdm install
```

### Updating a Private Plugin to a Specific Version

```bash
# Update the version in wdm.yml
# Change the version of private-plugin to "1.2.0"

# Install the updated dependencies
wdm install
```

### Removing a Private Plugin

```bash
# Remove the private plugin
wdm remove private-plugin
```

## Contributing

Contributions are welcome! Please open an issue or submit a pull request on GitHub.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

**Disclaimer:**

When handling tokens and private repositories, always ensure you follow best security practices:

- **Never commit tokens to version control.**
- **Use environment variables to manage sensitive information.**
- **Limit the scopes and permissions of your tokens to only what is necessary.**
