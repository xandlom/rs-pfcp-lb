//! Simple PFCP UPF Server for Testing
//!
//! A minimal UPF simulator that responds to PFCP messages for testing the proxy.

use clap::Parser;
use rs_pfcp::message;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::net::UdpSocket;

#[derive(Parser, Debug)]
#[command(name = "test-upf")]
#[command(about = "Simple PFCP UPF Server for testing")]
struct Args {
    /// Listen address
    #[arg(short, long, default_value = "0.0.0.0:8805")]
    listen: String,

    /// UPF name/identifier
    #[arg(short, long, default_value = "test-upf-1")]
    name: String,
}

struct UpfStats {
    total_messages: AtomicU64,
    heartbeats: AtomicU64,
    sessions: AtomicU64,
}

impl UpfStats {
    fn new() -> Self {
        Self {
            total_messages: AtomicU64::new(0),
            heartbeats: AtomicU64::new(0),
            sessions: AtomicU64::new(0),
        }
    }

    fn print(&self, upf_name: &str) {
        println!("\n[{}] Statistics:", upf_name);
        println!("  Total Messages:      {}", self.total_messages.load(Ordering::Relaxed));
        println!("  Heartbeats:          {}", self.heartbeats.load(Ordering::Relaxed));
        println!("  Session Messages:    {}", self.sessions.load(Ordering::Relaxed));
    }
}

async fn handle_message(
    data: Vec<u8>,
    src: SocketAddr,
    socket: Arc<UdpSocket>,
    upf_name: String,
    stats: Arc<UpfStats>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Parse message to get type and sequence
    let msg = message::parse(&data)?;
    let msg_type = msg.msg_type();
    let sequence = msg.sequence();
    let seid = msg.seid();

    stats.total_messages.fetch_add(1, Ordering::Relaxed);

    println!(
        "[{}] 📥 Received {:?} from {} (seq: {}, SEID: {:?})",
        upf_name, msg_type, src, sequence, seid
    );

    // Track message types
    match msg_type {
        message::MsgType::HeartbeatRequest => {
            stats.heartbeats.fetch_add(1, Ordering::Relaxed);
        }
        message::MsgType::SessionEstablishmentRequest
        | message::MsgType::SessionModificationRequest
        | message::MsgType::SessionDeletionRequest => {
            stats.sessions.fetch_add(1, Ordering::Relaxed);
            if let Some(s) = seid {
                println!("[{}]    ✓ Session SEID: {:#x}", upf_name, s);
            }
        }
        _ => {}
    }

    // For testing, we'll just echo back a simple response
    // In a real implementation, we would build proper PFCP responses
    // For now, just acknowledge receipt
    println!("[{}]    ✓ Processed message", upf_name);

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("\n🔵 Starting Test UPF Server: {}", args.name);
    println!("   Listen address: {}", args.listen);

    let socket = Arc::new(UdpSocket::bind(&args.listen).await?);
    let local_addr = socket.local_addr()?;
    println!("   ✓ Listening on {}\n", local_addr);

    let stats = Arc::new(UpfStats::new());

    // Spawn stats reporter
    {
        let stats = stats.clone();
        let name = args.name.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                stats.print(&name);
            }
        });
    }

    // Main message loop
    let mut buf = vec![0u8; 65536];
    loop {
        let (len, src) = socket.recv_from(&mut buf).await?;
        let data = buf[..len].to_vec();

        let socket_clone = socket.clone();
        let upf_name = args.name.clone();
        let stats_clone = stats.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_message(data, src, socket_clone, upf_name, stats_clone).await {
                eprintln!("Error handling message: {}", e);
            }
        });
    }
}
