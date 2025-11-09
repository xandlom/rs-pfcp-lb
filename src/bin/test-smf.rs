//! Simple PFCP SMF Client for Testing
//!
//! A test SMF simulator that sends various PFCP messages to test proxy functionality.

use clap::{Parser, Subcommand};
use rs_pfcp::ie::{NodeId, RecoveryTimeStamp};
use rs_pfcp::message::{
    AssociationSetupRequest, HeartbeatRequest, Message, SessionDeletionRequest,
    SessionEstablishmentRequest,
};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;

#[derive(Parser, Debug)]
#[command(name = "test-smf")]
#[command(about = "Simple PFCP SMF Client for testing the proxy")]
struct Args {
    /// Target address (proxy or UPF)
    #[arg(short, long, default_value = "127.0.0.1:8805")]
    target: String,

    /// Local bind address
    #[arg(short, long, default_value = "0.0.0.0:0")]
    bind: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Send heartbeat requests
    Heartbeat {
        /// Number of heartbeats to send
        #[arg(short, long, default_value = "5")]
        count: u32,

        /// Interval between heartbeats (seconds)
        #[arg(short, long, default_value = "1")]
        interval: u64,
    },

    /// Test session establishment
    Sessions {
        /// Number of sessions to create
        #[arg(short, long, default_value = "10")]
        count: u32,

        /// Whether to delete sessions after creation
        #[arg(short, long)]
        delete: bool,

        /// Delay between operations (milliseconds)
        #[arg(long, default_value = "100")]
        delay: u64,
    },

    /// Test load balancing distribution
    LoadBalance {
        /// Number of sessions to distribute
        #[arg(short, long, default_value = "30")]
        sessions: u32,
    },

    /// Full test scenario
    Full {
        /// Number of sessions
        #[arg(short, long, default_value = "20")]
        sessions: u32,
    },
}

struct TestContext {
    socket: Arc<UdpSocket>,
    target: SocketAddr,
    sequence: Arc<AtomicU32>,
}

impl TestContext {
    async fn new(bind: &str, target: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let socket = UdpSocket::bind(bind).await?;
        let target = tokio::net::lookup_host(target)
            .await?
            .next()
            .ok_or("Failed to resolve target")?;

        Ok(Self {
            socket: Arc::new(socket),
            target,
            sequence: Arc::new(AtomicU32::new(1)),
        })
    }

    fn next_sequence(&self) -> u32 {
        self.sequence.fetch_add(1, Ordering::Relaxed)
    }

    async fn send_heartbeat(&self) -> Result<(), Box<dyn std::error::Error>> {
        let seq = self.next_sequence();
        let request = HeartbeatRequest::builder()
            .sequence(seq)
            .recovery_time_stamp(RecoveryTimeStamp::new(get_recovery_timestamp())?)
            .build()?;

        let data = request.marshal()?;
        self.socket.send_to(&data, self.target).await?;

        println!("📤 Sent HeartbeatRequest (seq: {})", seq);
        Ok(())
    }

    async fn send_association_setup(&self) -> Result<(), Box<dyn std::error::Error>> {
        let seq = self.next_sequence();
        let request = AssociationSetupRequest::builder()
            .sequence(seq)
            .node_id(NodeId::new_ipv4([127, 0, 0, 1])?)
            .recovery_time_stamp(RecoveryTimeStamp::new(get_recovery_timestamp())?)
            .build()?;

        let data = request.marshal()?;
        self.socket.send_to(&data, self.target).await?;

        println!("📤 Sent AssociationSetupRequest (seq: {})", seq);
        Ok(())
    }

    async fn establish_session(&self, seid: u64) -> Result<(), Box<dyn std::error::Error>> {
        let seq = self.next_sequence();
        let request = SessionEstablishmentRequest::builder()
            .sequence(seq)
            .seid(seid)
            .node_id(NodeId::new_ipv4([127, 0, 0, 1])?)
            .build()?;

        let data = request.marshal()?;
        self.socket.send_to(&data, self.target).await?;

        println!("📤 Sent SessionEstablishmentRequest (SEID: {:#x}, seq: {})", seid, seq);
        Ok(())
    }

    async fn delete_session(&self, seid: u64) -> Result<(), Box<dyn std::error::Error>> {
        let seq = self.next_sequence();
        let request = SessionDeletionRequest::builder()
            .sequence(seq)
            .seid(seid)
            .build()?;

        let data = request.marshal()?;
        self.socket.send_to(&data, self.target).await?;

        println!("📤 Sent SessionDeletionRequest (SEID: {:#x}, seq: {})", seid, seq);
        Ok(())
    }

    async fn wait_for_responses(&self, expected: usize, timeout: Duration) {
        let mut received = 0;
        let mut buf = vec![0u8; 65536];

        let deadline = tokio::time::Instant::now() + timeout;

        while received < expected && tokio::time::Instant::now() < deadline {
            match tokio::time::timeout_at(deadline, self.socket.recv_from(&mut buf)).await {
                Ok(Ok((len, src))) => {
                    if let Ok(msg) = rs_pfcp::message::parse(&buf[..len]) {
                        println!("📥 Received {:?} from {} (seq: {})", msg.msg_type(), src, msg.sequence());
                        received += 1;
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("❌ Receive error: {}", e);
                    break;
                }
                Err(_) => {
                    println!("⏱️  Timeout waiting for responses ({}/{} received)", received, expected);
                    break;
                }
            }
        }
    }
}

fn get_recovery_timestamp() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32
}

async fn run_heartbeat_test(
    ctx: &TestContext,
    count: u32,
    interval: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🧪 Running Heartbeat Test");
    println!("   Count: {}, Interval: {}s\n", count, interval);

    for i in 1..=count {
        ctx.send_heartbeat().await?;
        if i < count {
            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
    }

    println!("\n⏳ Waiting for responses...");
    ctx.wait_for_responses(count as usize, Duration::from_secs(5))
        .await;

    Ok(())
}

async fn run_session_test(
    ctx: &TestContext,
    count: u32,
    delete: bool,
    delay: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🧪 Running Session Test");
    println!("   Sessions: {}, Delete: {}\n", count, delete);

    let base_seid = 0x100000;
    let mut seids = Vec::new();

    // Establish sessions
    println!("📝 Phase 1: Establishing {} sessions", count);
    for i in 0..count {
        let seid = base_seid + i as u64;
        seids.push(seid);
        ctx.establish_session(seid).await?;
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }

    ctx.wait_for_responses(count as usize, Duration::from_secs(5))
        .await;

    // Delete sessions if requested
    if delete {
        println!("\n📝 Phase 2: Deleting {} sessions", count);
        for seid in &seids {
            ctx.delete_session(*seid).await?;
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        ctx.wait_for_responses(count as usize, Duration::from_secs(5))
            .await;
    }

    println!("\n✅ Session test completed");
    Ok(())
}

async fn run_load_balance_test(
    ctx: &TestContext,
    sessions: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🧪 Running Load Balance Test");
    println!("   Creating {} sessions to test distribution\n", sessions);

    let base_seid = 0x200000;

    for i in 0..sessions {
        let seid = base_seid + i as u64;
        ctx.establish_session(seid).await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    println!("\n⏳ Waiting for responses...");
    ctx.wait_for_responses(sessions as usize, Duration::from_secs(10))
        .await;

    println!("\n✅ Load balance test completed");
    println!("   Check proxy statistics to see session distribution across UPFs");
    Ok(())
}

async fn run_full_test(
    ctx: &TestContext,
    sessions: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🧪 Running Full Test Scenario");
    println!("   Testing all proxy features with {} sessions\n", sessions);

    // 1. Heartbeat
    println!("📝 Step 1: Sending heartbeats");
    for _ in 0..3 {
        ctx.send_heartbeat().await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    ctx.wait_for_responses(3, Duration::from_secs(2)).await;

    // 2. Association
    println!("\n📝 Step 2: Setting up association");
    ctx.send_association_setup().await?;
    ctx.wait_for_responses(1, Duration::from_secs(2)).await;

    // 3. Session establishment (tests load balancing)
    println!("\n📝 Step 3: Establishing {} sessions (load balancing)", sessions);
    let base_seid = 0x400000;
    let mut seids = Vec::new();
    for i in 0..sessions {
        let seid = base_seid + i as u64;
        seids.push(seid);
        ctx.establish_session(seid).await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    ctx.wait_for_responses(sessions as usize, Duration::from_secs(5))
        .await;

    // 4. Delete sessions
    println!("\n📝 Step 4: Deleting sessions");
    for seid in &seids {
        ctx.delete_session(*seid).await?;
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    ctx.wait_for_responses(sessions as usize, Duration::from_secs(5))
        .await;

    println!("\n✅ Full test scenario completed successfully");
    println!("   Check proxy statistics for detailed results");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("\n📱 Starting Test SMF Client");
    println!("   Target: {}", args.target);

    let ctx = TestContext::new(&args.bind, &args.target).await?;
    println!("   ✓ Bound to local address: {}\n", ctx.socket.local_addr()?);

    match args.command {
        Commands::Heartbeat { count, interval } => {
            run_heartbeat_test(&ctx, count, interval).await?;
        }
        Commands::Sessions {
            count,
            delete,
            delay,
        } => {
            run_session_test(&ctx, count, delete, delay).await?;
        }
        Commands::LoadBalance { sessions } => {
            run_load_balance_test(&ctx, sessions).await?;
        }
        Commands::Full { sessions } => {
            run_full_test(&ctx, sessions).await?;
        }
    }

    println!("\n✨ Test completed\n");
    Ok(())
}
