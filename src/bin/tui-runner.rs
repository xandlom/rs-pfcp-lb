use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap},
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File},
    io::{self, BufRead, BufReader},
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

// Configuration constants
const PROXY_ADDR: &str = "127.0.0.1:8805";
const UPF1_ADDR: &str = "127.0.0.1:8806";
const UPF2_ADDR: &str = "127.0.0.1:8807";
const UPF3_ADDR: &str = "127.0.0.1:8808";
const MAX_LOG_LINES: usize = 1000;
const STATS_FILE_PATH: &str = "/tmp/pfcp-proxy-stats.json";

// Proxy statistics structures (mirroring the ones in statistics.rs)
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct ProxyStats {
    pub timestamp: String,
    pub total_messages_received: u64,
    pub total_messages_sent: u64,
    pub total_responses_forwarded: u64,
    pub responses_dropped: u64,
    pub active_sessions: usize,
    pub sessions_established: u64,
    pub sessions_deleted: u64,
    pub routed_by_seid: u64,
    pub routed_by_load_balance: u64,
    pub broadcasts: u64,
    pub message_types: HashMap<String, u64>,
    pub upf_stats: Vec<UpfStats>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct UpfStats {
    pub address: String,
    pub messages_sent: u64,
    pub active_sessions: usize,
    pub health: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum ServiceStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

impl ServiceStatus {
    fn color(&self) -> Color {
        match self {
            ServiceStatus::Stopped => Color::Gray,
            ServiceStatus::Starting => Color::Yellow,
            ServiceStatus::Running => Color::Green,
            ServiceStatus::Stopping => Color::Yellow,
            ServiceStatus::Error => Color::Red,
        }
    }

    fn symbol(&self) -> &'static str {
        match self {
            ServiceStatus::Stopped => "●",
            ServiceStatus::Starting => "◐",
            ServiceStatus::Running => "●",
            ServiceStatus::Stopping => "◑",
            ServiceStatus::Error => "●",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            ServiceStatus::Stopped => "Stopped",
            ServiceStatus::Starting => "Starting",
            ServiceStatus::Running => "Running",
            ServiceStatus::Stopping => "Stopping",
            ServiceStatus::Error => "Error",
        }
    }
}

#[derive(Debug)]
struct ServiceInfo {
    name: String,
    status: ServiceStatus,
    pid: Option<u32>,
    addr: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Dashboard,
    Logs,
    Tests,
    UPFs,
    Help,
}

impl Tab {
    fn all() -> Vec<Tab> {
        vec![Tab::Dashboard, Tab::Logs, Tab::Tests, Tab::UPFs, Tab::Help]
    }

    fn title(&self) -> &'static str {
        match self {
            Tab::Dashboard => "Dashboard",
            Tab::Logs => "Logs",
            Tab::Tests => "Tests",
            Tab::UPFs => "UPF Management",
            Tab::Help => "Help",
        }
    }
}

struct App {
    services: Vec<ServiceInfo>,
    current_tab: Tab,
    logs: VecDeque<String>,
    log_scroll: usize,
    message: Option<(String, Instant)>,
    pid_dir: PathBuf,
    log_dir: PathBuf,
    build_dir: PathBuf,
    should_quit: bool,
    selected_test: usize,
    auto_scroll_logs: bool,
    proxy_stats: Option<ProxyStats>,
    // UPF Management
    upf_input: String,
    upf_input_mode: bool,
    upf_list: Vec<String>,
    selected_upf: usize,
}

impl App {
    fn new() -> io::Result<Self> {
        let base_dir = std::env::current_dir()?;
        let pid_dir = base_dir.join(".pids");
        let log_dir = base_dir.join(".logs");
        let build_dir = base_dir.join("target/release");

        // Create directories
        fs::create_dir_all(&pid_dir)?;
        fs::create_dir_all(&log_dir)?;

        Ok(App {
            services: vec![
                ServiceInfo {
                    name: "pfcp-proxy".to_string(),
                    status: ServiceStatus::Stopped,
                    pid: None,
                    addr: PROXY_ADDR.to_string(),
                },
                ServiceInfo {
                    name: "upf1".to_string(),
                    status: ServiceStatus::Stopped,
                    pid: None,
                    addr: UPF1_ADDR.to_string(),
                },
                ServiceInfo {
                    name: "upf2".to_string(),
                    status: ServiceStatus::Stopped,
                    pid: None,
                    addr: UPF2_ADDR.to_string(),
                },
                ServiceInfo {
                    name: "upf3".to_string(),
                    status: ServiceStatus::Stopped,
                    pid: None,
                    addr: UPF3_ADDR.to_string(),
                },
            ],
            current_tab: Tab::Dashboard,
            logs: VecDeque::with_capacity(MAX_LOG_LINES),
            log_scroll: 0,
            message: None,
            pid_dir,
            log_dir,
            build_dir,
            should_quit: false,
            selected_test: 0,
            auto_scroll_logs: true,
            proxy_stats: None,
            upf_input: String::new(),
            upf_input_mode: false,
            upf_list: vec![
                UPF1_ADDR.to_string(),
                UPF2_ADDR.to_string(),
                UPF3_ADDR.to_string(),
            ],
            selected_upf: 0,
        })
    }

    /// Load proxy statistics from JSON file
    fn load_proxy_stats(&mut self) {
        if let Ok(data) = fs::read_to_string(STATS_FILE_PATH) {
            if let Ok(stats) = serde_json::from_str::<ProxyStats>(&data) {
                self.proxy_stats = Some(stats);
            }
        }
    }

    fn update_service_status(&mut self) {
        for service in &mut self.services {
            let pid_file = self.pid_dir.join(format!("{}.pid", service.name));

            if let Ok(pid_str) = fs::read_to_string(&pid_file) {
                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                    // Check if process is running
                    if process_exists(pid) {
                        service.status = ServiceStatus::Running;
                        service.pid = Some(pid);
                    } else {
                        service.status = ServiceStatus::Stopped;
                        service.pid = None;
                        // Clean up stale PID file
                        let _ = fs::remove_file(&pid_file);
                    }
                } else {
                    service.status = ServiceStatus::Stopped;
                    service.pid = None;
                }
            } else {
                service.status = ServiceStatus::Stopped;
                service.pid = None;
            }
        }
    }

    fn start_service(&mut self, service_idx: usize) -> io::Result<()> {
        if service_idx >= self.services.len() {
            return Ok(());
        }

        let service_name = self.services[service_idx].name.clone();
        let service_addr = self.services[service_idx].addr.clone();
        let service_status = self.services[service_idx].status;

        let pid_file = self.pid_dir.join(format!("{}.pid", service_name));
        let log_file = self.log_dir.join(format!("{}.log", service_name));

        // Check if already running
        if service_status == ServiceStatus::Running {
            self.set_message(format!("{} is already running", service_name));
            return Ok(());
        }

        // Start the service
        let child = if service_name == "pfcp-proxy" {
            Command::new(self.build_dir.join("pfcp-proxy"))
                .arg("--listen")
                .arg(PROXY_ADDR)
                .arg("--backends")
                .arg(format!("{},{},{}", UPF1_ADDR, UPF2_ADDR, UPF3_ADDR))
                .arg("--strategy")
                .arg("round-robin")
                .arg("--stats-interval")
                .arg("10")
                .arg("--health-check-interval")
                .arg("5")
                .arg("--log-level")
                .arg("info")
                .stdout(Stdio::from(File::create(&log_file)?))
                .stderr(Stdio::from(File::create(format!("{}.err", log_file.display()))?))
                .spawn()?
        } else {
            // UPF backend
            Command::new(self.build_dir.join("test-upf"))
                .arg("--listen")
                .arg(&service_addr)
                .arg("--name")
                .arg(&service_name)
                .stdout(Stdio::from(File::create(&log_file)?))
                .stderr(Stdio::from(File::create(format!("{}.err", log_file.display()))?))
                .spawn()?
        };

        let pid = child.id();
        fs::write(&pid_file, pid.to_string())?;

        self.services[service_idx].status = ServiceStatus::Starting;
        self.services[service_idx].pid = Some(pid);
        self.set_message(format!("Started {} (PID: {})", service_name, pid));

        Ok(())
    }

    fn stop_service(&mut self, service_idx: usize) -> io::Result<()> {
        if service_idx >= self.services.len() {
            return Ok(());
        }

        let service_name = self.services[service_idx].name.clone();
        let service_status = self.services[service_idx].status;
        let service_pid = self.services[service_idx].pid;

        let pid_file = self.pid_dir.join(format!("{}.pid", service_name));

        if service_status != ServiceStatus::Running {
            self.set_message(format!("{} is not running", service_name));
            return Ok(());
        }

        if let Some(pid) = service_pid {
            // Try graceful shutdown first
            kill_process(pid, false)?;

            // Wait a bit for graceful shutdown
            std::thread::sleep(Duration::from_millis(500));

            // Force kill if still running
            if process_exists(pid) {
                kill_process(pid, true)?;
            }

            self.services[service_idx].status = ServiceStatus::Stopped;
            self.services[service_idx].pid = None;
            let _ = fs::remove_file(&pid_file);

            self.set_message(format!("Stopped {}", service_name));
        }

        Ok(())
    }

    fn start_all(&mut self) -> io::Result<()> {
        // Check if binaries exist
        if !self.build_dir.join("pfcp-proxy").exists() || !self.build_dir.join("test-upf").exists() {
            self.set_message("Binaries not found. Please build the project first (Ctrl+B)".to_string());
            return Ok(());
        }

        // Start UPF backends first
        for i in 1..4 {
            self.start_service(i)?;
        }

        // Wait a bit for UPFs to start
        std::thread::sleep(Duration::from_millis(1000));

        // Start proxy
        self.start_service(0)?;

        self.set_message("All services started".to_string());
        Ok(())
    }

    fn stop_all(&mut self) -> io::Result<()> {
        // Stop proxy first
        self.stop_service(0)?;

        // Stop UPF backends
        for i in 1..4 {
            self.stop_service(i)?;
        }

        self.set_message("All services stopped".to_string());
        Ok(())
    }

    fn restart_all(&mut self) -> io::Result<()> {
        self.stop_all()?;
        std::thread::sleep(Duration::from_millis(1000));
        self.start_all()?;
        Ok(())
    }

    fn build_project(&mut self) -> io::Result<()> {
        self.set_message("Building project... (this may take a while)".to_string());

        // Run cargo build in a separate thread/process
        let output = Command::new("cargo")
            .arg("build")
            .arg("--release")
            .output()?;

        if output.status.success() {
            self.set_message("Build successful!".to_string());
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            self.set_message(format!("Build failed: {}", stderr.lines().next().unwrap_or("Unknown error")));
        }

        Ok(())
    }

    fn run_test(&mut self, test_idx: usize) -> io::Result<()> {
        let test_smf = self.build_dir.join("test-smf");
        if !test_smf.exists() {
            self.set_message("test-smf binary not found. Build the project first.".to_string());
            return Ok(());
        }

        // Check if proxy is running
        if self.services[0].status != ServiceStatus::Running {
            self.set_message("PFCP Proxy is not running. Start services first.".to_string());
            return Ok(());
        }

        let (test_name, args): (&str, Vec<&str>) = match test_idx {
            0 => ("Heartbeat Test", vec!["heartbeat", "--count", "5", "--interval", "1"]),
            1 => ("Basic Session Test", vec!["sessions", "--count", "10"]),
            2 => ("Load Balance Test", vec!["load-balance", "--sessions", "30"]),
            3 => ("Session Lifecycle Test", vec!["sessions", "--count", "20", "--delete"]),
            4 => ("Full Test", vec!["full", "--sessions", "20"]),
            _ => return Ok(()),
        };

        self.set_message(format!("Running {}...", test_name));

        // Run test and capture output
        let mut cmd = Command::new(&test_smf);
        cmd.arg("--target").arg(PROXY_ADDR);
        for arg in args {
            cmd.arg(arg);
        }

        let output = cmd.output()?;

        if output.status.success() {
            self.set_message(format!("{} completed successfully!", test_name));
            // Add output to logs
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                self.add_log(format!("[TEST] {}", line));
            }
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            self.set_message(format!("{} failed: {}", test_name, stderr.lines().next().unwrap_or("Unknown error")));
        }

        Ok(())
    }

    fn clean_logs(&mut self) -> io::Result<()> {
        // Remove log files
        if self.log_dir.exists() {
            fs::remove_dir_all(&self.log_dir)?;
            fs::create_dir_all(&self.log_dir)?;
        }

        // Clear in-memory logs
        self.logs.clear();
        self.log_scroll = 0;

        self.set_message("Logs cleaned".to_string());
        Ok(())
    }

    fn update_logs(&mut self) -> io::Result<()> {
        // Read logs from all service log files
        let services: Vec<String> = self.services.iter().map(|s| s.name.clone()).collect();
        for service_name in services {
            let log_file = self.log_dir.join(format!("{}.log", service_name));
            if log_file.exists() {
                if let Ok(file) = File::open(&log_file) {
                    let reader = BufReader::new(file);
                    let lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();

                    // Add new lines (this is simplified - in production you'd track position)
                    for line in lines.iter().rev().take(10).rev() {
                        self.add_log(format!("[{}] {}", service_name, line));
                    }
                }
            }
        }

        Ok(())
    }

    fn add_log(&mut self, line: String) {
        if self.logs.len() >= MAX_LOG_LINES {
            self.logs.pop_front();
        }
        self.logs.push_back(line);

        if self.auto_scroll_logs {
            self.log_scroll = self.logs.len().saturating_sub(1);
        }
    }

    fn set_message(&mut self, msg: String) {
        self.message = Some((msg, Instant::now()));
    }

    fn get_message(&self) -> Option<String> {
        if let Some((msg, time)) = &self.message {
            if time.elapsed() < Duration::from_secs(5) {
                return Some(msg.clone());
            }
        }
        None
    }

    /// Send UPF add/remove command to the proxy via a control file
    fn send_upf_command(&self, action: &str, addr: &str) -> io::Result<()> {
        // Write command to a control file that the proxy can read
        let control_file = self.build_dir.parent().unwrap().join(".upf_control");
        let command = format!("{}:{}\n", action, addr);

        // Append to control file
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&control_file)?;

        file.write_all(command.as_bytes())?;
        file.flush()?;

        Ok(())
    }

    fn scroll_logs_up(&mut self) {
        self.auto_scroll_logs = false;
        self.log_scroll = self.log_scroll.saturating_sub(1);
    }

    fn scroll_logs_down(&mut self) {
        self.auto_scroll_logs = false;
        self.log_scroll = (self.log_scroll + 1).min(self.logs.len().saturating_sub(1));
    }

    fn enable_auto_scroll(&mut self) {
        self.auto_scroll_logs = true;
        self.log_scroll = self.logs.len().saturating_sub(1);
    }
}

fn process_exists(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use std::process::Command;
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[cfg(windows)]
    {
        use std::process::Command;
        Command::new("tasklist")
            .arg("/FI")
            .arg(format!("PID eq {}", pid))
            .output()
            .map(|o| {
                let output = String::from_utf8_lossy(&o.stdout);
                output.contains(&pid.to_string())
            })
            .unwrap_or(false)
    }
}

fn kill_process(pid: u32, force: bool) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::process::Command;
        let signal = if force { "-9" } else { "-15" };
        Command::new("kill")
            .arg(signal)
            .arg(pid.to_string())
            .output()?;
    }

    #[cfg(windows)]
    {
        use std::process::Command;
        let mut cmd = Command::new("taskkill");
        cmd.arg("/PID").arg(pid.to_string());
        if force {
            cmd.arg("/F");
        }
        cmd.output()?;
    }

    Ok(())
}

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tabs
            Constraint::Min(0),     // Content
            Constraint::Length(3),  // Status bar
        ])
        .split(f.area());

    // Render tabs
    let titles: Vec<Line> = Tab::all()
        .iter()
        .map(|t| Line::from(t.title()))
        .collect();

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("PFCP Proxy TUI Manager"))
        .select(Tab::all().iter().position(|t| t == &app.current_tab).unwrap())
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    f.render_widget(tabs, chunks[0]);

    // Render content based on current tab
    match app.current_tab {
        Tab::Dashboard => render_dashboard(f, app, chunks[1]),
        Tab::Logs => render_logs(f, app, chunks[1]),
        Tab::Tests => render_tests(f, app, chunks[1]),
        Tab::UPFs => render_upfs(f, app, chunks[1]),
        Tab::Help => render_help(f, app, chunks[1]),
    }

    // Render status bar
    render_status_bar(f, app, chunks[2]);
}

fn render_dashboard(f: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),  // Services
            Constraint::Min(0),     // Stats area
        ])
        .split(area);

    // Services panel
    let service_items: Vec<ListItem> = app.services.iter().map(|s| {
        let status_span = Span::styled(
            format!("{} ", s.status.symbol()),
            Style::default().fg(s.status.color()),
        );
        let name_span = Span::styled(
            format!("{:<12}", s.name),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        );
        let status_text = Span::styled(
            format!("{:<10}", s.status.label()),
            Style::default().fg(s.status.color()),
        );
        let pid_span = if let Some(pid) = s.pid {
            Span::styled(format!("PID: {:<6}", pid), Style::default().fg(Color::Gray))
        } else {
            Span::styled("          ", Style::default())
        };
        let addr_span = Span::styled(
            format!(" {}", s.addr),
            Style::default().fg(Color::Cyan),
        );

        ListItem::new(Line::from(vec![
            status_span,
            name_span,
            status_text,
            pid_span,
            addr_span,
        ]))
    }).collect();

    let services = List::new(service_items)
        .block(Block::default().borders(Borders::ALL).title("Services"));

    f.render_widget(services, chunks[0]);

    // Stats area - split into left (proxy stats) and right (UPF stats)
    let stats_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),  // Proxy stats
            Constraint::Percentage(50),  // UPF stats
        ])
        .split(chunks[1]);

    // Render proxy statistics
    render_proxy_stats(f, app, stats_chunks[0]);

    // Render UPF distribution
    render_upf_stats(f, app, stats_chunks[1]);
}

fn render_proxy_stats(f: &mut Frame, app: &App, area: Rect) {
    let stats_text = if let Some(ref stats) = app.proxy_stats {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("Last Updated: ", Style::default().fg(Color::Gray)),
                Span::styled(&stats.timestamp[11..19], Style::default().fg(Color::Yellow)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("GLOBAL METRICS", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::raw("  Messages Received:   "),
                Span::styled(format!("{}", stats.total_messages_received), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::raw("  Messages Sent:       "),
                Span::styled(format!("{}", stats.total_messages_sent), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::raw("  Responses Forwarded: "),
                Span::styled(format!("{}", stats.total_responses_forwarded), Style::default().fg(Color::Cyan)),
            ]),
        ];

        if stats.responses_dropped > 0 {
            lines.push(Line::from(vec![
                Span::raw("  Responses Dropped:   "),
                Span::styled(format!("{}", stats.responses_dropped), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            ]));
        }

        lines.extend(vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("SESSIONS", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::raw("  Active Sessions:     "),
                Span::styled(format!("{}", stats.active_sessions), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::raw("  Established:         "),
                Span::styled(format!("{}", stats.sessions_established), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::raw("  Deleted:             "),
                Span::styled(format!("{}", stats.sessions_deleted), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("ROUTING DECISIONS", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::raw("  Routed by SEID:      "),
                Span::styled(format!("{}", stats.routed_by_seid), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::raw("  Load Balanced:       "),
                Span::styled(format!("{}", stats.routed_by_load_balance), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::raw("  Broadcasts:          "),
                Span::styled(format!("{}", stats.broadcasts), Style::default().fg(Color::Cyan)),
            ]),
        ]);

        // Add top message types if available
        if !stats.message_types.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("TOP MESSAGE TYPES", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            ]));

            let mut msg_types: Vec<_> = stats.message_types.iter().collect();
            msg_types.sort_by(|a, b| b.1.cmp(a.1));

            for (msg_type, count) in msg_types.iter().take(5) {
                let short_name = msg_type.replace("Request", "Req").replace("Response", "Rsp");
                lines.push(Line::from(vec![
                    Span::raw(format!("  {:<25}", if short_name.len() > 25 { &short_name[..25] } else { &short_name })),
                    Span::styled(format!("{:>6}", count), Style::default().fg(Color::Cyan)),
                ]));
            }
        }

        lines
    } else {
        vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("No proxy statistics available", Style::default().fg(Color::Yellow)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Start the proxy service to see realtime stats", Style::default().fg(Color::Gray)),
            ]),
        ]
    };

    let paragraph = Paragraph::new(stats_text)
        .block(Block::default().borders(Borders::ALL).title("Proxy Statistics"))
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

fn render_upf_stats(f: &mut Frame, app: &App, area: Rect) {
    let upf_text = if let Some(ref stats) = app.proxy_stats {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("UPF BACKEND DISTRIBUTION", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
        ];

        if stats.upf_stats.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("  No UPF backends connected", Style::default().fg(Color::Yellow)),
            ]));
        } else {
            for upf in &stats.upf_stats {
                // Extract health status color
                let health_color = if upf.health.contains("Healthy") {
                    Color::Green
                } else if upf.health.contains("Degraded") {
                    Color::Yellow
                } else if upf.health.contains("Unhealthy") {
                    Color::Red
                } else {
                    Color::Gray
                };

                lines.push(Line::from(vec![
                    Span::styled("●", Style::default().fg(health_color)),
                    Span::raw(" "),
                    Span::styled(&upf.address, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                ]));

                lines.push(Line::from(vec![
                    Span::raw("    Messages Sent:    "),
                    Span::styled(format!("{}", upf.messages_sent), Style::default().fg(Color::Cyan)),
                ]));

                lines.push(Line::from(vec![
                    Span::raw("    Active Sessions:  "),
                    Span::styled(format!("{}", upf.active_sessions), Style::default().fg(Color::Yellow)),
                ]));

                lines.push(Line::from(vec![
                    Span::raw("    Health:           "),
                    Span::styled(&upf.health, Style::default().fg(health_color)),
                ]));

                lines.push(Line::from(""));
            }
        }

        lines
    } else {
        vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("No UPF statistics available", Style::default().fg(Color::Yellow)),
            ]),
        ]
    };

    let paragraph = Paragraph::new(upf_text)
        .block(Block::default().borders(Borders::ALL).title("UPF Backends"))
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

fn render_logs(f: &mut Frame, app: &mut App, area: Rect) {
    let log_text: Vec<Line> = app.logs.iter().map(|line| {
        Line::from(line.clone())
    }).collect();

    let paragraph = Paragraph::new(log_text)
        .block(Block::default().borders(Borders::ALL).title(format!(
            "Logs (↑↓ to scroll, 'a' for auto-scroll) [{}/{}]",
            app.log_scroll + 1,
            app.logs.len()
        )))
        .wrap(Wrap { trim: false })
        .scroll((app.log_scroll as u16, 0));

    f.render_widget(paragraph, area);
}

fn render_tests(f: &mut Frame, app: &mut App, area: Rect) {
    let tests = vec![
        "1. Heartbeat Test (5 heartbeats)",
        "2. Basic Session Test (10 sessions)",
        "3. Load Balance Test (30 sessions)",
        "4. Session Lifecycle Test (20 sessions with deletion)",
        "5. Full Test (heartbeat + sessions)",
    ];

    let test_items: Vec<ListItem> = tests.iter().enumerate().map(|(idx, test)| {
        let style = if idx == app.selected_test {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let marker = if idx == app.selected_test { "→ " } else { "  " };
        ListItem::new(Line::from(format!("{}{}", marker, test))).style(style)
    }).collect();

    let list = List::new(test_items)
        .block(Block::default().borders(Borders::ALL).title("Test Scenarios (↑↓ to select, Enter to run)"));

    f.render_widget(list, area);
}

fn render_upfs(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Input area
            Constraint::Min(0),     // UPF list
            Constraint::Length(6),  // Instructions
        ])
        .split(area);

    // Input area
    let input_text = if app.upf_input_mode {
        format!("Add UPF: {} █", app.upf_input)
    } else {
        "Press 'a' to add UPF, ↑/↓ to select, 'd' to delete selected UPF".to_string()
    };

    let input = Paragraph::new(input_text)
        .block(Block::default().borders(Borders::ALL).title("UPF Management"))
        .style(if app.upf_input_mode {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        });

    f.render_widget(input, chunks[0]);

    // UPF list
    let upf_items: Vec<ListItem> = app.upf_list.iter().enumerate().map(|(idx, upf)| {
        let is_selected = idx == app.selected_upf;
        let status = if let Some(ref stats) = app.proxy_stats {
            stats.upf_stats.iter()
                .find(|s| s.address == *upf)
                .map(|s| s.health.clone())
                .unwrap_or_else(|| "Unknown".to_string())
        } else {
            "Unknown".to_string()
        };

        let health_color = if status.contains("Healthy") {
            Color::Green
        } else if status.contains("Degraded") {
            Color::Yellow
        } else if status.contains("Unhealthy") {
            Color::Red
        } else {
            Color::Gray
        };

        let line = Line::from(vec![
            Span::styled(
                if is_selected { "► " } else { "  " },
                Style::default().fg(Color::Yellow)
            ),
            Span::styled("● ", Style::default().fg(health_color)),
            Span::styled(format!("{:<25}", upf), Style::default().fg(Color::White)),
            Span::styled(format!(" [{}]", status), Style::default().fg(health_color)),
        ]);

        ListItem::new(line)
    }).collect();

    let upf_list = List::new(upf_items)
        .block(Block::default().borders(Borders::ALL).title(format!(
            "Configured UPF Backends ({}/64)",
            app.upf_list.len()
        )));

    f.render_widget(upf_list, chunks[1]);

    // Instructions
    let instructions = vec![
        Line::from(vec![
            Span::styled("Controls:", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  a           - Add new UPF (enter address:port, e.g., 127.0.0.1:8809)"),
        Line::from("  d           - Delete selected UPF"),
        Line::from("  ↑/↓         - Navigate UPF list"),
        Line::from("  ESC         - Cancel input"),
    ];

    let help = Paragraph::new(instructions)
        .block(Block::default().borders(Borders::ALL).title("Help"));

    f.render_widget(help, chunks[2]);
}

fn render_help(f: &mut Frame, _app: &mut App, area: Rect) {
    let help_text = vec![
        Line::from(vec![
            Span::styled("Navigation:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  Tab         - Switch between tabs"),
        Line::from("  q           - Quit"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Service Control:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  s           - Start all services"),
        Line::from("  x           - Stop all services"),
        Line::from("  r           - Restart all services"),
        Line::from("  1-4         - Toggle individual service (1=proxy, 2-4=upfs)"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Build & Clean:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  Ctrl+B      - Build project"),
        Line::from("  Ctrl+C      - Clean logs"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Logs Tab:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  ↑/↓         - Scroll logs"),
        Line::from("  a           - Enable auto-scroll"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Tests Tab:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  ↑/↓         - Select test"),
        Line::from("  Enter       - Run selected test"),
        Line::from(""),
        Line::from(vec![
            Span::styled("UPF Management Tab:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  a           - Add new UPF"),
        Line::from("  d           - Delete selected UPF"),
        Line::from("  ↑/↓         - Navigate UPF list"),
        Line::from("  ESC         - Cancel input"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Configuration:", Style::default().fg(Color::Gray)),
        ]),
        Line::from(format!("  Proxy:   {}", PROXY_ADDR)),
        Line::from(format!("  UPF1:    {}", UPF1_ADDR)),
        Line::from(format!("  UPF2:    {}", UPF2_ADDR)),
        Line::from(format!("  UPF3:    {}", UPF3_ADDR)),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title("Help"))
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

fn render_status_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let status_text = if let Some(msg) = app.get_message() {
        msg
    } else {
        let running_count = app.services.iter().filter(|s| s.status == ServiceStatus::Running).count();
        format!("{} services running | Press 'h' for help", running_count)
    };

    let status = Paragraph::new(status_text)
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::White));

    f.render_widget(status, area);
}

fn handle_events(app: &mut App) -> io::Result<()> {
    if event::poll(Duration::from_millis(100))? {
        if let Event::Key(key) = event::read()? {
            match app.current_tab {
                Tab::Dashboard | Tab::Help => {
                    match key.code {
                        KeyCode::Char('q') => app.should_quit = true,
                        KeyCode::Tab => {
                            let current_idx = Tab::all().iter().position(|t| t == &app.current_tab).unwrap();
                            app.current_tab = Tab::all()[(current_idx + 1) % Tab::all().len()];
                        }
                        KeyCode::Char('s') => {
                            app.start_all()?;
                        }
                        KeyCode::Char('x') => {
                            app.stop_all()?;
                        }
                        KeyCode::Char('r') => {
                            app.restart_all()?;
                        }
                        KeyCode::Char('1') => {
                            if app.services[0].status == ServiceStatus::Running {
                                app.stop_service(0)?;
                            } else {
                                app.start_service(0)?;
                            }
                        }
                        KeyCode::Char('2') => {
                            if app.services[1].status == ServiceStatus::Running {
                                app.stop_service(1)?;
                            } else {
                                app.start_service(1)?;
                            }
                        }
                        KeyCode::Char('3') => {
                            if app.services[2].status == ServiceStatus::Running {
                                app.stop_service(2)?;
                            } else {
                                app.start_service(2)?;
                            }
                        }
                        KeyCode::Char('4') => {
                            if app.services[3].status == ServiceStatus::Running {
                                app.stop_service(3)?;
                            } else {
                                app.start_service(3)?;
                            }
                        }
                        KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            app.build_project()?;
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            app.clean_logs()?;
                        }
                        KeyCode::Char('h') => {
                            app.current_tab = Tab::Help;
                        }
                        _ => {}
                    }
                }
                Tab::Logs => {
                    match key.code {
                        KeyCode::Char('q') => app.should_quit = true,
                        KeyCode::Tab => {
                            let current_idx = Tab::all().iter().position(|t| t == &app.current_tab).unwrap();
                            app.current_tab = Tab::all()[(current_idx + 1) % Tab::all().len()];
                        }
                        KeyCode::Up => app.scroll_logs_up(),
                        KeyCode::Down => app.scroll_logs_down(),
                        KeyCode::Char('a') => app.enable_auto_scroll(),
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            app.clean_logs()?;
                        }
                        _ => {}
                    }
                }
                Tab::Tests => {
                    match key.code {
                        KeyCode::Char('q') => app.should_quit = true,
                        KeyCode::Tab => {
                            let current_idx = Tab::all().iter().position(|t| t == &app.current_tab).unwrap();
                            app.current_tab = Tab::all()[(current_idx + 1) % Tab::all().len()];
                        }
                        KeyCode::Up => {
                            app.selected_test = app.selected_test.saturating_sub(1);
                        }
                        KeyCode::Down => {
                            app.selected_test = (app.selected_test + 1).min(4);
                        }
                        KeyCode::Enter => {
                            app.run_test(app.selected_test)?;
                        }
                        _ => {}
                    }
                }
                Tab::UPFs => {
                    if app.upf_input_mode {
                        // Handle input mode
                        match key.code {
                            KeyCode::Char(c) => {
                                app.upf_input.push(c);
                            }
                            KeyCode::Backspace => {
                                app.upf_input.pop();
                            }
                            KeyCode::Enter => {
                                // Try to add the UPF
                                if !app.upf_input.is_empty() {
                                    if app.upf_list.len() >= 64 {
                                        app.set_message("Maximum 64 UPFs reached".to_string());
                                    } else if app.upf_list.contains(&app.upf_input) {
                                        app.set_message(format!("UPF {} already exists", app.upf_input));
                                    } else {
                                        app.upf_list.push(app.upf_input.clone());
                                        app.send_upf_command("add", &app.upf_input)?;
                                        app.set_message(format!("Added UPF: {}", app.upf_input));
                                        app.upf_input.clear();
                                        app.upf_input_mode = false;
                                    }
                                }
                            }
                            KeyCode::Esc => {
                                app.upf_input.clear();
                                app.upf_input_mode = false;
                            }
                            _ => {}
                        }
                    } else {
                        // Normal mode
                        match key.code {
                            KeyCode::Char('q') => app.should_quit = true,
                            KeyCode::Tab => {
                                let current_idx = Tab::all().iter().position(|t| t == &app.current_tab).unwrap();
                                app.current_tab = Tab::all()[(current_idx + 1) % Tab::all().len()];
                            }
                            KeyCode::Char('a') => {
                                app.upf_input_mode = true;
                                app.upf_input.clear();
                            }
                            KeyCode::Char('d') => {
                                if !app.upf_list.is_empty() && app.selected_upf < app.upf_list.len() {
                                    let removed = app.upf_list.remove(app.selected_upf);
                                    app.send_upf_command("remove", &removed)?;
                                    app.set_message(format!("Removed UPF: {}", removed));
                                    if app.selected_upf >= app.upf_list.len() && app.selected_upf > 0 {
                                        app.selected_upf -= 1;
                                    }
                                }
                            }
                            KeyCode::Up => {
                                app.selected_upf = app.selected_upf.saturating_sub(1);
                            }
                            KeyCode::Down => {
                                if !app.upf_list.is_empty() {
                                    app.selected_upf = (app.selected_upf + 1).min(app.upf_list.len() - 1);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new()?;
    app.set_message("Welcome! Press 's' to start services, 'h' for help, 'q' to quit".to_string());

    // Main loop
    let mut last_update = Instant::now();
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        // Update service status periodically
        if last_update.elapsed() > Duration::from_secs(1) {
            app.update_service_status();
            app.load_proxy_stats();
            // Periodically update logs (simplified)
            if last_update.elapsed() > Duration::from_secs(2) {
                let _ = app.update_logs();
            }
            last_update = Instant::now();
        }

        handle_events(&mut app)?;

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
