# RCraft - Minecraft CLI Launcher

🦀 An ultra-lightweight Minecraft launcher in CLI, written in Rust, which automatically downloads the necessary files and runs the game.

## Demo
<div align="center">
  <img src="screenshot/screenshot.png" alt="Demo RCraft"/>
</div>

## Requirements

- Rust (latest stable version recommended).
- Internet connection for downloading Minecraft files.
- Java Runtime Environment (JRE) installed on the system (automatically detected).

## Usage

> [!warning]
> RCraft is currently in beta v0.4, so you may encounter bugs. be gentle with it.

Run the launcher with positional arguments:
```bash
./RCraft <username> <minecraft_version> <ram_mb>
```

**Recommendation:** Before executing the binary, it's recommended to run `sudo chmod 777 RCraft` to ensure proper permissions.

Example:
```bash
./RCraft vdkvdev 1.21.8 8192
```

> [!Note]
> Currently, RCraft v0.4 (beta) supports Linux only
> For now, supports versions 1.8 and above only.
> Downloads are stored in a local `.minecraft` directory structure for persistence.
> The launcher detects Java automatically. Ensure Java is installed and in your PATH.

## Upcoming Features

- Support for Windows and macOS

## License

This project is licensed under the GNU General Public License v3.0 (GPL-3.0).
For more details, see the [LICENSE](LICENSE) file in the repository.
