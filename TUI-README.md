# PFCP Proxy TUI Manager

A Terminal User Interface (TUI) application for managing the PFCP Proxy/Load Balancer and UPF backends locally.

## Overview

The TUI Manager provides an interactive, user-friendly interface to control and monitor PFCP services, replacing the command-line script `run-local.sh` with a full-featured terminal application.

## Features

- **Real-time Service Management**: Start, stop, and restart services with a single keypress
- **Live Status Monitoring**: View service states, PIDs, and addresses at a glance
- **Interactive Log Viewer**: Scroll through logs from all services with auto-scroll support
- **Test Execution**: Run various PFCP test scenarios interactively
- **Build Integration**: Build the project directly from the TUI
- **Tab-based Interface**: Organized layout with Dashboard, Logs, Tests, and Help tabs

## Installation

Build the TUI runner:

```bash
cargo build --release --bin tui-runner
```

The binary will be available at `target/release/tui-runner`.

## Usage

Start the TUI application:

```bash
./target/release/tui-runner
```

Or run directly with cargo:

```bash
cargo run --release --bin tui-runner
```

## Interface Layout

The TUI consists of three main sections:

### 1. Tab Bar (Top)
Switch between different views:
- **Dashboard**: Service status and recent logs
- **Logs**: Full log viewer with scrolling
- **Tests**: Test scenario execution
- **Help**: Keyboard shortcuts and configuration

### 2. Content Area (Middle)
Displays content based on the selected tab.

### 3. Status Bar (Bottom)
Shows messages and current application status.

## Keyboard Shortcuts

### Global Commands

| Key | Action |
|-----|--------|
| `Tab` | Switch to next tab |
| `q` | Quit application |
| `h` | Jump to Help tab |

### Service Control

| Key | Action |
|-----|--------|
| `s` | Start all services |
| `x` | Stop all services |
| `r` | Restart all services |
| `1` | Toggle PFCP Proxy (start/stop) |
| `2` | Toggle UPF1 (start/stop) |
| `3` | Toggle UPF2 (start/stop) |
| `4` | Toggle UPF3 (start/stop) |

### Build & Maintenance

| Key | Action |
|-----|--------|
| `Ctrl+B` | Build project |
| `Ctrl+C` | Clean logs |

### Logs Tab

| Key | Action |
|-----|--------|
| `↑` | Scroll logs up |
| `↓` | Scroll logs down |
| `a` | Enable auto-scroll |

### Tests Tab

| Key | Action |
|-----|--------|
| `↑` | Select previous test |
| `↓` | Select next test |
| `Enter` | Run selected test |

## Service Configuration

The TUI manages the following services:

| Service | Address | Description |
|---------|---------|-------------|
| **pfcp-proxy** | 127.0.0.1:8805 | PFCP Proxy/Load Balancer |
| **upf1** | 127.0.0.1:8806 | UPF Backend 1 |
| **upf2** | 127.0.0.1:8807 | UPF Backend 2 |
| **upf3** | 127.0.0.1:8808 | UPF Backend 3 |

## Test Scenarios

The TUI provides access to the following test scenarios:

1. **Heartbeat Test**: Sends 5 heartbeat messages to test basic connectivity
2. **Basic Session Test**: Creates 10 test sessions to verify session management
3. **Load Balance Test**: Creates 30 sessions to test load balancing across UPFs
4. **Session Lifecycle Test**: Creates 20 sessions and tests deletion functionality
5. **Full Test**: Comprehensive test including heartbeats and session management

## Service States

Services can be in the following states:

- ● **Stopped** (Gray): Service is not running
- ◐ **Starting** (Yellow): Service is being started
- ● **Running** (Green): Service is active and running
- ◑ **Stopping** (Yellow): Service is being stopped
- ● **Error** (Red): Service encountered an error

## Dashboard View

The Dashboard provides:
- **Service Panel**: Shows status, PID, and address for each service
- **Recent Logs**: Displays the most recent log entries from all services

## Logs View

The Logs view offers:
- Full log history (up to 1000 lines)
- Scroll through logs with arrow keys
- Auto-scroll mode to follow new log entries
- Service-tagged log entries for easy identification

## Tests View

The Tests view allows:
- Navigation through available test scenarios
- One-click test execution
- Test results displayed in logs
- Visual indicator for selected test

## Directory Structure

The TUI creates and manages the following directories:

- `.pids/`: Process ID files for running services
- `.logs/`: Log files for each service
- `target/release/`: Built binaries

## Comparison with run-local.sh

| Feature | run-local.sh | TUI Manager |
|---------|--------------|-------------|
| Service control | Command-line args | Single keypress |
| Status viewing | Separate command | Real-time display |
| Log viewing | External tail | Built-in viewer |
| Test execution | Menu prompts | Interactive selection |
| Visual feedback | Text messages | Colored status indicators |
| Multitasking | Sequential | Concurrent monitoring |

## Troubleshooting

### Binaries not found
If you see "Binaries not found", press `Ctrl+B` to build the project first.

### Service won't start
- Check if the port is already in use
- Verify binaries exist in `target/release/`
- Review logs in the Logs tab

### Logs not updating
- Logs update every 2 seconds automatically
- Use `a` key in Logs tab to enable auto-scroll
- Check if services are actually running

## Technical Details

### Implementation

- **Framework**: Built with `ratatui` (modern TUI framework)
- **Backend**: `crossterm` for cross-platform terminal control
- **Language**: Rust
- **Async**: No async runtime needed for TUI (uses polling)

### Performance

- Minimal CPU usage when idle
- Efficient log buffering (circular buffer)
- Non-blocking UI updates
- Process management via system calls

### Platform Support

- Linux: Full support (uses `kill` command)
- macOS: Full support (uses `kill` command)
- Windows: Partial support (uses `taskkill`)

## Development

To modify the TUI:

1. Edit `src/bin/tui-runner.rs`
2. Build with `cargo build --release --bin tui-runner`
3. Test with `cargo run --release --bin tui-runner`

## Future Enhancements

Potential improvements:
- [ ] Real-time metrics dashboard
- [ ] Log filtering and search
- [ ] Service configuration editing
- [ ] Export logs to file
- [ ] Custom test scenarios
- [ ] Network statistics visualization

## License

Same as the main project (Apache-2.0).

## Contributing

Contributions welcome! Please ensure:
- Code follows Rust best practices
- UI remains responsive
- Keyboard shortcuts are documented
- Cross-platform compatibility is maintained
