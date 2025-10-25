# Installation Guide

This guide covers all installation methods for quickctx.

## Quick Install (Recommended)

### Linux, macOS, Android (Termux)

Using curl:

```bash
curl -fsSL https://raw.githubusercontent.com/CaddyGlow/quickctx/main/install.sh | bash
```

Using wget:

```bash
wget -qO- https://raw.githubusercontent.com/CaddyGlow/quickctx/main/install.sh | bash
```

**Custom installation directory:**

```bash
export QUICKCTX_INSTALL_DIR="$HOME/bin"
curl -fsSL https://raw.githubusercontent.com/CaddyGlow/quickctx/main/install.sh | bash
```

Default installation directory: `~/.local/bin`

### Windows

Open PowerShell and run:

```powershell
irm https://raw.githubusercontent.com/CaddyGlow/quickctx/main/install.ps1 | iex
```

Default installation directory: `%LOCALAPPDATA%\Programs\quickctx`

## Other Installation Methods

### Homebrew (macOS/Linux)

```bash
brew tap CaddyGlow/packages
brew install quickctx
```

Or install directly without tapping:

```bash
brew install CaddyGlow/packages/quickctx
```

### Scoop (Windows)

```powershell
scoop bucket add quickctx https://github.com/CaddyGlow/homebrew-packages
scoop install quickctx
```

### Using Cargo

From crates.io (when published):

```bash
cargo install quickctx
```

Using cargo-binstall (faster, downloads pre-built binaries):

```bash
cargo binstall quickctx
```

### Manual Installation

1. **Download the latest release:**
   - Visit: https://github.com/CaddyGlow/quickctx/releases/latest
   - Download the appropriate archive for your platform:
     - Linux: `quickctx-x86_64-unknown-linux-gnu.tar.gz`
     - macOS: `quickctx-x86_64-apple-darwin.tar.gz`
     - Windows: `quickctx-x86_64-pc-windows-msvc.zip`
     - ARM Linux: `quickctx-aarch64-unknown-linux-gnu.tar.gz`
     - Android: `quickctx-aarch64-linux-android.tar.gz`

2. **Extract the archive:**

   ```bash
   # Linux/macOS (tar.gz)
   tar -xzf quickctx-*.tar.gz

   # Windows (zip)
   # Use Explorer or: Expand-Archive quickctx-*.zip
   ```

3. **Move to a directory in your PATH:**

   ```bash
   # Linux/macOS
   mv quickctx ~/.local/bin/

   # Windows (PowerShell)
   Move-Item quickctx.exe $env:LOCALAPPDATA\Programs\quickctx\
   ```

### From Source

Requirements:

- Rust 1.75 or newer (Rust 2024 edition)
- Git

Steps:

```bash
git clone https://github.com/CaddyGlow/quickctx.git
cd quickctx
cargo build --release
```

Binaries will be available at:

- `target/release/quickctx`
- `target/release/quickctx-analyze`

Copy them to a directory in your PATH.

## Platform-Specific Notes

### Linux

The install script automatically detects glibc vs musl and selects the appropriate binary.

**Add to PATH (if needed):**

```bash
# For bash
echo 'export PATH="$PATH:$HOME/.local/bin"' >> ~/.bashrc
source ~/.bashrc

# For zsh
echo 'export PATH="$PATH:$HOME/.local/bin"' >> ~/.zshrc
source ~/.zshrc

# For fish
fish_add_path ~/.local/bin
```

### macOS

You may need to allow the binary in System Preferences → Security & Privacy if you get a security warning on first run.

**Alternative: Remove quarantine attribute:**

```bash
xattr -d com.apple.quarantine ~/.local/bin/quickctx
```

### Windows

The installer automatically adds the installation directory to your PATH. You may need to restart your terminal for the changes to take effect.

**Manual PATH update:**

1. Open System Properties → Environment Variables
2. Edit the User `Path` variable
3. Add: `%LOCALAPPDATA%\Programs\quickctx`

### Android (Termux)

1. Install Termux from F-Droid (not Play Store)
2. Update packages: `pkg update && pkg upgrade`
3. Install curl: `pkg install curl`
4. Run the install script:
   ```bash
   curl -fsSL https://raw.githubusercontent.com/CaddyGlow/quickctx/main/install.sh | bash
   ```

## Updating

Quickctx includes a built-in self-update feature (works with binary installations):

```bash
# Check for updates
quickctx update --check-only

# Update interactively (with confirmation)
quickctx update

# Update automatically (no confirmation)
quickctx update --yes
```

**Automatic update checks:**

- Quickctx checks for updates every 7 days
- Non-intrusive notification if an update is available
- Doesn't interrupt or slow down normal operations

**For package manager installations:**

```bash
# Homebrew
brew upgrade quickctx

# Scoop
scoop update quickctx

# Cargo
cargo install quickctx --force  # or
cargo binstall quickctx --force
```

## Verification

After installation, verify it works:

```bash
quickctx --version
quickctx --help
```

Test the copy functionality:

```bash
echo "fn main() {}" | quickctx copy -
```

## Troubleshooting

### "command not found" error

The installation directory is not in your PATH. See platform-specific PATH instructions above.

### Permission denied

Make the binary executable:

```bash
chmod +x ~/.local/bin/quickctx
```

### Windows SmartScreen warning

This is normal for new executables. Click "More info" → "Run anyway".

### SSL/TLS certificate errors

Update your system's CA certificates:

```bash
# Debian/Ubuntu
sudo apt update && sudo apt install ca-certificates

# Fedora/RHEL
sudo dnf install ca-certificates

# macOS
# Usually not needed, but you can update with:
brew install ca-certificates
```

### Cannot find binary for your platform

Check available releases at: https://github.com/CaddyGlow/quickctx/releases

If your platform isn't available, you can:

1. Build from source (see instructions above)
2. Open an issue requesting support for your platform

## Uninstallation

### Homebrew

```bash
brew uninstall quickctx
```

### Scoop

```powershell
scoop uninstall quickctx
```

### Script installation

```bash
# Linux/macOS
rm ~/.local/bin/quickctx
rm -rf ~/.config/quickctx  # Remove config/cache

# Windows (PowerShell)
Remove-Item $env:LOCALAPPDATA\Programs\quickctx -Recurse
# Manually remove from PATH via System Properties
```

### Cargo installation

```bash
cargo uninstall quickctx
```

## Support

- Report issues: https://github.com/CaddyGlow/quickctx/issues
- View documentation: https://github.com/CaddyGlow/quickctx
- Check releases: https://github.com/CaddyGlow/quickctx/releases
