# RCraft - Minecraft CLI Launcher

🦀 An ultra-lightweight Minecraft launcher in CLI, written in Rust, which automatically downloads the necessary files and runs the game.

## Requirements

- Rust (latest stable version recommended).
- Internet connection for downloading Minecraft files.
- Java Runtime Environment (JRE) installed on the system (automatically detected).

## Installation

1. Navigate to the project directory:
   ```bash
   cd RCraft
   ```
2. Install dependencies:
   ```bash
   cargo build --release
   ```
   This compiles the project with all dependencies.

## Usage

Run the launcher with positional arguments:
```bash
./rcraft <username> <minecraft_version>
```

Example:
```bash
./rcraft vdkvdev 1.13
```

## Notes

- The launcher detects Java automatically. Ensure Java is installed and in your PATH.
- Downloads are stored in a local `.minecraft` directory structure for persistence.
- Tested on Unix-like systems (Linux, macOS). Windows may require adjustments for terminal commands.

## License

This project is licensed under the GNU General Public License v3.0 (GPL-3.0).
For more details, see the [LICENSE](LICENSE) file in the repository.
