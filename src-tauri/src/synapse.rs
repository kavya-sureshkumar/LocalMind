// Phase 2 Synapse: each machine can run an `rpc-server` child process that
// llama.cpp on a remote host reaches via `--rpc host:port`. On top of Phase 1's
// manual start/stop, we now also:
//   - advertise the worker on the LAN via mDNS (`_localmind-synapse._tcp`)
//   - browse for other workers, emitting peer add/remove events to the UI
//   - expose a `restart_worker` so the host can flush worker VRAM on demand
//
// Auth + smart layer split + tok/s telemetry come in Phase 3.
use crate::{binaries, config};
use anyhow::{anyhow, Result};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UdpSocket;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

pub const DEFAULT_WORKER_PORT: u16 = 50052;
const SERVICE_TYPE: &str = "_localmind-synapse._tcp.local.";

// UDP broadcast beacon — runs alongside mDNS as a fallback for networks
// where multicast is blocked (very common on Wi-Fi). Worker sends every
// BEACON_INTERVAL; host listens on BEACON_PORT and ages peers out after
// PEER_TTL of silence.
const BEACON_PORT: u16 = 50053;
const BEACON_INTERVAL: Duration = Duration::from_secs(3);
const PEER_TTL: Duration = Duration::from_secs(15);
const BEACON_MAGIC: &str = "localmind-synapse/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BeaconPayload {
    /// Magic string so we can ignore stray UDP traffic on this port.
    magic: String,
    /// Stable per-machine ID so the host can dedupe.
    id: String,
    hostname: String,
    port: u16,
    version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SynapseWorkerStatus {
    pub running: bool,
    pub port: u16,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SynapsePeer {
    /// Stable mDNS instance name — used as the dedupe key.
    pub id: String,
    /// Human-friendly hostname (whatever the worker advertised).
    pub hostname: String,
    /// First resolvable address — usually the LAN IPv4 we want.
    pub address: String,
    pub port: u16,
    /// `address:port`, the exact string the host appends to `--rpc`.
    pub endpoint: String,
}

struct WorkerHandle {
    child: Option<Child>,
    port: u16,
    /// Owns the mDNS advertisement; dropping unregisters automatically.
    advertised: Option<ServiceInfo>,
    /// Beacon broadcaster task; cancelled when the worker stops.
    beacon: Option<JoinHandle<()>>,
}

/// Beacon entry tracked on the host side. We only emit a peer-added event
/// when we first hear from a worker, then refresh `last_seen` on each ping.
/// A janitor task removes entries that have gone silent for PEER_TTL.
struct BeaconEntry {
    peer: SynapsePeer,
    last_seen: Instant,
}

pub struct SynapseState {
    worker: Mutex<WorkerHandle>,
    /// Single shared mDNS daemon (advertise + browse share one socket).
    daemon: Mutex<Option<ServiceDaemon>>,
    /// Peers we've seen, keyed by mDNS instance name. Cached so the UI can
    /// re-render on demand without waiting for the next browse tick.
    peers: Mutex<HashMap<String, SynapsePeer>>,
    /// UDP-beacon peers, keyed by beacon `id`. Separate map so the janitor
    /// can age them out without touching mDNS-discovered ones (mDNS does
    /// its own TTL via ServiceRemoved events).
    beacons: Mutex<HashMap<String, BeaconEntry>>,
}

impl SynapseState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            worker: Mutex::new(WorkerHandle {
                child: None,
                port: DEFAULT_WORKER_PORT,
                advertised: None,
                beacon: None,
            }),
            daemon: Mutex::new(None),
            peers: Mutex::new(HashMap::new()),
            beacons: Mutex::new(HashMap::new()),
        })
    }

    pub async fn status(&self) -> SynapseWorkerStatus {
        let w = self.worker.lock().await;
        SynapseWorkerStatus {
            running: w.child.is_some(),
            port: w.port,
            pid: w.child.as_ref().and_then(|c| c.id()),
        }
    }

    pub async fn list_peers(&self) -> Vec<SynapsePeer> {
        // Merge mDNS-discovered peers with UDP-beacon peers, keyed by endpoint
        // so the same worker reachable via both routes shows up only once.
        let mut by_endpoint: HashMap<String, SynapsePeer> = HashMap::new();
        for p in self.peers.lock().await.values() {
            by_endpoint.insert(p.endpoint.clone(), p.clone());
        }
        for entry in self.beacons.lock().await.values() {
            by_endpoint
                .entry(entry.peer.endpoint.clone())
                .or_insert_with(|| entry.peer.clone());
        }
        by_endpoint.into_values().collect()
    }

    pub async fn stop_worker(&self) -> Result<()> {
        let mut w = self.worker.lock().await;
        if let Some(mut c) = w.child.take() {
            let _ = c.kill().await;
            let _ = c.wait().await;
        }
        if let Some(handle) = w.beacon.take() {
            handle.abort();
        }
        if let Some(info) = w.advertised.take() {
            if let Some(d) = self.daemon.lock().await.as_ref() {
                let _ = d.unregister(info.get_fullname());
            }
        }
        Ok(())
    }

    pub async fn start_worker(
        &self,
        app: &AppHandle,
        port: Option<u16>,
    ) -> Result<SynapseWorkerStatus> {
        // Make sure the llama.cpp bundle is unpacked — `rpc-server` ships in
        // the same archive as `llama-server`, so we trigger that download here
        // if the user toggles worker mode before they've ever loaded a model.
        binaries::ensure_llama_server(app).await?;

        self.stop_worker().await?;
        let port = port.unwrap_or(DEFAULT_WORKER_PORT);
        crate::llama::kill_orphan_on_port(port).await;

        let binary = config::rpc_server_path();
        if !binary.exists() {
            return Err(anyhow!(
                "rpc-server binary not found at {} — is the bundled llama.cpp build missing RPC support?",
                binary.display()
            ));
        }

        // Bind to 0.0.0.0 so other machines on the LAN can connect; the host
        // typed our address into its workers field. There's no auth on the RPC
        // wire (Phase 3 will fix that with an auth shim) — only run worker
        // mode on networks you control.
        let mut cmd = Command::new(&binary);
        cmd.arg("-H").arg("0.0.0.0");
        cmd.arg("-p").arg(port.to_string());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow!("failed to spawn rpc-server: {e}"))?;
        pipe_output(app, &mut child);

        // Advertise on mDNS so the host's Synapse page picks us up automatically.
        // Best-effort: if advertising fails (e.g. no multicast on this NIC, or
        // hostname has chars mdns-sd refuses) the worker still works, the host
        // just has to type the IP manually. Surface the failure as a synapse:log
        // line so the UI can show *why* discovery isn't happening.
        let advertised = match self.advertise(port).await {
            Ok(info) => {
                let addrs = info
                    .get_addresses()
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                let _ = app.emit(
                    "synapse:log",
                    serde_json::json!({
                        "stream": "stdout",
                        "line": format!(
                            "mDNS advertise OK: {} on {} (port {})",
                            info.get_fullname(),
                            if addrs.is_empty() { "<no addr>".to_string() } else { addrs },
                            port,
                        ),
                    }),
                );
                Some(info)
            }
            Err(e) => {
                let msg = format!("mDNS advertise failed: {e}");
                eprintln!("synapse: {msg}");
                let _ = app.emit(
                    "synapse:log",
                    serde_json::json!({ "stream": "stderr", "line": msg }),
                );
                None
            }
        };

        // Spawn the UDP-broadcast beacon. This runs in parallel with mDNS as a
        // fallback for networks where multicast is filtered (Wi-Fi APs with
        // client isolation, Windows machines on Public profile, corp Wi-Fi…).
        // Beacon uses 255.255.255.255, which is far more permissive on most
        // networks than 224.0.0.251 multicast.
        let beacon_handle = match spawn_beacon(app.clone(), port).await {
            Ok(h) => Some(h),
            Err(e) => {
                let msg = format!("UDP beacon failed: {e}");
                eprintln!("synapse: {msg}");
                let _ = app.emit(
                    "synapse:log",
                    serde_json::json!({ "stream": "stderr", "line": msg }),
                );
                None
            }
        };

        {
            let mut w = self.worker.lock().await;
            w.child = Some(child);
            w.port = port;
            w.advertised = advertised;
            w.beacon = beacon_handle;
        }

        let _ = app.emit("synapse:ready", serde_json::json!({ "port": port }));

        Ok(self.status().await)
    }

    /// Stop + start in one call so the worker frees VRAM and re-advertises.
    /// Useful when a previous host disconnected mid-inference and llama.cpp
    /// left buffers allocated.
    pub async fn restart_worker(
        &self,
        app: &AppHandle,
        port: Option<u16>,
    ) -> Result<SynapseWorkerStatus> {
        // If no port given, reuse the one we were last running on.
        let port = match port {
            Some(p) => Some(p),
            None => Some(self.worker.lock().await.port),
        };
        self.start_worker(app, port).await
    }

    /// Start browsing for `_localmind-synapse._tcp` peers. Idempotent — calling
    /// twice keeps the same daemon. Emits `synapse:peer-added` /
    /// `synapse:peer-removed` events as the LAN view changes.
    pub async fn start_discovery(self: &Arc<Self>, app: &AppHandle) -> Result<()> {
        let daemon = {
            let mut guard = self.daemon.lock().await;
            if guard.is_none() {
                *guard = Some(ServiceDaemon::new().map_err(|e| anyhow!("mdns daemon: {e}"))?);
            }
            guard.as_ref().unwrap().clone()
        };

        let receiver = daemon
            .browse(SERVICE_TYPE)
            .map_err(|e| anyhow!("mdns browse: {e}"))?;
        // Clone for the mDNS browse task; the original `app` is reused below
        // for the beacon listener + janitor.
        let app_mdns = app.clone();
        let me = self.clone();
        tokio::spawn(async move {
            let app = app_mdns;
            // mdns-sd's receiver is a flume channel; recv_async is awaitable.
            while let Ok(event) = receiver.recv_async().await {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        let id = info.get_fullname().to_string();

                        // Skip our own advertisement — nothing useful about
                        // adding ourselves to our own peer list.
                        let own_fullname = me
                            .worker
                            .lock()
                            .await
                            .advertised
                            .as_ref()
                            .map(|a| a.get_fullname().to_string());
                        if own_fullname.as_deref() == Some(id.as_str()) {
                            continue;
                        }

                        let port = info.get_port();
                        let address = info
                            .get_addresses()
                            .iter()
                            .filter(|a| a.is_ipv4())
                            .map(|a| a.to_string())
                            .next()
                            .unwrap_or_else(|| {
                                info.get_addresses()
                                    .iter()
                                    .map(|a| a.to_string())
                                    .next()
                                    .unwrap_or_default()
                            });
                        if address.is_empty() {
                            continue;
                        }
                        let hostname = info
                            .get_property_val_str("hostname")
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| info.get_hostname().to_string());
                        let endpoint = format!("{}:{}", address, port);
                        let peer = SynapsePeer {
                            id: id.clone(),
                            hostname,
                            address,
                            port,
                            endpoint,
                        };
                        me.peers.lock().await.insert(id, peer.clone());
                        let _ = app.emit("synapse:peer-added", &peer);
                    }
                    ServiceEvent::ServiceRemoved(_, fullname) => {
                        let removed = me.peers.lock().await.remove(&fullname).is_some();
                        if removed {
                            let _ = app.emit(
                                "synapse:peer-removed",
                                serde_json::json!({ "id": fullname }),
                            );
                        }
                    }
                    _ => {}
                }
            }
        });

        // UDP-beacon listener. Runs alongside mDNS so peers from either route
        // both surface in the UI (deduped by endpoint in `list_peers`). Bound
        // to 0.0.0.0:50053 — no special firewall coordination needed if the
        // user already let LocalMind through, since this is the same exe.
        match UdpSocket::bind(("0.0.0.0", BEACON_PORT)).await {
            Ok(sock) => {
                let _ = app.emit(
                    "synapse:log",
                    serde_json::json!({
                        "stream": "stdout",
                        "line": format!("UDP beacon listener on 0.0.0.0:{BEACON_PORT}"),
                    }),
                );
                let app_l = app.clone();
                let me_l = self.clone();
                tokio::spawn(async move { run_beacon_listener(me_l, app_l, sock).await });

                // Janitor: drop beacon entries after PEER_TTL of silence so a
                // worker that vanishes (machine slept, network dropped, app
                // killed) doesn't linger forever in the host's peer list.
                let app_j = app.clone();
                let me_j = self.clone();
                tokio::spawn(async move { run_beacon_janitor(me_j, app_j).await });
            }
            Err(e) => {
                let msg = format!("UDP beacon listener failed: {e}");
                eprintln!("synapse: {msg}");
                let _ = app.emit(
                    "synapse:log",
                    serde_json::json!({ "stream": "stderr", "line": msg }),
                );
            }
        }

        Ok(())
    }

    async fn advertise(&self, port: u16) -> Result<ServiceInfo> {
        // Spin up the daemon if start_discovery hasn't been called yet.
        let daemon = {
            let mut guard = self.daemon.lock().await;
            if guard.is_none() {
                *guard = Some(ServiceDaemon::new().map_err(|e| anyhow!("mdns daemon: {e}"))?);
            }
            guard.as_ref().unwrap().clone()
        };

        // Real hostname for the TXT record (UI display) — may contain spaces,
        // underscores, etc. Windows in particular often has hostnames mdns-sd's
        // strict validator rejects.
        let raw_host = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "localmind".to_string());

        // Sanitized hostname for the actual mDNS record: ASCII alnum + hyphen
        // only, falls back to `localmind` if everything was stripped. RFC 1123
        // labels also can't start/end with a hyphen, so we trim those too.
        let sanitized = sanitize_dns_label(&raw_host);
        let dns_host = if sanitized.is_empty() {
            "localmind".to_string()
        } else {
            sanitized
        };
        let instance = format!("{}-{}", dns_host, port);
        let mdns_host = format!("{}.local.", dns_host);

        let ip = local_ip_address::local_ip()
            .map(|ip| ip.to_string())
            .map_err(|e| anyhow!("local ip: {e}"))?;

        let mut props: HashMap<String, String> = HashMap::new();
        props.insert("hostname".to_string(), raw_host.clone());
        props.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());
        props.insert("kind".to_string(), "rpc-server".to_string());

        let info = ServiceInfo::new(
            SERVICE_TYPE,
            &instance,
            &mdns_host,
            ip.as_str(),
            port,
            Some(props),
        )
        .map_err(|e| anyhow!("mdns service info: {e}"))?;

        daemon
            .register(info.clone())
            .map_err(|e| anyhow!("mdns register: {e}"))?;
        Ok(info)
    }
}

/// Coerce an arbitrary hostname into a valid DNS label per RFC 1123:
/// ASCII letters, digits, and hyphens, no leading/trailing hyphen. Windows
/// hostnames may contain underscores or spaces which mdns-sd rejects.
fn sanitize_dns_label(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    cleaned.trim_matches('-').to_string()
}

/// Spawn the UDP-broadcast beacon for this worker. Sends a small JSON packet
/// to 255.255.255.255:BEACON_PORT every BEACON_INTERVAL. Returns a JoinHandle
/// the caller stores so it can `.abort()` on stop_worker.
async fn spawn_beacon(app: AppHandle, port: u16) -> Result<JoinHandle<()>> {
    let raw_host = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "localmind".to_string());
    let id = format!("beacon:{}-{}", sanitize_dns_label(&raw_host), port);
    let payload = BeaconPayload {
        magic: BEACON_MAGIC.to_string(),
        id,
        hostname: raw_host,
        port,
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let bytes = serde_json::to_vec(&payload).map_err(|e| anyhow!("beacon serialize: {e}"))?;

    // Bind to an ephemeral port; we only ever send. Setting broadcast on the
    // socket lets us hit the limited-broadcast address 255.255.255.255.
    let sock = UdpSocket::bind(("0.0.0.0", 0))
        .await
        .map_err(|e| anyhow!("beacon bind: {e}"))?;
    sock.set_broadcast(true)
        .map_err(|e| anyhow!("beacon set_broadcast: {e}"))?;

    let _ = app.emit(
        "synapse:log",
        serde_json::json!({
            "stream": "stdout",
            "line": format!(
                "UDP beacon broadcasting on 255.255.255.255:{BEACON_PORT} every {}s",
                BEACON_INTERVAL.as_secs(),
            ),
        }),
    );

    let handle = tokio::spawn(async move {
        loop {
            // Send to the limited broadcast address; routers won't forward it
            // off the LAN, which is exactly what we want.
            if let Err(e) = sock.send_to(&bytes, ("255.255.255.255", BEACON_PORT)).await {
                let _ = app.emit(
                    "synapse:log",
                    serde_json::json!({
                        "stream": "stderr",
                        "line": format!("beacon send failed: {e}"),
                    }),
                );
            }
            tokio::time::sleep(BEACON_INTERVAL).await;
        }
    });
    Ok(handle)
}

/// Receive UDP beacons forever. Each packet that parses cleanly and isn't from
/// our own beacon ID becomes a peer-added (or refresh) event.
async fn run_beacon_listener(state: Arc<SynapseState>, app: AppHandle, sock: UdpSocket) {
    let own_id = format!(
        "beacon:{}-{}",
        sanitize_dns_label(
            &hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "localmind".to_string())
        ),
        state.worker.lock().await.port
    );

    let mut buf = [0u8; 1024];
    loop {
        let (n, src) = match sock.recv_from(&mut buf).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        let payload: BeaconPayload = match serde_json::from_slice(&buf[..n]) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if payload.magic != BEACON_MAGIC {
            continue;
        }
        if payload.id == own_id {
            continue;
        }

        let address = src.ip().to_string();
        let endpoint = format!("{}:{}", address, payload.port);
        let peer = SynapsePeer {
            id: payload.id.clone(),
            hostname: payload.hostname.clone(),
            address,
            port: payload.port,
            endpoint,
        };

        // Insert or refresh. Only fire peer-added on first sight; refreshes
        // are silent so the UI doesn't churn every 3s.
        let mut beacons = state.beacons.lock().await;
        let is_new = !beacons.contains_key(&payload.id);
        beacons.insert(
            payload.id.clone(),
            BeaconEntry {
                peer: peer.clone(),
                last_seen: Instant::now(),
            },
        );
        drop(beacons);
        if is_new {
            let _ = app.emit("synapse:peer-added", &peer);
        }
    }
}

/// Periodically prune beacon entries whose last_seen is older than PEER_TTL.
/// Removed peers fire `synapse:peer-removed` so the UI can drop them from the
/// list without waiting for a full refresh.
async fn run_beacon_janitor(state: Arc<SynapseState>, app: AppHandle) {
    let mut ticker = tokio::time::interval(Duration::from_secs(2));
    loop {
        ticker.tick().await;
        let mut to_remove = Vec::new();
        {
            let now = Instant::now();
            let mut beacons = state.beacons.lock().await;
            beacons.retain(|id, entry| {
                if now.duration_since(entry.last_seen) > PEER_TTL {
                    to_remove.push(id.clone());
                    false
                } else {
                    true
                }
            });
        }
        for id in to_remove {
            let _ = app.emit("synapse:peer-removed", serde_json::json!({ "id": id }));
        }
    }
}

fn pipe_output(app: &AppHandle, child: &mut Child) {
    if let Some(stdout) = child.stdout.take() {
        let app = app.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app.emit(
                    "synapse:log",
                    serde_json::json!({ "stream": "stdout", "line": line }),
                );
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let app = app.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app.emit(
                    "synapse:log",
                    serde_json::json!({ "stream": "stderr", "line": line }),
                );
            }
        });
    }
}
